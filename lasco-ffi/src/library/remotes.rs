use futures_util::stream::{FuturesUnordered, StreamExt};
use lasco_core::crdt::{CrdtOperation, OperationContent};
use lasco_core::identifiers::RemoteUuid;
use lasco_core::library::sync::{
    PushMediaSource, PushProgressObserver, compaction,
    remote_access::{StorageRead, StorageReadWrite},
};
use lasco_core::library_json::{
    CloudS3Config, DebugLocalAndroidConfig, DebugLocalAppleConfig, FixedPathConfig, LibraryJson,
    RemoteConfig, RemoteKind, UsbAndroidConfig, UsbAppleConfig,
};
use lasco_core::operations::{LibraryPassword, LibraryUsername};
use lasco_core::storage::AtomicWriteMode;
use rand::RngCore;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use super::{
    FfiCompactionLockInfo, FfiCrdtOperation, FfiDot, FfiKv, FfiLibrary, FfiMediaItem, FfiOperation,
    FfiRemote, FfiUploadBenchmarkSample, ffi_count,
};
use crate::error::LascoError;
use crate::ids::FfiRemoteUuid;

/// Receives completed full-media upload progress for one Push invocation.
///
/// Calls arrive on Lasco's Rust runtime thread. Implementations must return quickly and must
/// marshal any UI work onto their platform's UI dispatcher.
#[uniffi::export(callback_interface)]
pub trait PushProgressSink: Send + Sync {
    fn upload_progress(&self, fraction: f64);
}

struct FfiPushProgressObserver {
    sink: Box<dyn PushProgressSink>,
}

impl PushProgressObserver for FfiPushProgressObserver {
    fn media_upload_progress(&self, uploaded: usize, total: usize) {
        debug_assert!(total > 0);
        self.sink.upload_progress(uploaded as f64 / total as f64);
    }
}

fn next_media_fetch_priority(remote_count: usize) -> Result<u32, LascoError> {
    u32::try_from(remote_count).map_err(|_| LascoError::Other {
        msg: "remote count exceeds the persisted media-fetch priority range".to_string(),
    })
}

const DEFAULT_IMPORTER_REMOTE_UPLOAD_CONCURRENCY: usize = 2;
const MAX_IMPORTER_REMOTE_UPLOAD_CONCURRENCY: usize = 5;
const MAX_BENCHMARK_BYTES: u64 = 16 * 1024 * 1024;

fn validate_benchmark_request(
    bytes_per_upload: u64,
    max_parallel_uploads: u8,
) -> Result<(), LascoError> {
    if bytes_per_upload == 0 || bytes_per_upload > MAX_BENCHMARK_BYTES {
        return Err(LascoError::Other {
            msg: "benchmark upload size must be between 1 byte and 16 MiB".to_string(),
        });
    }
    if !(1..=MAX_IMPORTER_REMOTE_UPLOAD_CONCURRENCY).contains(&usize::from(max_parallel_uploads)) {
        return Err(LascoError::Other {
            msg: "benchmark parallel uploads must be between 1 and 5".to_string(),
        });
    }
    Ok(())
}

async fn push_configured_media_sources(
    library: &FfiLibrary,
    target_remote_id: FfiRemoteUuid,
    app_support_dir: Option<String>,
    progress: Box<dyn PushProgressSink>,
    max_concurrent_media_uploads: usize,
) -> Result<u64, LascoError> {
    let target: RemoteUuid = target_remote_id.try_into()?;
    let config = library.load_library_json()?;
    let resolution = library
        .inner
        .resolve_push_media(target, &config.media_source_order)?;
    if !resolution.unresolved_data.is_empty() {
        return Err(LascoError::from(lasco_core::error::LibraryError::Sync(
            lasco_core::error::SyncError::MissingMediaOnConfiguredSources(
                resolution.unresolved_data,
            ),
        )));
    }

    // Only the remotes the plan names are opened. Push verifies each of them before
    // reading anything from it.
    let mut sources: HashMap<RemoteUuid, Box<dyn lasco_core::storage::Storage + Send + Sync>> =
        HashMap::new();
    for source_id in resolution.source_remote_ids() {
        sources.insert(
            source_id,
            library.build_storage_for_remote(&source_id, app_support_dir.as_deref())?,
        );
    }
    let target_storage = library.build_storage_for_remote(&target, app_support_dir.as_deref())?;
    let inner = library.inner.clone();
    let assignments = resolution.assignments;
    let progress = FfiPushProgressObserver { sink: progress };
    // The push runs on the runtime owned by this library, not on the foreign
    // executor driving this exported async function. Storage backends build
    // network clients that need a Tokio context.
    let report = library
        .rt
        .spawn(async move {
            let source_reads = sources
                .iter()
                .map(|(id, storage)| (*id, StorageRead::new(storage.as_ref())))
                .collect();
            inner
                .push_with_media_source_and_progress_with_concurrency(
                    target_storage.as_ref(),
                    target,
                    PushMediaSource::Plan(lasco_core::library::sync::PushMediaPlan {
                        assignments,
                        sources: source_reads,
                    }),
                    Some(&progress),
                    max_concurrent_media_uploads,
                )
                .await
        })
        .await
        .map_err(|e| LascoError::Other { msg: e.to_string() })?
        .map_err(LascoError::from)?;
    Ok(ffi_count(report.ops_uploaded))
}

