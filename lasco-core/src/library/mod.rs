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
use crate::library::local_dirs::LocalDirs;
use crate::library::local_ops_read_write::LocalOpsReadWriteLock;
use crate::library::remote_media_list_lock::RemoteMediaListLock;
use crate::library::sync_policy::{FetchSlotGuard, RemoteSyncGuard, SyncPolicy};
use crate::operations::{LibraryPassword, LibraryUsername};
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
    /// The sole lock that grants access to `operations.log`.
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
    pub(crate) async fn init(
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
                operation_state: parking_lot::RwLock::new(OperationState::default()),
                crdt_replica_state: parking_lot::RwLock::new(crdt_replica),
                sync_policy: SyncPolicy::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        };
        Ok((library, password_uuid))
    }

    pub(crate) async fn open(local_dirs: LocalDirs, credentials: Credentials) -> Result<Library> {
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
        reconcile_outbox_log(&local_ops_read_write_lock, &master_key, &crdt_replica)?;

        Ok(Library {
            inner: Arc::new(LibraryInner {
                library_id: local_dirs.library_id(),
                master_key,
                local_dirs,
                username: credentials.username,
                operation_state: parking_lot::RwLock::new(OperationState::default()),
                crdt_replica_state: parking_lot::RwLock::new(crdt_replica),
                sync_policy: SyncPolicy::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        })
    }

    /// Open with a pre-loaded `MasterKey` (session cache path).
    pub(crate) async fn open_with_master_key(
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
        reconcile_outbox_log(&local_ops_read_write_lock, &master_key, &crdt_replica)?;
        Ok(Library {
            inner: Arc::new(LibraryInner {
                master_key,
                library_id,
                local_dirs,
                username,
                operation_state: parking_lot::RwLock::new(OperationState::default()),
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

    /// Atomically records a local CRDT operation in canonical state/outbox and the durable log.
    pub(crate) fn record_local_operation(
        &self,
        timestamp: chrono::DateTime<chrono::Utc>,
        content: crate::crdt::OperationContent,
    ) -> Result<()> {
        let mut replica = self.inner.crdt_replica_state.write();
        let crdt_operation = crate::crdt::CrdtOperation {
            dot: replica.state.next_local_dot(),
            author: self.inner.username.clone(),
            timestamp,
            content,
        };
        replica.state.apply(&crdt_operation);
        replica.outgoing.push(crdt_operation.clone());
        crate::crdt::save_persisted(
            &self.inner.local_dirs.local_state_crdt().snapshot_path(),
            &self.inner.master_key,
            &replica,
        )?;
        // A crash after the snapshot save can leave this operation absent from the log;
        // recovery appends it from the durable outbox on the next local mutation/push.
        self.local_ops_read_write()
            .append_operation(&crdt_operation)?;
        Ok(())
    }

    pub async fn load_local_state(&self) -> Result<()> {
        let replica = self.inner.crdt_replica_state.read();
        let state = OperationState::from_reconstructed(replica.state.materialize());
        *self.inner.operation_state.write() = state;
        Ok(())
    }

    pub fn pending_media_count(&self) -> Result<u32> {
        let count = self
            .inner
            .crdt_replica_state
            .read()
            .outgoing
            .iter()
            .filter(|operation| {
                matches!(
                    operation.content,
                    crate::crdt::OperationContent::MediaCreation(_)
                )
            })
            .count();
        Ok(count as u32)
    }

    pub fn has_unpushed_changes(&self, remote_id: &str) -> Result<bool> {
        let remote_last_known_state_dir =
            self.inner.local_dirs.remote_last_known_state_dir(remote_id);
        let master_key = &self.inner.master_key;
        let local_ids = self.local_ops_read_write().known_dots()?;

        if local_ids.is_empty() {
            return Ok(false);
        }

        let remote_ids = crate::remote::last_known_state::collect_dots_from_dir(
            &remote_last_known_state_dir.operations_dir(),
            master_key,
        )
        .map_err(crate::library::sync::error::SyncError::LocalCacheCorrupt)?;

        Ok(!local_ids.is_subset(&remote_ids))
    }

    pub fn list_operations(&self) -> Result<Vec<crate::crdt::CrdtOperation>> {
        self.local_ops_read_write().read_operations()
    }
}

/// Repairs the only interrupted local-write state that can escape the normal
/// snapshot-then-append sequence. The snapshot's outbox is authoritative, so
/// appending any absent outgoing dots makes the log a complete merge oracle again.
fn reconcile_outbox_log(
    lock: &LocalOpsReadWriteLock,
    master_key: &MasterKey,
    replica: &crate::crdt::CrdtStateReplica,
) -> Result<()> {
    let mut log = lock.lock(master_key);
    let known = log.known_dots()?;
    for operation in &replica.outgoing {
        if !known.contains(&operation.dot) {
            log.append_operation(operation)?;
        }
    }
    Ok(())
}
