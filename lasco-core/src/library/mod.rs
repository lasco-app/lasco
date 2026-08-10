pub mod albums;
pub mod error;
pub mod groups;
pub mod local_dirs;
mod local_ops_read_write;
pub mod media;
mod remote_media_list_lock;
pub mod sync;
mod sync_policy;
pub mod user;

use std::fmt;
use std::sync::Arc;

use uuid::Uuid;

use crate::encryption::library_salt::{generate_salt, write_salt_file};
use crate::encryption::master_key::{
    MasterKey, find_master_key, generate_master_key, write_mk_file,
};
use crate::error::LibraryError;
use crate::identifiers::OpUuid;
use crate::library::local_dirs::LocalDirs;
use crate::library::local_ops_read_write::LocalOpsReadWriteLock;
use crate::library::remote_media_list_lock::RemoteMediaListLock;
use crate::library::sync_policy::{FetchSlotGuard, RemoteSyncGuard, SyncPolicy};
use crate::operations::{LibraryPassword, LibraryUsername, Operation, OperationGroup};
use crate::state::OperationState;

pub use crate::identifiers::LibraryId;

/// Version of the on-disk library protocol. Increment when the layout is changed
/// in a backward-incompatible way.
pub const PROTOCOL_VERSION: u32 = 1;

/// Library format version written as a sentinel file (`local_state/library/version_{i}`) on init.
pub const LIBRARY_FORMAT_VERSION: u32 = 1;

/// Sentinel filename for the current library format version.
pub const LIBRARY_FORMAT_SENTINEL: &str = "version_1";

#[derive(Debug)]
pub struct Credentials {
    pub username: LibraryUsername,
    pub password: LibraryPassword,
}

pub type Result<T> = std::result::Result<T, LibraryError>;

pub(crate) struct LibraryInner {
    pub(crate) master_key: MasterKey,
    pub(crate) library_id: LibraryId,
    pub(crate) local_dirs: LocalDirs,
    pub(crate) operation_state: parking_lot::RwLock<OperationState>,
    /// Canonical state and the durable outgoing CRDT-operation outbox.
    pub(crate) crdt_replica_state: parking_lot::RwLock<crate::crdt::CrdtStateReplica>,
    pub(crate) sync_policy: SyncPolicy,
    pub(crate) username: LibraryUsername,
    /// The sole lock that grants access to `pending.op` and `operations.log`.
    pub(crate) local_ops_read_write_lock: LocalOpsReadWriteLock,
    /// Per-remote locks for synchronous `media_list.json` read-modify-write access.
    pub(crate) remote_media_list_lock: RemoteMediaListLock,
}

#[derive(Clone)]
pub struct Library {
    pub(crate) inner: Arc<LibraryInner>,
}

impl fmt::Debug for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Library")
            .field("library_id", &self.inner.library_id)
            .finish_non_exhaustive()
    }
}

fn _assert_send<T: Send>() {}
const _: () = {
    let _ = _assert_send::<Library>;
};