#[uniffi::export]
impl FfiLibrary {
    /// Adds one Lasco Cloud storage destination. The core resolves and caches
    /// its short-lived S3 credentials when the remote is first used.
    pub fn add_remote_cloud_s3(
        &self,
        name: String,
        cloud_storage_id: String,
    ) -> Result<FfiRemoteUuid, LascoError> {
        if cloud_storage_id.trim().is_empty() {
            return Err(LascoError::Other {
                msg: "cloud storage id must not be empty".to_string(),
            });
        }
        if self.load_library_json()?.remotes.iter().any(|remote| {
            matches!(&remote.kind, RemoteKind::CloudS3(config) if config.cloud_storage_id == cloud_storage_id)
        }) {
            return Err(LascoError::Other { msg: "Lasco Cloud storage is already configured".to_string() });
        }
        self.add_remote_config(
            name,
            RemoteKind::CloudS3(CloudS3Config { cloud_storage_id }),
        )
    }

    /// # Errors
    ///
    /// Returns the newest-first half-open range `[start_pos, end_pos_exclusive)`.
    ///
    /// Returns an error if persisted local operations cannot be read or decoded.
    pub fn list_operations(
        &self,
        start_pos: u64,
        end_pos_exclusive: u64,
    ) -> Result<Vec<FfiCrdtOperation>, LascoError> {
        Ok(self
            .inner
            .list_operations_range(start_pos, end_pos_exclusive)?
            .into_iter()
            .map(crdt_operation_to_ffi)
            .collect())
    }

    /// # Errors
    ///
    /// Returns an error if user records cannot be read from local library state.
    pub fn user_list(&self) -> Result<Vec<String>, LascoError> {
        let users = self
            .rt
            .block_on(self.inner.user_list())
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        Ok(users.into_iter().map(|u| u.0).collect())
    }

    /// # Errors
    ///
    /// Returns an error if the user key or add-user operation cannot be persisted.
    pub fn user_add(&self, username: String, password: String) -> Result<(), LascoError> {
        self.rt
            .block_on(
                self.inner
                    .user_add(LibraryUsername(username), LibraryPassword(password)),
            )
            .map(|_uuid| ())
            .map_err(|e| LascoError::Other { msg: e.to_string() })
    }

    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex.
    pub fn list_remotes(&self) -> Vec<FfiRemote> {
        self.remotes.lock().unwrap().clone()
    }

