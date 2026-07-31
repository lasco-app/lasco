use std::path::PathBuf;

use lasco_core::identifiers::RemoteUuid;
use lasco_core::library_json::{save_library, DebugLocalAndroidConfig, DebugLocalAppleConfig, FixedPathConfig, LibraryJson, RemoteConfig, RemoteKind};
use lasco_core::operations::{LibraryPassword, LibraryUsername, Operation};

use super::{FfiKv, FfiLibrary, FfiMediaItem, FfiOperation, FfiOperationGroup, FfiRemote, FfiSyncResult};
use crate::error::LascoError;

fn parse_remote_uuid(remote_id: &str) -> Result<RemoteUuid, LascoError> {
    remote_id
        .parse::<uuid::Uuid>()
        .map(RemoteUuid::from_uuid)
        .map_err(|e| LascoError::Other { msg: format!("invalid remote id '{remote_id}': {e}") })
}

#[uniffi::export]
impl FfiLibrary {
    pub fn list_operation_groups(&self) -> Result<Vec<FfiOperationGroup>, LascoError> {
        let groups = self.inner.list_operation_groups()?;
        Ok(groups.into_iter().map(|g| {
            let ops: Vec<FfiOperation> = g.operations.into_iter().map(operation_to_ffi).collect();
            FfiOperationGroup {
                op_id: g.op_id.0.to_string(),
                parent_op_id: g.parent_op_id.map(|p| p.0.to_string()),
                operations: ops,
                author: g.author.0,
            }
        }).collect())
    }

    pub fn user_list(&self) -> Result<Vec<String>, LascoError> {
        let users = self
            .rt
            .block_on(self.inner.user_list())
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        Ok(users.into_iter().map(|u| u.0).collect())
    }

    pub fn user_add(&self, username: String, password: String) -> Result<(), LascoError> {
        self.rt
            .block_on(self.inner.user_add(LibraryUsername(username), LibraryPassword(password)))
            .map(|_uuid| ())
            .map_err(|e| LascoError::Other { msg: e.to_string() })
    }

    pub fn list_remotes(&self) -> Vec<FfiRemote> {
        self.remotes.lock().unwrap().clone()
    }