impl Library {
    pub(crate) fn try_acquire_remote_sync(&self, remote_id: &str) -> Option<RemoteSyncGuard<'_>> {
        self.inner.sync_policy.try_acquire_remote(remote_id)
    }

    pub(crate) fn try_acquire_fetch_slot(&self) -> Option<FetchSlotGuard<'_>> {
        self.inner.sync_policy.try_acquire_fetch_slot()
    }

    /// Initialize a new library with the given credentials.
    ///
    /// Writes crypto metadata (salt, sentinel, master-key file) to `local_state/library/`
    /// on the local filesystem. No remote storage is touched.
    pub async fn init(
        local_dirs: LocalDirs,
        library_id: LibraryId,
        credentials: Credentials,
    ) -> Result<(Library, Uuid)> {
        let lib_dir = local_dirs.local_state_library_dir();
        std::fs::create_dir_all(lib_dir.path())?;

        let salt = generate_salt();
        write_salt_file(lib_dir.path(), salt)?;

        std::fs::write(lib_dir.path().join(LIBRARY_FORMAT_SENTINEL), b"")?;
        std::fs::write(
            lib_dir.path().join(format!("library_id_{}", library_id.0)),
            b"",
        )?;

        let master_key = generate_master_key();
        let password_uuid = Uuid::new_v4();
        write_mk_file(
            lib_dir.path(),
            &credentials.username.0,
            password_uuid,
            &master_key,
            salt,
            &credentials.password.0,
        )?;
        let local_ops_read_write_lock =
            LocalOpsReadWriteLock::new(local_dirs.local_state_operations());
        let crdt_replica = crate::crdt::CrdtStateReplica {
            state: crate::crdt::CanonicalState::new(crate::crdt::DeviceId::random()),
            outgoing: Vec::new(),
        };
        crate::crdt::save_persisted(
            &local_dirs.local_state_crdt().snapshot_path(),
            &master_key,
            &crdt_replica,
        )?;

        let library = Library {
            inner: Arc::new(LibraryInner {
                master_key,
                library_id,
                local_dirs,
                username: credentials.username,
                operation_state: parking_lot::RwLock::new(OperationState::build(&[])),
                crdt_replica_state: parking_lot::RwLock::new(crdt_replica),
                sync_policy: SyncPolicy::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        };
        Ok((library, password_uuid))
    }

    pub async fn open(local_dirs: LocalDirs, credentials: Credentials) -> Result<Library> {
        let lib_dir = local_dirs.local_state_library_dir();
        let sentinel_path = lib_dir.path().join(LIBRARY_FORMAT_SENTINEL);
        if !sentinel_path.exists() {
            return Err(LibraryError::UnsupportedFormatVersion {
                found: "(unknown)".to_string(),
                expected: LIBRARY_FORMAT_SENTINEL.to_string(),
            });
        }

        let (master_key, _password_uuid) = find_master_key(
            lib_dir.path(),
            &credentials.username.0,
            &credentials.password.0,
        )?;
        let local_ops_read_write_lock =
            LocalOpsReadWriteLock::new(local_dirs.local_state_operations());
        let crdt_replica = crate::crdt::load_persisted(
            &local_dirs.local_state_crdt().snapshot_path(),
            &master_key,
            crate::crdt::DeviceId::random(),
        )?;

        Ok(Library {
            inner: Arc::new(LibraryInner {
                library_id: local_dirs.library_id(),
                master_key,
                local_dirs,
                username: credentials.username,
                operation_state: parking_lot::RwLock::new(OperationState::build(&[])),
                crdt_replica_state: parking_lot::RwLock::new(crdt_replica),
                sync_policy: SyncPolicy::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        })
    }

    /// Open with a pre-loaded MasterKey (session cache path).
    pub async fn open_with_master_key(
        local_dirs: LocalDirs,
        master_key: MasterKey,
        library_id: LibraryId,
        username: LibraryUsername,
    ) -> Result<Library> {
        let local_ops_read_write_lock =
            LocalOpsReadWriteLock::new(local_dirs.local_state_operations());
        let crdt_replica = crate::crdt::load_persisted(
            &local_dirs.local_state_crdt().snapshot_path(),
            &master_key,
            crate::crdt::DeviceId::random(),
        )?;
        Ok(Library {
            inner: Arc::new(LibraryInner {
                master_key,
                library_id,
                local_dirs,
                username,
                operation_state: parking_lot::RwLock::new(OperationState::build(&[])),
                crdt_replica_state: parking_lot::RwLock::new(crdt_replica),
                sync_policy: SyncPolicy::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        })
    }

    pub fn library_id(&self) -> LibraryId {
        self.inner.library_id
    }

    pub fn username(&self) -> &LibraryUsername {
        &self.inner.username
    }

    pub fn master_key(&self) -> &MasterKey {
        &self.inner.master_key
    }

    pub fn protocol_version(&self) -> u32 {
        PROTOCOL_VERSION
    }

    /// Append a single operation to the pending group, creating it if needed.
    /// All local mutations go through this instead of writing directly to the main log.
    pub(crate) fn append_to_pending(&self, op: Operation) -> Result<()> {
        self.record_local_crdt_operation(&op)?;
        let mut local_ops = self.local_ops_read_write();
        let mut group = local_ops.read_pending()?.unwrap_or_else(|| OperationGroup {
            op_id: OpUuid::new(),
            parent_op_id: None,
            author: self.inner.username.clone(),
            operations: vec![],
        });

        group.operations.push(op);
        local_ops.write_pending(&group)
    }

    /// Temporary mutation bridge: the legacy log remains available to the
    /// current sync implementation, while every new local change is already
    /// represented as one dot-tagged operation in the CRDT outbox.
    fn record_local_crdt_operation(&self, operation: &Operation) -> Result<()> {
        use crate::crdt::{CrdtOperation, MediaCreation, OperationContent};

        let mut replica = self.inner.crdt_replica_state.write();
        let dot = replica.state.next_local_dot();
        let content = match operation {
            Operation::MediaCreation {
                media_id,
                filename_original,
                date,
                storage_date,
                size_bytes,
                content_hash,
                modified_at,
                gps,
                apple_aae_media_id,
                apple_live_photo_media_id,
                ..
            } => OperationContent::MediaCreation(MediaCreation {
                media_id: *media_id,
                filename_original: filename_original.clone(),
                date: *date,
                storage_date: *storage_date,
                size_bytes: *size_bytes,
                content_hash: *content_hash,
                modified_at: *modified_at,
                gps: *gps,
                apple_aae_media_id: *apple_aae_media_id,
                apple_live_photo_media_id: *apple_live_photo_media_id,
            }),
            Operation::MediaRename { media_id, name, .. } => OperationContent::MediaRename {
                media_id: *media_id,
                name: name.clone(),
            },
            Operation::MediaPropsUpdate {
                media_id,
                key,
                value,
                ..
            } => OperationContent::MediaPropsUpdate {
                media_id: *media_id,
                key: key.clone(),
                value: value.clone(),
            },
            Operation::AlbumCreation {
                album_id,
                name,
                album_id_parent,
                ..
            } => OperationContent::AlbumCreation {
                album_id: *album_id,
                name: name.clone(),
                parent_id: *album_id_parent,
            },
            Operation::AlbumMediaAdd {
                album_id, media_id, ..
            } => OperationContent::AlbumMediaAdd {
                album_id: *album_id,
                media_id: *media_id,
            },
            Operation::AlbumMediaRemove {
                album_id, media_id, ..
            } => OperationContent::AlbumMediaRemove {
                album_id: *album_id,
                media_id: *media_id,
                observed: replica.state.album_member_dots(*album_id, *media_id),
            },
            Operation::AlbumDeletion { album_id, .. } => OperationContent::AlbumDeletion {
                album_id: *album_id,
            },
            Operation::AlbumRename { album_id, name, .. } => OperationContent::AlbumRename {
                album_id: *album_id,
                name: Some(name.clone()),
            },
            Operation::AlbumReparent {
                album_id,
                new_parent_id,
                ..
            } => OperationContent::AlbumReparent {
                album_id: *album_id,
                parent_id: *new_parent_id,
            },
            Operation::AlbumThumbnailSet {
                album_id, media_id, ..
            } => OperationContent::AlbumThumbnailSet {
                album_id: *album_id,
                media_id: *media_id,
            },
            Operation::GroupCreation {
                group_id,
                album_id_parent,
                ..
            } => OperationContent::GroupCreation {
                group_id: *group_id,
                parent_id: *album_id_parent,
            },
            Operation::GroupMediaAdd {
                group_id, media_id, ..
            } => OperationContent::GroupMediaAdd {
                group_id: *group_id,
                media_id: *media_id,
            },
            Operation::GroupMediaRemove {
                group_id, media_id, ..
            } => OperationContent::GroupMediaRemove {
                group_id: *group_id,
                media_id: *media_id,
                observed: replica.state.group_member_dots(*group_id, *media_id),
            },
            Operation::GroupDeletion { group_id, .. } => OperationContent::GroupDeletion {
                group_id: *group_id,
            },
        };
        let timestamp = match operation {
            Operation::MediaCreation { timestamp, .. }
            | Operation::MediaRename { timestamp, .. }
            | Operation::MediaPropsUpdate { timestamp, .. }
            | Operation::AlbumCreation { timestamp, .. }
            | Operation::AlbumMediaAdd { timestamp, .. }
            | Operation::AlbumMediaRemove { timestamp, .. }
            | Operation::AlbumDeletion { timestamp, .. }
            | Operation::AlbumRename { timestamp, .. }
            | Operation::AlbumReparent { timestamp, .. }
            | Operation::AlbumThumbnailSet { timestamp, .. }
            | Operation::GroupCreation { timestamp, .. }
            | Operation::GroupMediaAdd { timestamp, .. }
            | Operation::GroupMediaRemove { timestamp, .. }
            | Operation::GroupDeletion { timestamp, .. } => *timestamp,
        };
        let crdt_operation = CrdtOperation {
            dot,
            author: self.inner.username.clone(),
            timestamp,
            content,
        };
        replica.state.apply(&crdt_operation);
        replica.outgoing.push(crdt_operation);
        crate::crdt::save_persisted(
            &self.inner.local_dirs.local_state_crdt().snapshot_path(),
            &self.inner.master_key,
            &replica,
        )?;
        Ok(())
    }

    pub async fn load_local_state(&self) -> Result<()> {
        let replica = self.inner.crdt_replica_state.read();
        let state = OperationState::from_reconstructed(replica.state.materialize());
        *self.inner.operation_state.write() = state;
        Ok(())
    }

    pub fn pending_media_count(&self) -> Result<u32> {
        let group = self.local_ops_read_write().read_pending()?;
        let Some(group) = group else {
            return Ok(0);
        };
        let count = group
            .operations
            .iter()
            .filter(|op| matches!(op, Operation::MediaCreation { .. }))
            .count();
        Ok(count as u32)
    }

    pub fn has_unpushed_changes(&self, remote_id: &str) -> Result<bool> {
        let remote_last_known_state_dir =
            self.inner.local_dirs.remote_last_known_state_dir(remote_id);
        let master_key = &self.inner.master_key;
        let local_ids = {
            let local_ops = self.local_ops_read_write();
            let mut local_ids: std::collections::HashSet<_> = local_ops
                .read_log_groups()?
                .into_iter()
                .map(|group| group.op_id)
                .collect();
            if let Some(pending) = local_ops.read_pending()? {
                local_ids.insert(pending.op_id);
            }
            local_ids
        };

        if local_ids.is_empty() {
            return Ok(false);
        }

        let remote_ids = crate::remote::last_known_state::collect_group_ids_from_dir(
            &remote_last_known_state_dir.operations_dir(),
            master_key,
        )
        .map_err(crate::library::sync::error::SyncError::LocalCacheCorrupt)?;

        Ok(!local_ids.is_subset(&remote_ids))
    }

    pub fn list_operation_groups(&self) -> Result<Vec<OperationGroup>> {
        let local_ops = self.local_ops_read_write();
        let mut groups = local_ops.read_log_groups()?;
        if let Some(pending) = local_ops.read_pending()? {
            groups.push(pending);
        }
        Ok(groups)
    }
}

#[cfg(test)]
mod tests;