    /// Returns the owner and creation time of this remote's compaction lock, if held.
    pub fn inspect_compaction_lock(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<Option<FfiCompactionLockInfo>, LascoError> {
        let remote_id: RemoteUuid = remote_id.try_into()?;
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let device_id = self.load_library_json()?.device_id.to_string();
        let info = self
            .rt
            .block_on(compaction::inspect_lock(&StorageRead::new(
                storage.as_ref(),
            )))
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?;
        Ok(info.map(|lock| FfiCompactionLockInfo {
            is_owned_by_current_device: lock.owner_device_id == device_id,
            owner_device_id: lock.owner_device_id,
            created_at: lock.created_at.to_rfc3339(),
        }))
    }

    /// Removes a compaction lock only when it still names this local device as its owner.
    /// The caller is responsible for obtaining explicit user confirmation before this call.
    pub fn remove_own_compaction_lock(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<bool, LascoError> {
        let remote_id: RemoteUuid = remote_id.try_into()?;
        let device_id = self.load_library_json()?.device_id.to_string();
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        self.rt
            .block_on(compaction::remove_lock_owned_by(
                &StorageReadWrite::new(storage.as_ref()),
                &device_id,
            ))
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
    }

    /// # Errors
    ///
    /// Returns an error if the name already exists or library configuration cannot be read or saved.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex during the
    /// in-memory update after configuration is saved.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn add_remote_fixed_path(
        &self,
        name: String,
        path: String,
    ) -> Result<FfiRemoteUuid, LascoError> {
        let library_json = self.library_json_read_write();
        let mut lib_config = library_json.read()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{name}' already exists"),
            });
        }

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name,
            auto_push: true,
            media_fetch_priority: next_media_fetch_priority(lib_config.remotes.len())?,
            exclude_from_media_fetch: false,
            kind: RemoteKind::FixedPath(FixedPathConfig {
                root_dir: PathBuf::from(&path),
            }),
        };

        let ffi_remote = remote_config_to_ffi(&remote_config);
        let is_first_remote = lib_config.remotes.is_empty();
        lib_config.media_source_order.push(remote_uuid);
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        library_json
            .write(&lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().push(ffi_remote);
        Ok(remote_uuid.into())
    }

    /// Add a wired USB drive selected through Android's Storage Access
    /// Framework. `tree_uri` is an opaque, persistable access grant.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty URI, duplicate name, or failed configuration persistence.
    pub fn add_remote_usb_android(
        &self,
        name: String,
        tree_uri: String,
    ) -> Result<FfiRemoteUuid, LascoError> {
        if tree_uri.trim().is_empty() {
            return Err(LascoError::Other {
                msg: "USB drive tree URI must not be empty".to_string(),
            });
        }
        self.add_remote_config(name, RemoteKind::UsbAndroid(UsbAndroidConfig { tree_uri }))
    }

    /// Add a wired USB drive selected through Apple's document picker.
    /// `bookmark_base64` is an opaque security-scoped bookmark.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty bookmark, duplicate name, or failed configuration persistence.
    pub fn add_remote_usb_apple(
        &self,
        name: String,
        bookmark_base64: String,
    ) -> Result<FfiRemoteUuid, LascoError> {
        if bookmark_base64.trim().is_empty() {
            return Err(LascoError::Other {
                msg: "USB drive bookmark must not be empty".to_string(),
            });
        }
        self.add_remote_config(
            name,
            RemoteKind::UsbApple(UsbAppleConfig { bookmark_base64 }),
        )
    }

    /// # Errors
    ///
    /// Returns an error for a duplicate name or failed library-configuration persistence.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex during the
    /// in-memory update after configuration is saved.
    pub fn add_remote_debug_local_apple(&self, name: String) -> Result<FfiRemoteUuid, LascoError> {
        let library_json = self.library_json_read_write();
        let mut lib_config = library_json.read()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{name}' already exists"),
            });
        }

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name: name.clone(),
            auto_push: true,
            media_fetch_priority: next_media_fetch_priority(lib_config.remotes.len())?,
            exclude_from_media_fetch: false,
            kind: RemoteKind::DebugLocalApple(DebugLocalAppleConfig {
                local_dir_name: name,
            }),
        };

        let ffi_remote = remote_config_to_ffi(&remote_config);
        let is_first_remote = lib_config.remotes.is_empty();
        lib_config.media_source_order.push(remote_uuid);
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        library_json
            .write(&lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().push(ffi_remote);
        Ok(remote_uuid.into())
    }

    /// # Errors
    ///
    /// Returns an error for a duplicate name or failed library-configuration persistence.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex during the
    /// in-memory update after configuration is saved.
    pub fn add_remote_debug_local_android(
        &self,
        name: String,
    ) -> Result<FfiRemoteUuid, LascoError> {
        let library_json = self.library_json_read_write();
        let mut lib_config = library_json.read()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{name}' already exists"),
            });
        }

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name: name.clone(),
            auto_push: true,
            media_fetch_priority: next_media_fetch_priority(lib_config.remotes.len())?,
            exclude_from_media_fetch: false,
            kind: RemoteKind::DebugLocalAndroid(DebugLocalAndroidConfig {
                local_dir_name: name,
            }),
        };

        let ffi_remote = remote_config_to_ffi(&remote_config);
        let is_first_remote = lib_config.remotes.is_empty();
        lib_config.media_source_order.push(remote_uuid);
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        library_json
            .write(&lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().push(ffi_remote);
        Ok(remote_uuid.into())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "The FFI contract exposes S3 connection settings as explicit scalar parameters."
    )]
    /// # Errors
    ///
    /// Returns an error for a duplicate name, failed secret-key encryption, or failed configuration persistence.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex during the
    /// in-memory update after configuration is saved.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn add_remote_s3(
        &self,
        name: String,
        endpoint: String,
        bucket: String,
        region: String,
        path_prefix: String,
        access_key: String,
        secret_key: String,
    ) -> Result<FfiRemoteUuid, LascoError> {
        let library_json = self.library_json_read_write();
        let mut lib_config = library_json.read()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{name}' already exists"),
            });
        }

        let (secret_key_encrypted, secret_key_encryption_description) =
            lasco_core::s3_secret::encrypt_s3_secret_key(self.inner.master_key(), &secret_key)
                .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        let path_prefix = if path_prefix.is_empty() {
            None
        } else {
            Some(path_prefix)
        };

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name,
            auto_push: true,
            media_fetch_priority: next_media_fetch_priority(lib_config.remotes.len())?,
            exclude_from_media_fetch: false,
            kind: RemoteKind::S3(lasco_core::library_json::S3Config {
                endpoint,
                bucket,
                region,
                path_prefix,
                access_key,
                secret_key_encrypted,
                secret_key_encryption_description,
            }),
        };

        let ffi_remote = remote_config_to_ffi(&remote_config);
        let is_first_remote = lib_config.remotes.is_empty();
        lib_config.media_source_order.push(remote_uuid);
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        library_json
            .write(&lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().push(ffi_remote);
        Ok(remote_uuid.into())
    }

    /// Adds an SMB 2/3 share. The password is encrypted with this library's
    /// master key and is never included in [`FfiRemote`].
    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    pub fn add_remote_smb(
        &self,
        name: String,
        server: String,
        port: u16,
        share: String,
        path_prefix: String,
        username: String,
        password: String,
        domain: Option<String>,
    ) -> Result<FfiRemoteUuid, LascoError> {
        let connection = lasco_core::storage::SmbConnectionConfig::new(
            &server,
            port,
            &share,
            (!path_prefix.trim().is_empty()).then_some(path_prefix.as_str()),
            &username,
            &password,
            domain.as_deref(),
        )
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let (password_encrypted, password_encryption_description) =
            lasco_core::smb_secret::encrypt_smb_password(self.inner.master_key(), &password)
                .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.add_remote_config(
            name,
            RemoteKind::Smb(lasco_core::library_json::SmbConfig {
                server: connection.server,
                port: connection.port,
                share: connection.share,
                path_prefix: connection.path_prefix,
                username: connection.username,
                domain: connection.domain,
                password_encrypted,
                password_encryption_description,
            }),
        )
    }

    /// Removes a remote from the configuration and deletes everything this client cached
    /// about it.
    ///
    /// # Errors
    ///
    /// Returns an error if `remote_id` is invalid or unknown, if the configuration update
    /// cannot be saved, or if a sync holds the remote, in which case the remote is already out
    /// of the configuration and its directory is deleted on the next open.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex during the
    /// in-memory removal after configuration is saved.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn remove_remote(&self, remote_id: FfiRemoteUuid) -> Result<(), LascoError> {
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;

        // The remote is claimed for the whole removal, so an operation holding it stops this
        // before anything is written and the configuration is left as it was. Dropping the
        // remote from the configuration inside that claim is what stops a later operation from
        // resolving it and recreating the directory about to be deleted.
        self.inner.forget_remote(remote_uuid, || {
            let library_json = self.library_json_read_write();
            let mut lib_config = library_json.read()?;

            let index = lib_config
                .remotes
                .iter()
                .position(|r| r.remote_uuid == remote_uuid)
                .ok_or_else(|| LascoError::Other {
                    msg: format!("remote '{}' not found", remote_id.value),
                })?;

            let cloud_storage_id = match &lib_config.remotes[index].kind {
                RemoteKind::CloudS3(config) => Some(config.cloud_storage_id.clone()),
                _ => None,
            };

            lib_config.remotes.remove(index);
            lib_config
                .media_source_order
                .retain(|id| *id != remote_uuid);
            if lib_config.default_fetch_remote == Some(remote_uuid) {
                lib_config.default_fetch_remote = None;
            }
            library_json
                .write(&lib_config)
                .map_err(|e| LascoError::Other { msg: e.to_string() })?;
            if let Some(cloud_storage_id) = cloud_storage_id {
                self.inner
                    .cloud_runtime()
                    .forget_storage_id(&cloud_storage_id);
            }
            Ok::<(), LascoError>(())
        })?;

        self.remotes
            .lock()
            .unwrap()
            .retain(|r| r.remote_id != remote_id);
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if `remote_id` is invalid or unknown, or the configuration update cannot be saved.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex during the
    /// in-memory auto-push update after configuration is saved.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn set_remote_auto_push(
        &self,
        remote_id: FfiRemoteUuid,
        enabled: bool,
    ) -> Result<(), LascoError> {
        let library_json = self.library_json_read_write();
        let mut lib_config = library_json.read()?;
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let remote = lib_config
            .remotes
            .iter_mut()
            .find(|r| r.remote_uuid == remote_uuid)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{}' not found", remote_id.value),
            })?;

        remote.auto_push = enabled;
        library_json
            .write(&lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        if let Some(remote) = self
            .remotes
            .lock()
            .unwrap()
            .iter_mut()
            .find(|r| r.remote_id == remote_id)
        {
            remote.auto_push = enabled;
        }
        Ok(())
    }

    /// Returns the ordered subset of remotes used to retrieve uncached originals.
    ///
    /// # Errors
    ///
    /// Returns an error if the library configuration cannot be read.
    pub fn get_media_source_order(&self) -> Result<Vec<FfiRemoteUuid>, LascoError> {
        let config = self.load_library_json()?;
        Ok(config
            .media_source_order
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Replaces the ordered subset of remotes used to retrieve uncached originals.
    ///
    /// An empty list is valid and disables remote media-source lookups. Every supplied ID must
    /// belong to a configured remote and may appear only once.
    ///
    /// # Errors
    ///
    /// Returns an error if an ID is invalid, unknown, duplicated, or the configuration cannot be
    /// saved.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn set_media_source_order(&self, remote_ids: Vec<FfiRemoteUuid>) -> Result<(), LascoError> {
        let library_json = self.library_json_read_write();
        let mut config = library_json.read()?;
        let configured: std::collections::HashSet<_> = config
            .remotes
            .iter()
            .map(|remote| remote.remote_uuid)
            .collect();
        let mut ordered = Vec::with_capacity(remote_ids.len());
        let mut seen = std::collections::HashSet::with_capacity(remote_ids.len());

        for remote_id in remote_ids {
            let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
            if !configured.contains(&remote_uuid) {
                return Err(LascoError::Other {
                    msg: format!("remote '{}' not found", remote_id.value),
                });
            }
            if !seen.insert(remote_uuid) {
                return Err(LascoError::Other {
                    msg: format!("remote '{}' appears more than once", remote_id.value),
                });
            }
            ordered.push(remote_uuid);
        }

        config.media_source_order = ordered;
        library_json
            .write(&config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })
    }

    /// # Errors
    ///
    /// Returns an error if the ID/configuration is invalid, storage cannot be built, or remote push fails.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn push_remote(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<u64, LascoError> {
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.push(storage.as_ref(), remote_uuid))
            .map_err(LascoError::from)?;
        Ok(ffi_count(report.ops_uploaded))
    }

    /// Push to `target_remote_id`, relaying absent local media from the selected
    /// configured source remote. Callers should only use this after an explicit
    /// user choice; ordinary and scheduled pushes remain local-only.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid IDs, unavailable remote storage, failed validation, or failed relay/upload.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn push_remote_from_remote(
        &self,
        target_remote_id: FfiRemoteUuid,
        source_remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<u64, LascoError> {
        let target_remote_uuid: RemoteUuid = target_remote_id.clone().try_into()?;
        let source_remote_uuid: RemoteUuid = source_remote_id.clone().try_into()?;
        let target_storage =
            self.build_storage_for_remote(&target_remote_uuid, app_support_dir.as_deref())?;
        let source_storage =
            self.build_storage_for_remote(&source_remote_uuid, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.push_with_media_source(
                target_storage.as_ref(),
                target_remote_uuid,
                PushMediaSource::FromRemote {
                    remote_id: source_remote_uuid,
                    storage: StorageRead::new(source_storage.as_ref()),
                },
            ))
            .map_err(LascoError::from)?;
        Ok(ffi_count(report.ops_uploaded))
    }

    /// # Errors
    ///
    /// Returns an error if the ID/configuration is invalid, storage cannot be built, or remote fetch fails.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn fetch_remote(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<u64, LascoError> {
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.fetch(storage.as_ref(), remote_uuid))
            .map_err(LascoError::from)?;
        Ok(ffi_count(report.ops_downloaded))
    }

    /// # Errors
    ///
    /// Returns an error if the ID/configuration is invalid, storage cannot be built, the task fails, or remote push fails.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub async fn push_remote_async(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<u64, LascoError> {
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move { inner.push(storage.as_ref(), remote_uuid).await })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(ffi_count(report.ops_uploaded))
    }

    /// Push using the ordered configured media sources. Preparation completes before core push
    /// starts, and reads nothing but local files: the media cache and the media inventories.
    ///
    /// # Errors
    ///
    /// Returns an error if the ID or configuration is invalid, storage cannot be built, some
    /// data blob has no known place to be read from, or the push itself fails.
    pub async fn push_remote_using_configured_media_sources_async(
        &self,
        target_remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
        progress: Box<dyn PushProgressSink>,
    ) -> Result<u64, LascoError> {
        push_configured_media_sources(
            self,
            target_remote_id,
            app_support_dir,
            progress,
            DEFAULT_IMPORTER_REMOTE_UPLOAD_CONCURRENCY,
        )
        .await
    }

    /// Push using configured media sources with a bounded number of concurrent full-media
    /// uploads for this target. A desktop importer should choose this after benchmarking the
    /// target and schedule several remotes independently.
    ///
    /// # Errors
    ///
    /// Returns an error for a concurrency outside 1 through 5, or for the same failures as
    /// [`Self::push_remote_using_configured_media_sources_async`].
    pub async fn push_remote_using_configured_media_sources_with_options_async(
        &self,
        target_remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
        progress: Box<dyn PushProgressSink>,
        max_concurrent_media_uploads: u8,
    ) -> Result<u64, LascoError> {
        let max_concurrent_media_uploads = usize::from(max_concurrent_media_uploads);
        if !(1..=MAX_IMPORTER_REMOTE_UPLOAD_CONCURRENCY).contains(&max_concurrent_media_uploads) {
            return Err(LascoError::Other {
                msg: "media upload concurrency must be between 1 and 5".to_string(),
            });
        }
        push_configured_media_sources(
            self,
            target_remote_id,
            app_support_dir,
            progress,
            max_concurrent_media_uploads,
        )
        .await
    }

    /// Measures one remote at each parallelism from one through `max_parallel_uploads`.
    /// Temporary random benchmark objects are removed before this method returns.
    ///
    /// The desktop importer runs this concurrently for selected remotes, then uses the result to
    /// select an individual remote upload limit and to compare aggregate throughput against the
    /// sum of isolated remote rates.
    ///
    /// # Errors
    ///
    /// Returns an error if the request is outside 1 through 16 MiB or 1 through 5 uploads,
    /// storage construction fails, or a temporary upload or cleanup fails.
    pub async fn benchmark_remote_upload_async(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
        bytes_per_upload: u64,
        max_parallel_uploads: u8,
    ) -> Result<Vec<FfiUploadBenchmarkSample>, LascoError> {
        validate_benchmark_request(bytes_per_upload, max_parallel_uploads)?;
        let remote_id: RemoteUuid = remote_id.try_into()?;
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let buffer_size = usize::try_from(bytes_per_upload).map_err(|_| LascoError::Other {
            msg: "benchmark upload size is too large for this platform".to_string(),
        })?;
        let mut payload = vec![0_u8; buffer_size];
        rand::rngs::OsRng.fill_bytes(&mut payload);
        let benchmark_id = uuid::Uuid::new_v4();

        self.rt
            .spawn(async move {
                let mut samples = Vec::with_capacity(usize::from(max_parallel_uploads));
                for parallel_uploads in 1..=max_parallel_uploads {
                    let keys: Vec<String> = (0..parallel_uploads)
                        .map(|ordinal| {
                            format!(
                                "lasco-importer-benchmark/{benchmark_id}/{parallel_uploads}/{ordinal}"
                            )
                        })
                        .collect();
                    let start = Instant::now();
                    let mut writes = FuturesUnordered::new();
                    for key in &keys {
                        writes.push(storage.put_atomic(key, &payload, AtomicWriteMode::Replace));
                    }
                    let mut upload_error = None;
                    while let Some(result) = writes.next().await {
                        if let Err(error) = result {
                            upload_error.get_or_insert(error);
                        }
                    }
                    let elapsed = start.elapsed();

                    // Attempt every deletion so a failure cannot strand the rest of this sample.
                    let mut cleanup_error = None;
                    for key in &keys {
                        if let Err(error) = storage.delete(key).await {
                            cleanup_error.get_or_insert(error);
                        }
                    }
                    if let Some(error) = cleanup_error {
                        return Err(LascoError::Storage {
                            msg: format!("benchmark cleanup failed: {error}"),
                        });
                    }
                    if let Some(error) = upload_error {
                        return Err(LascoError::Storage {
                            msg: format!("benchmark upload failed: {error}"),
                        });
                    }

                    let elapsed_millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
                    let elapsed_nanos = elapsed.as_nanos().max(1);
                    let total_bytes = bytes_per_upload.saturating_mul(u64::from(parallel_uploads));
                    let bytes_per_second = u64::try_from(
                        (u128::from(total_bytes) * 1_000_000_000) / elapsed_nanos,
                    )
                    .unwrap_or(u64::MAX);
                    samples.push(FfiUploadBenchmarkSample {
                        parallel_uploads,
                        bytes_per_upload,
                        elapsed_millis,
                        bytes_per_second,
                    });
                }
                Ok::<_, LascoError>(samples)
            })
            .await
            .map_err(|error| LascoError::Other {
                msg: format!("benchmark task failed: {error}"),
            })?
    }

    /// Confirms which media blobs a remote holds and records them in its media inventory,
    /// without fetching. Returns how many blobs it newly confirmed.
    ///
    /// # Errors
    ///
    /// Returns an error if the ID is invalid, storage cannot be built, a sync is already
    /// running for this remote, or the remote does not belong to this library.
    pub async fn confirm_remote_media_async(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<u64, LascoError> {
        let remote_uuid: RemoteUuid = remote_id.try_into()?;
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let confirmed = self
            .rt
            .spawn(async move {
                inner
                    .confirm_remote_media(storage.as_ref(), remote_uuid)
                    .await
            })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(ffi_count(confirmed))
    }

    /// # Errors
    ///
    /// Returns an error for invalid IDs, unavailable storage, task failure, failed validation, or failed relay/upload.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub async fn push_remote_from_remote_async(
        &self,
        target_remote_id: FfiRemoteUuid,
        source_remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<u64, LascoError> {
        let target_remote_uuid: RemoteUuid = target_remote_id.clone().try_into()?;
        let source_remote_uuid: RemoteUuid = source_remote_id.clone().try_into()?;
        let target_storage =
            self.build_storage_for_remote(&target_remote_uuid, app_support_dir.as_deref())?;
        let source_storage =
            self.build_storage_for_remote(&source_remote_uuid, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move {
                inner
                    .push_with_media_source(
                        target_storage.as_ref(),
                        target_remote_uuid,
                        PushMediaSource::FromRemote {
                            remote_id: source_remote_uuid,
                            storage: StorageRead::new(source_storage.as_ref()),
                        },
                    )
                    .await
            })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(ffi_count(report.ops_uploaded))
    }

    /// # Errors
    ///
    /// Returns an error if the ID/configuration is invalid, storage cannot be built, the task fails, or remote fetch fails.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub async fn fetch_remote_async(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<u64, LascoError> {
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move { inner.fetch(storage.as_ref(), remote_uuid).await })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(ffi_count(report.ops_downloaded))
    }

    /// # Errors
    ///
    /// Returns an error if the ID/configuration is invalid, storage cannot be built, or remote identity cannot be verified.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn connect_remote(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<(), LascoError> {
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let remote = lasco_core::library::sync::remote_access::StorageRead::new(storage.as_ref());
        self.rt
            .block_on(lasco_core::library::sync::verify_remote_identity(
                &remote,
                remote_uuid,
            ))
            .map_err(|e| LascoError::Other {
                msg: format!("remote unreachable: {e}"),
            })?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the ID is invalid or unknown, storage cannot be built, or remote initialization fails.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn initialize_remote(
        &self,
        remote_id: FfiRemoteUuid,
        app_support_dir: Option<String>,
    ) -> Result<(), LascoError> {
        let lib_config = self.load_library_json()?;
        let remote_uuid = remote_id.clone().try_into()?;
        lasco_core::library_json::find_remote_by_uuid(&lib_config, &remote_uuid).ok_or_else(
            || LascoError::Other {
                msg: format!("remote '{}' not found", remote_id.value),
            },
        )?;
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        self.rt
            .block_on(self.inner.initialize_remote(storage.as_ref(), remote_uuid))
            .map_err(LascoError::from)
    }
}

