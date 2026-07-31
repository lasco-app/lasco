pub mod albums;
pub mod error;
pub mod groups;
pub mod local_dirs;
pub mod media;
pub mod sync;
mod sync_policy;
pub mod user;

use std::fmt;
use std::sync::Arc;

use uuid::Uuid;

use crate::encryption::master_key::{MasterKey, find_master_key, generate_master_key, write_mk_file};
use crate::encryption::library_salt::{generate_salt, write_salt_file};
use crate::error::LibraryError;
use crate::identifiers::OpUuid;
use crate::state::OperationState;
use crate::operations::local_ops as op_log;
use crate::operations::{LibraryPassword, LibraryUsername, Operation, OperationGroup};
use crate::library::local_dirs::LocalDirs;
use crate::library::sync_policy::{FetchSlotGuard, RemoteSyncGuard, SyncPolicy};

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
    pub(crate) sync_policy: SyncPolicy,
    pub(crate) username: LibraryUsername,
    /// Guards all reads/writes of pending.op and operations.log, held only across sync
    /// std::fs calls, never across an await.
    pub(crate) op_files_lock: parking_lot::Mutex<()>,
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
        let lib_dir = local_dirs.local_library_dir();
        std::fs::create_dir_all(&lib_dir)?;

        let salt = generate_salt();
        write_salt_file(&lib_dir, salt)?;

        std::fs::write(lib_dir.join(LIBRARY_FORMAT_SENTINEL), b"")?;
        std::fs::write(lib_dir.join(format!("library_id_{}", library_id.0)), b"")?;

        let master_key = generate_master_key();
        let password_uuid = Uuid::new_v4();
        write_mk_file(
            &lib_dir,
            &credentials.username.0,
            password_uuid,
            &master_key,
            salt,
            &credentials.password.0,
        )?;

        let library = Library {
            inner: Arc::new(LibraryInner {
                master_key,
                library_id,
                local_dirs,
                username: credentials.username,
                operation_state: parking_lot::RwLock::new(OperationState::build(&[])),
                sync_policy: SyncPolicy::new(),
                op_files_lock: parking_lot::Mutex::new(()),
            }),
        };
        Ok((library, password_uuid))
    }

    pub async fn open(
        local_dirs: LocalDirs,
        credentials: Credentials,
    ) -> Result<Library> {
        let lib_dir = local_dirs.local_library_dir();
        let sentinel_path = lib_dir.join(LIBRARY_FORMAT_SENTINEL);
        if !sentinel_path.exists() {
            return Err(LibraryError::UnsupportedFormatVersion {
                found: "(unknown)".to_string(),
                expected: LIBRARY_FORMAT_SENTINEL.to_string(),
            });
        }

        let (master_key, _password_uuid) =
            find_master_key(&lib_dir, &credentials.username.0, &credentials.password.0)?;

        Ok(Library {
            inner: Arc::new(LibraryInner {
                library_id: local_dirs.library_id(),
                master_key,
                local_dirs,
                username: credentials.username,
                operation_state: parking_lot::RwLock::new(OperationState::build(&[])),
                sync_policy: SyncPolicy::new(),
                op_files_lock: parking_lot::Mutex::new(()),
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
        Ok(Library {
            inner: Arc::new(LibraryInner {
                master_key,
                library_id,
                local_dirs,
                username,
                operation_state: parking_lot::RwLock::new(OperationState::build(&[])),
                sync_policy: SyncPolicy::new(),
                op_files_lock: parking_lot::Mutex::new(()),
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

    /// Serializes access to pending.op and operations.log. `f` must be sync (no `.await`
    /// inside), so the lock is never held across an await point.
    pub(crate) fn with_op_lock<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let _guard = self.inner.op_files_lock.lock();
        f()
    }

    /// Append a single operation to the pending group, creating it if needed.
    /// All local mutations go through this instead of writing directly to the main log.
    pub(crate) fn append_to_pending(&self, op: Operation) -> Result<()> {
        let local_dirs = &self.inner.local_dirs;
        let master_key = &self.inner.master_key;
        let pending_path = local_dirs.pending_op_path();

        self.with_op_lock(|| {
            let mut group = op_log::read_pending_op_group(&pending_path, master_key)?
                .unwrap_or_else(|| OperationGroup {
                    op_id: OpUuid::new(),
                    parent_op_id: None,
                    author: self.inner.username.clone(),
                    operations: vec![],
                });

            group.operations.push(op);
            op_log::write_pending_op_group(&pending_path, master_key, &group)?;
            Ok(())
        })
    }

    pub async fn load_local_state(&self) -> Result<()> {
        let local_dirs = &self.inner.local_dirs;
        let master_key = &self.inner.master_key;

        let groups = self.with_op_lock(|| {
            let mut groups = op_log::read_op_groups(&local_dirs.operations_log_path(), master_key)?;
            if let Some(pending) = op_log::read_pending_op_group(&local_dirs.pending_op_path(), master_key)? {
                groups.push(pending);
            }
            Ok(groups)
        })?;
        let state = OperationState::build(&groups);
        *self.inner.operation_state.write() = state;
        Ok(())
    }

    pub fn pending_media_count(&self) -> Result<u32> {
        let pending_path = self.inner.local_dirs.pending_op_path();
        let master_key = &self.inner.master_key;
        let group = self.with_op_lock(|| Ok(op_log::read_pending_op_group(&pending_path, master_key)?))?;
        let Some(group) = group else {
            return Ok(0);
        };
        let count = group.operations.iter()
            .filter(|op| matches!(op, Operation::MediaCreation { .. }))
            .count();
        Ok(count as u32)
    }

    pub fn has_unpushed_changes(&self, remote_id: &str) -> Result<bool> {
        let local_dirs = &self.inner.local_dirs;
        let master_key = &self.inner.master_key;

        let local_ids = self.with_op_lock(|| {
            let mut local_ids = op_log::read_op_ids(&local_dirs.operations_log_path())?;
            if let Some(pending) = op_log::read_pending_op_group(&local_dirs.pending_op_path(), master_key)? {
                local_ids.insert(pending.op_id);
            }
            Ok(local_ids)
        })?;

        if local_ids.is_empty() {
            return Ok(false);
        }

        let remote_ids = crate::remote::last_known_state::collect_group_ids_from_dir(
            &local_dirs.remote_ops_dir(remote_id),
            master_key,
        )
        .map_err(crate::library::sync::error::SyncError::LocalCacheCorrupt)?;

        Ok(!local_ids.is_subset(&remote_ids))
    }

    pub fn list_operation_groups(&self) -> Result<Vec<OperationGroup>> {
        self.with_op_lock(|| {
            let mut groups = op_log::read_op_groups(
                &self.inner.local_dirs.operations_log_path(),
                &self.inner.master_key,
            )?;
            if let Some(pending) = op_log::read_pending_op_group(
                &self.inner.local_dirs.pending_op_path(),
                &self.inner.master_key,
            )? {
                groups.push(pending);
            }
            Ok(groups)
        })
    }

}

#[cfg(test)]
mod tests;