    pub fn add_remote_fixed_path(&self, name: String, path: String) -> Result<String, LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{}' already exists", name),
            });
        }

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name,
            auto_push: true,
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
        Ok(remote_uuid.to_string())
    }

    pub fn add_remote_debug_local_apple(&self, name: String) -> Result<String, LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{}' already exists", name),
            });
        }

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name: name.clone(),
            auto_push: true,
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
        Ok(remote_uuid.to_string())
    }

    pub fn add_remote_debug_local_android(&self, name: String) -> Result<String, LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{}' already exists", name),
            });
        }

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name: name.clone(),
            auto_push: true,
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
        Ok(remote_uuid.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_remote_s3(
        &self,
        name: String,
        endpoint: String,
        bucket: String,
        region: String,
        path_prefix: String,
        access_key: String,
        secret_key: String,
    ) -> Result<String, LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

        if lib_config.remotes.iter().any(|r| r.name == name) {
            return Err(LascoError::Other {
                msg: format!("remote '{}' already exists", name),
            });
        }

        let (secret_key_encrypted, secret_key_encryption_description) =
            lasco_core::s3_secret::encrypt_s3_secret_key(self.inner.master_key(), &secret_key)
                .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        let path_prefix = if path_prefix.is_empty() { None } else { Some(path_prefix) };

        let remote_uuid = RemoteUuid::new();
        let remote_config = RemoteConfig {
            remote_uuid,
            name,
            auto_push: true,
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
        Ok(remote_uuid.to_string())
    }

    pub fn remove_remote(&self, remote_id: String) -> Result<(), LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;

        let index = lib_config
            .remotes
            .iter()
            .position(|r| r.remote_uuid.to_string() == remote_id)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{}' not found", remote_id),
            })?;

        lib_config.remotes.remove(index);
        save_library(&self.app_dir, &library_id, &lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().retain(|r| r.id != remote_id);
        Ok(())
    }

    pub fn set_remote_auto_push(&self, remote_id: String, enabled: bool) -> Result<(), LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config = self.load_library_json()?;
        let remote = lib_config
            .remotes
            .iter_mut()
            .find(|r| r.remote_uuid.to_string() == remote_id)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{}' not found", remote_id),
            })?;

        remote.auto_push = enabled;
        save_library(&self.app_dir, &library_id, &lib_config)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        self.remotes.lock().unwrap().iter_mut().find(|r| r.id == remote_id).map(|r| r.auto_push = enabled);
        Ok(())
    }

    pub fn sync(&self, app_support_dir: Option<String>) -> Result<FfiSyncResult, LascoError> {
        let remote_id = self
            .sync_remote_id
            .clone()
            .ok_or_else(|| LascoError::Other { msg: "no remotes configured".to_string() })?;

        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.sync(storage.as_ref(), &remote_id))
            .map_err(LascoError::from)?;
        Ok(FfiSyncResult {
            pushed: report.push.ops_uploaded as u32,
            pulled: report.fetch.ops_downloaded as u32,
        })
    }

    pub fn push_remote(&self, remote_id: String, app_support_dir: Option<String>) -> Result<u32, LascoError> {
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.push(storage.as_ref(), &remote_id))
            .map_err(LascoError::from)?;
        Ok(report.ops_uploaded as u32)
    }

    pub fn fetch_remote(&self, remote_id: String, app_support_dir: Option<String>) -> Result<u32, LascoError> {
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let report = self
            .rt
            .block_on(self.inner.fetch(storage.as_ref(), &remote_id))
            .map_err(LascoError::from)?;
        Ok(report.ops_downloaded as u32)
    }

    pub async fn sync_async(&self, app_support_dir: Option<String>) -> Result<FfiSyncResult, LascoError> {
        let remote_id = self
            .sync_remote_id
            .clone()
            .ok_or_else(|| LascoError::Other { msg: "no remotes configured".to_string() })?;

        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move { inner.sync(storage.as_ref(), &remote_id).await })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(FfiSyncResult {
            pushed: report.push.ops_uploaded as u32,
            pulled: report.fetch.ops_downloaded as u32,
        })
    }

    pub async fn push_remote_async(&self, remote_id: String, app_support_dir: Option<String>) -> Result<u32, LascoError> {
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move { inner.push(storage.as_ref(), &remote_id).await })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(report.ops_uploaded as u32)
    }

    pub async fn fetch_remote_async(&self, remote_id: String, app_support_dir: Option<String>) -> Result<u32, LascoError> {
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let inner = self.inner.clone();
        let report = self
            .rt
            .spawn(async move { inner.fetch(storage.as_ref(), &remote_id).await })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(LascoError::from)?;
        Ok(report.ops_downloaded as u32)
    }

    pub fn connect_remote(&self, remote_id: String, app_support_dir: Option<String>) -> Result<(), LascoError> {
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        let remote_uuid = parse_remote_uuid(&remote_id)?;
        self.rt
            .block_on(lasco_core::library::sync::verify_remote_identity(storage.as_ref(), remote_uuid))
            .map_err(|e| LascoError::Other { msg: format!("remote unreachable: {e}") })?;
        Ok(())
    }

    pub fn initialize_remote(&self, remote_id: String, app_support_dir: Option<String>) -> Result<(), LascoError> {
        let lib_config = self.load_library_json()?;
        let remote_uuid = parse_remote_uuid(&remote_id)?;
        lasco_core::library_json::find_remote_by_uuid(&lib_config, &remote_uuid)
            .ok_or_else(|| LascoError::Other { msg: format!("remote '{}' not found", remote_id) })?;
        let storage = self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
        self.rt
            .block_on(self.inner.initialize_remote(storage.as_ref(), remote_uuid))
            .map_err(LascoError::from)
    }
}

impl FfiLibrary {
    pub(super) fn load_library_json(&self) -> Result<LibraryJson, LascoError> {
        let library_id = self.inner.library_id();
        LibraryJson::load(&self.app_dir, &library_id)?.ok_or(LascoError::NotFound)
    }