impl FfiLibrary {
    fn add_remote_config(
        &self,
        name: String,
        kind: RemoteKind,
    ) -> Result<FfiRemoteUuid, LascoError> {
        let library_json = self.library_json_read_write();
        if name.trim().is_empty() {
            return Err(LascoError::Other {
                msg: "remote name must not be empty".to_string(),
            });
        }

        let mut lib_config = library_json.read()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{name}' already exists"),
            });
        }

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name,
            auto_push: true,
            media_fetch_priority: next_media_fetch_priority(lib_config.remotes.len())?,
            exclude_from_media_fetch: false,
            kind,
        };
        let ffi_remote = remote_config_to_ffi(&remote_config);
        let is_first_remote = lib_config.remotes.is_empty();
        lib_config.media_source_order.push(remote_uuid);
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        library_json
            .write(&lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().push(ffi_remote);
        Ok(remote_uuid.into())
    }

    pub(super) fn load_library_json(&self) -> Result<LibraryJson, LascoError> {
        self.library_json_read_write().read().map_err(Into::into)
    }

    pub(super) fn library_json_read_write(
        &self,
    ) -> lasco_core::library_json::LibraryJsonReadWrite<'_> {
        self.inner.library_json_read_write(&self.app_dir)
    }

    pub(super) fn build_storage_for_remote(
        &self,
        remote_id: &RemoteUuid,
        app_support_dir: Option<&str>,
    ) -> Result<Box<dyn lasco_core::storage::Storage + Send + Sync>, LascoError> {
        #[cfg(test)]
        if let Some(storage) = self.test_remotes.lock().unwrap().get(remote_id).cloned() {
            return Ok(Box::new(storage));
        }

        let lib_config = self.load_library_json()?;

        let remote = lib_config
            .remotes
            .iter()
            .find(|remote| remote.remote_uuid == *remote_id)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{remote_id}' not found"),
            })?;

        if let RemoteKind::CloudS3(cloud) = &remote.kind {
            self.inner
                .cloud_runtime()
                .register_remote(*remote_id, cloud.cloud_storage_id.clone());
            return Ok(Box::new(lasco_core::storage::StorageLascoCloudS3::new(
                *remote_id,
                self.inner.cloud_runtime(),
            )));
        }

        lasco_core::client::build_storage(
            &self.app_dir,
            remote,
            Some(self.inner.master_key()),
            app_support_dir.map(std::path::Path::new),
        )
        .map_err(|e| LascoError::Other { msg: e.to_string() })
    }
}

