use lasco_core::crdt::{CrdtOperation, OperationContent};
use lasco_core::identifiers::RemoteUuid;
use lasco_core::library::sync::{PushMediaSource, remote_access::StorageRead};
use lasco_core::library_json::{
    DebugLocalAndroidConfig, DebugLocalAppleConfig, FixedPathConfig, LibraryJson, RemoteConfig,
    RemoteKind, UsbAndroidConfig, UsbAppleConfig, save_library,
};
use lasco_core::operations::{LibraryPassword, LibraryUsername};
use std::path::PathBuf;

use super::{
    FfiCrdtOperation, FfiDot, FfiKv, FfiLibrary, FfiMediaItem, FfiOperation, FfiRemote, ffi_count,
};
use crate::error::LascoError;
use crate::ids::FfiRemoteUuid;

fn next_media_fetch_priority(remote_count: usize) -> Result<u32, LascoError> {
    u32::try_from(remote_count).map_err(|_| LascoError::Other {
        msg: "remote count exceeds the persisted media-fetch priority range".to_string(),
    })
}

#[uniffi::export]
impl FfiLibrary {
    /// # Errors
    ///
    /// Returns an error if persisted local operations cannot be read or decoded.
    pub fn list_operations(&self) -> Result<Vec<FfiCrdtOperation>, LascoError> {
        Ok(self
            .inner
            .list_operations()?
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
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

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
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        save_library(&self.app_dir, &library_id, &lib_config)
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
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

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
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        save_library(&self.app_dir, &library_id, &lib_config)
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
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

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
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        save_library(&self.app_dir, &library_id, &lib_config)
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
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

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
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        save_library(&self.app_dir, &library_id, &lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().push(ffi_remote);
        Ok(remote_uuid.into())
    }

    /// # Errors
    ///
    /// Returns an error if `remote_id` is invalid or unknown, or the configuration update cannot be saved.
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
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;

        let index = lib_config
            .remotes
            .iter()
            .position(|r| r.remote_uuid == remote_uuid)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{}' not found", remote_id.value),
            })?;

        lib_config.remotes.remove(index);
        if lib_config.default_fetch_remote == Some(remote_uuid) {
            lib_config.default_fetch_remote = None;
        }
        save_library(&self.app_dir, &library_id, &lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

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
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let remote = lib_config
            .remotes
            .iter_mut()
            .find(|r| r.remote_uuid == remote_uuid)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{}' not found", remote_id.value),
            })?;

        remote.auto_push = enabled;
        save_library(&self.app_dir, &library_id, &lib_config)
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

    /// # Errors
    ///
    /// Returns an error if `remote_id` is invalid or unknown, or the configuration update cannot be saved.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cached remote-list mutex during the
    /// in-memory priority update after configuration is saved.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn set_remote_media_fetch_priority(
        &self,
        remote_id: FfiRemoteUuid,
        priority: u32,
    ) -> Result<(), LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;
        let remote_uuid: RemoteUuid = remote_id.clone().try_into()?;
        let remote = lib_config
            .remotes
            .iter_mut()
            .find(|remote| remote.remote_uuid == remote_uuid)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{}' not found", remote_id.value),
            })?;
        remote.media_fetch_priority = priority;
        save_library(&self.app_dir, &library_id, &lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        if let Some(remote) = self
            .remotes
            .lock()
            .unwrap()
            .iter_mut()
            .find(|remote| remote.remote_id == remote_id)
        {
            remote.media_fetch_priority = priority;
        }
        Ok(())
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
        let remote_id_string = remote_uuid.to_string();
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.push(storage.as_ref(), &remote_id_string))
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
        let target_remote_id_string = target_remote_uuid.to_string();
        let source_remote_id_string = source_remote_uuid.to_string();
        let target_storage =
            self.build_storage_for_remote(&target_remote_uuid, app_support_dir.as_deref())?;
        let source_storage =
            self.build_storage_for_remote(&source_remote_uuid, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.push_with_media_source(
                target_storage.as_ref(),
                &target_remote_id_string,
                PushMediaSource::FromRemote {
                    remote_id: &source_remote_id_string,
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
        let remote_id_string = remote_uuid.to_string();
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.fetch(storage.as_ref(), &remote_id_string))
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
        let remote_id_string = remote_uuid.to_string();
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move { inner.push(storage.as_ref(), &remote_id_string).await })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(ffi_count(report.ops_uploaded))
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
        let target_remote_id_string = target_remote_uuid.to_string();
        let source_remote_id_string = source_remote_uuid.to_string();
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
                        &target_remote_id_string,
                        PushMediaSource::FromRemote {
                            remote_id: &source_remote_id_string,
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
        let remote_id_string = remote_uuid.to_string();
        let storage = self.build_storage_for_remote(&remote_uuid, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move { inner.fetch(storage.as_ref(), &remote_id_string).await })
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
        if name.trim().is_empty() {
            return Err(LascoError::Other {
                msg: "remote name must not be empty".to_string(),
            });
        }

        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

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
        lib_config.remotes.push(remote_config);
        if is_first_remote {
            lib_config.default_fetch_remote = Some(remote_uuid);
        }
        save_library(&self.app_dir, &library_id, &lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().push(ffi_remote);
        Ok(remote_uuid.into())
    }

    pub(super) fn load_library_json(&self) -> Result<LibraryJson, LascoError> {
        let library_id = self.inner.library_id();
        LibraryJson::load(&self.app_dir, &library_id)?.ok_or(LascoError::NotFound)
    }

    pub(super) fn build_storage_for_remote(
        &self,
        remote_id: &RemoteUuid,
        app_support_dir: Option<&str>,
    ) -> Result<Box<dyn lasco_core::storage::Storage + Send + Sync>, LascoError> {
        let lib_config = self.load_library_json()?;

        let remote = lib_config
            .remotes
            .iter()
            .find(|remote| remote.remote_uuid == *remote_id)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{remote_id}' not found"),
            })?;

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
    let (kind, endpoint, bucket, region, path) = match &r.kind {
        RemoteKind::S3(s3) => (
            "s3".to_string(),
            Some(s3.endpoint.clone()),
            Some(s3.bucket.clone()),
            Some(s3.region.clone()),
            s3.path_prefix.clone(),
        ),
        RemoteKind::FixedPath(fs) => (
            "fixed_path".to_string(),
            None,
            None,
            None,
            Some(fs.root_dir.to_string_lossy().into_owned()),
        ),
        RemoteKind::UsbAndroid(_) => ("usb_android".to_string(), None, None, None, None),
        RemoteKind::UsbApple(_) => ("usb_apple".to_string(), None, None, None, None),
        RemoteKind::DebugLocalApple(cfg) => (
            "debug_local_apple".to_string(),
            None,
            None,
            None,
            Some(cfg.local_dir_name.clone()),
        ),
        RemoteKind::DebugLocalAndroid(cfg) => (
            "debug_local_android".to_string(),
            None,
            None,
            None,
            Some(cfg.local_dir_name.clone()),
        ),
    };
    FfiRemote {
        remote_id: r.remote_uuid.into(),
        name: r.name.clone(),
        auto_push: r.auto_push,
        media_fetch_priority: r.media_fetch_priority,
        exclude_from_media_fetch: r.exclude_from_media_fetch,
        kind,
        endpoint,
        bucket,
        region,
        path,
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