    pub(super) fn build_storage_for_remote(
        &self,
        remote_id: &str,
        app_support_dir: Option<&str>,
    ) -> Result<Box<dyn lasco_core::storage::Storage + Send + Sync>, LascoError> {
        let library_id = self.inner.library_id();
        let lib_config = self.load_library_json()?;

        // Find the requested remote and temporarily move it to the front so
        // build_storage (which always uses index 0) builds the right storage.
        let idx = lib_config
            .remotes
            .iter()
            .position(|r| r.remote_uuid.to_string() == remote_id)
            .ok_or_else(|| LascoError::Other {
                msg: format!("remote '{}' not found", remote_id),
            })?;
        let mut reordered = lib_config;
        reordered.remotes.swap(0, idx);

        lasco_core::client::build_storage(
            &self.app_dir,
            &reordered,
            &library_id,
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
        id: r.remote_uuid.to_string(),
        name: r.name.clone(),
        auto_push: r.auto_push,
        kind,
        endpoint,
        bucket,
        region,
        path,
    }
}

pub(super) fn media_entry_to_ffi(e: lasco_core::library::media::MediaEntry) -> FfiMediaItem {
    FfiMediaItem {
        media_id: e.media_id.to_string(),
        filename_original: e.filename_original.0,
        name: e.name.map(|n| n.0),
        date: e.date.to_rfc3339(),
        year: e.storage_date.year,
        month: e.storage_date.month,
        size_bytes: e.size_bytes,
        content_hash: e.content_hash.to_hex(),
        author: e.author,
        apple_aae_media_id: e.apple_aae_media_id.map(|id| id.to_string()),
        apple_live_photo_media_id: e.apple_live_photo_media_id.map(|id| id.to_string()),
    }
}

fn kv(key: &str, value: impl ToString) -> FfiKv {
    FfiKv { key: key.to_string(), value: value.to_string() }
}

fn opt_kv(key: &str, value: Option<impl ToString>) -> FfiKv {
    FfiKv { key: key.to_string(), value: value.map(|v| v.to_string()).unwrap_or_default() }
}

pub(super) fn operation_to_ffi(op: Operation) -> FfiOperation {
    match op {
        Operation::MediaCreation { timestamp, media_id, filename_original, date, storage_date, size_bytes, .. } => {
            FfiOperation {
                kind: "MediaCreation".to_string(),
                timestamp: timestamp.to_rfc3339(),
                args: vec![
                    kv("media_id", media_id),
                    kv("filename_original", filename_original),
                    kv("date", date.to_rfc3339()),
                    kv("year", storage_date.year),
                    kv("month", storage_date.month),
                    kv("size_bytes", size_bytes),
                ],
            }
        }
        Operation::MediaRename { timestamp, media_id, name, .. } => FfiOperation {
            kind: "MediaRename".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![
                kv("media_id", media_id),
                opt_kv("name", name),
            ],
        },
        Operation::MediaPropsUpdate { timestamp, media_id, key, value, .. } => FfiOperation {
            kind: "MediaPropsUpdate".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("media_id", media_id), kv("key", key), kv("value", value)],
        },
        Operation::AlbumCreation { timestamp, album_id, name, album_id_parent, .. } => FfiOperation {
            kind: "AlbumCreation".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![
                kv("album_id", album_id),
                kv("name", name),
                opt_kv("parent_id", album_id_parent),
            ],
        },
        Operation::AlbumMediaAdd { timestamp, album_id, media_id, .. } => FfiOperation {
            kind: "AlbumMediaAdd".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("album_id", album_id), kv("media_id", media_id)],
        },
        Operation::AlbumMediaRemove { timestamp, album_id, media_id, .. } => FfiOperation {
            kind: "AlbumMediaRemove".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("album_id", album_id), kv("media_id", media_id)],
        },
        Operation::AlbumDeletion { timestamp, album_id, .. } => FfiOperation {
            kind: "AlbumDeletion".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("album_id", album_id)],
        },
        Operation::AlbumRename { timestamp, album_id, name, .. } => FfiOperation {
            kind: "AlbumRename".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("album_id", album_id), kv("name", name)],
        },
        Operation::AlbumReparent { timestamp, album_id, new_parent_id, .. } => FfiOperation {
            kind: "AlbumReparent".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("album_id", album_id), opt_kv("new_parent_id", new_parent_id)],
        },
        Operation::AlbumThumbnailSet { timestamp, album_id, media_id, .. } => FfiOperation {
            kind: "AlbumThumbnailSet".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("album_id", album_id), opt_kv("media_id", media_id)],
        },
        Operation::GroupCreation { timestamp, group_id, album_id_parent, .. } => FfiOperation {
            kind: "GroupCreation".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![
                kv("group_id", group_id),
                kv("album_id_parent", album_id_parent),
            ],
        },
        Operation::GroupMediaAdd { timestamp, group_id, media_id, .. } => FfiOperation {
            kind: "GroupMediaAdd".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("group_id", group_id), kv("media_id", media_id)],
        },
        Operation::GroupMediaRemove { timestamp, group_id, media_id, .. } => FfiOperation {
            kind: "GroupMediaRemove".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("group_id", group_id), kv("media_id", media_id)],
        },
        Operation::GroupDeletion { timestamp, group_id, .. } => FfiOperation {
            kind: "GroupDeletion".to_string(),
            timestamp: timestamp.to_rfc3339(),
            args: vec![kv("group_id", group_id)],
        },
    }
}