pub(super) fn remote_config_to_ffi(r: &RemoteConfig) -> FfiRemote {
    let mut remote = FfiRemote {
        remote_id: r.remote_uuid.into(),
        name: r.name.clone(),
        auto_push: r.auto_push,
        kind: String::new(),
        endpoint: None,
        bucket: None,
        region: None,
        path: None,
        server: None,
        port: None,
        share: None,
        username: None,
        domain: None,
    };
    match &r.kind {
        RemoteKind::S3(s3) => {
            remote.kind = "s3".to_string();
            remote.endpoint = Some(s3.endpoint.clone());
            remote.bucket = Some(s3.bucket.clone());
            remote.region = Some(s3.region.clone());
            remote.path = s3.path_prefix.clone();
        }
        RemoteKind::Smb(smb) => {
            remote.kind = "smb".to_string();
            remote.server = Some(smb.server.clone());
            remote.port = Some(smb.port);
            remote.share = Some(smb.share.clone());
            remote.path = smb.path_prefix.clone();
            remote.username = Some(smb.username.clone());
            remote.domain = smb.domain.clone();
        }
        RemoteKind::CloudS3(cloud) => {
            remote.kind = "lasco_cloud_s3".to_string();
            remote.path = Some(cloud.cloud_storage_id.clone());
        }
        RemoteKind::FixedPath(fs) => {
            remote.kind = "fixed_path".to_string();
            remote.path = Some(fs.root_dir.to_string_lossy().into_owned());
        }
        RemoteKind::UsbAndroid(_) => remote.kind = "usb_android".to_string(),
        RemoteKind::UsbApple(_) => remote.kind = "usb_apple".to_string(),
        RemoteKind::DebugLocalApple(cfg) => {
            remote.kind = "debug_local_apple".to_string();
            remote.path = Some(cfg.local_dir_name.clone());
        }
        RemoteKind::DebugLocalAndroid(cfg) => {
            remote.kind = "debug_local_android".to_string();
            remote.path = Some(cfg.local_dir_name.clone());
        }
    }
    remote
}

#[cfg(test)]
mod importer_benchmark_tests {
    use super::*;

    fn contains_file(path: &std::path::Path) -> bool {
        std::fs::read_dir(path).unwrap().flatten().any(|entry| {
            let path = entry.path();
            path.is_file() || (path.is_dir() && contains_file(&path))
        })
    }

    #[test]
    fn benchmark_uploads_to_a_fixed_path_and_removes_temporary_objects() {
        let temp = tempfile::tempdir().unwrap();
        let app_dir = temp.path().join("app");
        let remote_dir = temp.path().join("remote");
        std::fs::create_dir_all(&remote_dir).unwrap();
        let app_dir_string = app_dir.to_string_lossy().into_owned();

        crate::library::ffi_create_library(
            "importer-benchmark".to_string(),
            "tester".to_string(),
            "password".to_string(),
            Some(app_dir_string.clone()),
        )
        .unwrap();
        let library = FfiLibrary::open(
            Some("importer-benchmark".to_string()),
            "tester".to_string(),
            "password".to_string(),
            Some(app_dir_string),
        )
        .unwrap();
        let remote = library
            .add_remote_fixed_path(
                "benchmark remote".to_string(),
                remote_dir.to_string_lossy().into_owned(),
            )
            .unwrap();

        let samples = library
            .rt
            .block_on(library.benchmark_remote_upload_async(remote, None, 1024, 3))
            .unwrap();

        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].parallel_uploads, 1);
        assert_eq!(samples[2].parallel_uploads, 3);
        assert!(samples.iter().all(|sample| sample.bytes_per_second > 0));
        assert!(!contains_file(&remote_dir.join("lasco-importer-benchmark")));
    }
}

pub(super) fn media_entry_to_ffi(e: lasco_core::library::media::MediaEntry) -> FfiMediaItem {
    FfiMediaItem {
        media_id: e.media_id.into(),
        filename_original: e.filename_original.0,
        name: e.name.map(|n| n.0),
        date: e.date.to_rfc3339(),
        year: e.storage_date.year,
        month: e.storage_date.month,
        size_bytes: e.size_bytes,
        content_hash: e.content_hash.to_hex(),
        author: e.author,
        trashed_by: e.trashed_by,
        trashed_at: e.trashed_at.map(|timestamp| timestamp.to_rfc3339()),
        apple_aae_media_id: e.apple_aae_media_id.map(Into::into),
        apple_live_photo_media_id: e.apple_live_photo_media_id.map(Into::into),
    }
}

fn kv(key: &str, value: &impl ToString) -> FfiKv {
    FfiKv {
        key: key.to_string(),
        value: value.to_string(),
    }
}

fn opt_kv(key: &str, value: Option<impl ToString>) -> FfiKv {
    FfiKv {
        key: key.to_string(),
        value: value.map(|v| v.to_string()).unwrap_or_default(),
    }
}

pub(super) fn crdt_operation_to_ffi(op: CrdtOperation) -> FfiCrdtOperation {
    FfiCrdtOperation {
        dot: FfiDot {
            lamport_counter: op.dot.lamport_counter,
            device_id: format!("{:032x}", op.dot.device_id.0),
        },
        author: op.author.0,
        operation: operation_to_ffi(op.content, op.timestamp.to_rfc3339()),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "One exhaustive conversion keeps the FFI representation aligned with every core operation variant."
)]
fn operation_to_ffi(op: OperationContent, timestamp: String) -> FfiOperation {
    match op {
        OperationContent::MediaCreation(creation) => FfiOperation {
            kind: "MediaCreation".to_string(),
            timestamp: timestamp.clone(),
            args: vec![
                kv("media_id", &creation.media_id),
                kv("filename_original", &creation.filename_original),
                kv("date", &creation.date.to_rfc3339()),
                kv("year", &creation.storage_date.year),
                kv("month", &creation.storage_date.month),
                kv("size_bytes", &creation.size_bytes),
            ],
        },
        OperationContent::ApplePhotosResourceOriginAdded(origin) => FfiOperation {
            kind: "ApplePhotosResourceOriginAdded".to_string(),
            timestamp: timestamp.clone(),
            args: vec![
                kv("media_id", &origin.media_id),
                kv("cloud_asset_id", &origin.cloud_asset_id),
                opt_kv("modification_date", origin.modification_date.map(|date| date.to_rfc3339())),
                kv("resource_type", &format!("{:?}", origin.resource_type)),
                kv("filename", &origin.filename),
            ],
        },
        OperationContent::ApplePhotosCollectionLinkAdded(link) => FfiOperation {
            kind: "ApplePhotosCollectionLinkAdded".to_string(),
            timestamp: timestamp.clone(),
            args: vec![
                kv("album_id", &link.album_id),
                kv("cloud_collection_id", &link.cloud_collection_id),
                kv("kind", &format!("{:?}", link.kind)),
            ],
        },
        OperationContent::MediaRename { media_id, name } => FfiOperation {
            kind: "MediaRename".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("media_id", &media_id), opt_kv("name", name)],
        },
        OperationContent::MediaPropsUpdate {
            media_id,
            key,
            value,
        } => FfiOperation {
            kind: "MediaPropsUpdate".to_string(),
            timestamp: timestamp.clone(),
            args: vec![
                kv("media_id", &media_id),
                kv("key", &key),
                kv("value", &value),
            ],
        },
        OperationContent::MediaTrashSet { media_id, trashed } => FfiOperation {
            kind: "MediaTrashSet".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("media_id", &media_id), kv("trashed", &trashed)],
        },
        OperationContent::MediaDeletion { media_ids } => FfiOperation {
            kind: "MediaDeletion".to_string(),
            timestamp: timestamp.clone(),
            args: media_ids
                .iter()
                .map(|media_id| kv("media_id", media_id))
                .collect(),
        },
        OperationContent::AlbumCreation {
            album_id,
            name,
            parent_id,
        } => FfiOperation {
            kind: "AlbumCreation".to_string(),
            timestamp: timestamp.clone(),
            args: vec![
                kv("album_id", &album_id),
                kv("name", &name),
                opt_kv("parent_id", parent_id),
            ],
        },
        OperationContent::AlbumMediaAdd { album_id, media_id } => FfiOperation {
            kind: "AlbumMediaAdd".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("album_id", &album_id), kv("media_id", &media_id)],
        },
        OperationContent::AlbumMediaRemove {
            album_id, media_id, ..
        } => FfiOperation {
            kind: "AlbumMediaRemove".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("album_id", &album_id), kv("media_id", &media_id)],
        },
        OperationContent::AlbumDeletion { album_id } => FfiOperation {
            kind: "AlbumDeletion".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("album_id", &album_id)],
        },
        OperationContent::AlbumRename { album_id, name } => FfiOperation {
            kind: "AlbumRename".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("album_id", &album_id), opt_kv("name", name)],
        },
        OperationContent::AlbumReparent {
            album_id,
            parent_id,
        } => FfiOperation {
            kind: "AlbumReparent".to_string(),
            timestamp: timestamp.clone(),
            args: vec![
                kv("album_id", &album_id),
                opt_kv("new_parent_id", parent_id),
            ],
        },
        OperationContent::AlbumThumbnailSet { album_id, media_id } => FfiOperation {
            kind: "AlbumThumbnailSet".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("album_id", &album_id), opt_kv("media_id", media_id)],
        },
        OperationContent::GroupCreation {
            group_id,
            parent_id,
        } => FfiOperation {
            kind: "GroupCreation".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("group_id", &group_id), kv("album_id_parent", &parent_id)],
        },
        OperationContent::GroupMediaAdd { group_id, media_id } => FfiOperation {
            kind: "GroupMediaAdd".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("group_id", &group_id), kv("media_id", &media_id)],
        },
        OperationContent::GroupMediaRemove {
            group_id, media_id, ..
        } => FfiOperation {
            kind: "GroupMediaRemove".to_string(),
            timestamp: timestamp.clone(),
            args: vec![kv("group_id", &group_id), kv("media_id", &media_id)],
        },
        OperationContent::GroupDeletion { group_id } => FfiOperation {
            kind: "GroupDeletion".to_string(),
            timestamp,
            args: vec![kv("group_id", &group_id)],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::operation_to_ffi;
    use lasco_core::crdt::{ApplePhotosCollectionKind, ApplePhotosCollectionLink, OperationContent};
    use lasco_core::identifiers::AlbumUuid;
    use lasco_core::operations::ApplePhotosCloudCollectionId;

    #[test]
    fn collection_link_operations_are_visible_in_ffi_operation_output() {
        let operation = operation_to_ffi(
            OperationContent::ApplePhotosCollectionLinkAdded(ApplePhotosCollectionLink {
                album_id: AlbumUuid::from_uuid(uuid::Uuid::new_v4()),
                cloud_collection_id: ApplePhotosCloudCollectionId("icloud-trip".into()),
                kind: ApplePhotosCollectionKind::Album,
            }),
            "2026-09-15T00:00:00Z".into(),
        );

        assert_eq!(operation.kind, "ApplePhotosCollectionLinkAdded");
        assert_eq!(operation.args[1].key, "cloud_collection_id");
        assert_eq!(operation.args[1].value, "icloud-trip");
        assert_eq!(operation.args[2].key, "kind");
        assert_eq!(operation.args[2].value, "Album");
    }
}
