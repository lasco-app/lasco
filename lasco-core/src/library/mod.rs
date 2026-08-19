pub mod albums;
pub mod error;
pub mod groups;
pub mod local_dirs;
mod local_ops_read_write;
pub mod media;
mod range;
mod remote_media_list_lock;
pub mod sync;
mod sync_policy;
pub mod user;

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use uuid::Uuid;

use crate::encryption::library_salt::{generate_salt, write_salt_file};
use crate::encryption::master_key::{
    MasterKey, find_master_key, generate_master_key, write_mk_file,
};
use crate::error::LibraryError;
use crate::identifiers::RemoteUuid;
use crate::library::local_dirs::LocalDirs;
use crate::library::local_ops_read_write::LocalOpsReadWriteLock;
use crate::library::remote_media_list_lock::RemoteMediaListLock;
use crate::library::sync_policy::{FetchSlotGuard, RemoteSyncGuard, SyncPolicy};
use crate::library_json::{LibraryJsonReadWrite, LibraryJsonReadWriteLock};
use crate::operations::{LibraryPassword, LibraryUsername};

pub use crate::identifiers::LibraryId;

/// Version of the on-disk library protocol. Increment when the layout is changed
/// in a backward-incompatible way.
pub const PROTOCOL_VERSION: u32 = 1;

/// Library format version written as a sentinel file (`local_state/library/version_{i}`) on init.
/// Increment it when the on-disk layout changes in a way an older build must not open.
pub const LIBRARY_FORMAT_VERSION: u32 = 1;

/// Sentinel filename for the current library format version.
#[must_use]
pub fn library_format_sentinel() -> String {
    format!("version_{LIBRARY_FORMAT_VERSION}")
}

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
    pub(crate) state: parking_lot::RwLock<crate::crdt::CrdtState>,
    pub(crate) sync_policy: SyncPolicy,
    pub(crate) username: LibraryUsername,
    /// Serializes synchronous reads and read-modify-write updates of this library's `library.json`.
    library_json_read_write_lock: LibraryJsonReadWriteLock,
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
    /// Opens exclusive synchronous access to this library's `library.json`.
    /// The returned object must not be held across an `.await`.
    pub fn library_json_read_write<'a>(&'a self, app_dir: &'a Path) -> LibraryJsonReadWrite<'a> {
        self.inner
            .library_json_read_write_lock
            .lock(app_dir, self.inner.library_id)
    }

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
    pub(crate) fn init(
        local_dirs: LocalDirs,
        library_id: LibraryId,
        device_id: crate::crdt::DeviceId,
        credentials: Credentials,
    ) -> Result<(Library, Uuid)> {
        let lib_dir = local_dirs.local_state_library_dir();
        std::fs::create_dir_all(lib_dir.path())?;

        let salt = generate_salt();
        write_salt_file(lib_dir.path(), salt)?;

        std::fs::write(lib_dir.path().join(library_format_sentinel()), b"")?;
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
        let initial_crdt = crate::crdt::CrdtState::new(device_id);
        crate::crdt::save_persisted(
            &local_dirs.local_state_crdt().snapshot_path(),
            &master_key,
            &initial_crdt,
        )?;

        let library = Library {
            inner: Arc::new(LibraryInner {
                master_key,
                library_id,
                local_dirs,
                username: credentials.username,
                state: parking_lot::RwLock::new(initial_crdt),
                sync_policy: SyncPolicy::new(),
                library_json_read_write_lock: LibraryJsonReadWriteLock::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        };
        Ok((library, password_uuid))
    }

    pub(crate) fn open(
        local_dirs: LocalDirs,
        device_id: crate::crdt::DeviceId,
        credentials: Credentials,
    ) -> Result<Library> {
        let lib_dir = local_dirs.local_state_library_dir();
        let sentinel_path = lib_dir.path().join(library_format_sentinel());
        if !sentinel_path.exists() {
            return Err(LibraryError::UnsupportedFormatVersion {
                found: "(unknown)".to_string(),
                expected: library_format_sentinel(),
            });
        }

        let (master_key, _password_uuid) = find_master_key(
            lib_dir.path(),
            &credentials.username.0,
            &credentials.password.0,
        )?;
        let local_ops_read_write_lock =
            LocalOpsReadWriteLock::new(local_dirs.local_state_operations());
        let mut loaded_crdt = crate::crdt::load_persisted(
            &local_dirs.local_state_crdt().snapshot_path(),
            &master_key,
            device_id,
        )?;
        loaded_crdt.set_device_id(device_id);
        reconcile_snapshot_with_operation_log(
            &mut loaded_crdt,
            &local_ops_read_write_lock,
            &local_dirs,
            &master_key,
        )?;
        Ok(Library {
            inner: Arc::new(LibraryInner {
                library_id: local_dirs.library_id(),
                master_key,
                local_dirs,
                username: credentials.username,
                state: parking_lot::RwLock::new(loaded_crdt),
                sync_policy: SyncPolicy::new(),
                library_json_read_write_lock: LibraryJsonReadWriteLock::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        })
    }

    /// Open with a pre-loaded `MasterKey` (session cache path).
    pub(crate) fn open_with_master_key(
        local_dirs: LocalDirs,
        master_key: MasterKey,
        library_id: LibraryId,
        device_id: crate::crdt::DeviceId,
        username: LibraryUsername,
    ) -> Result<Library> {
        let local_ops_read_write_lock =
            LocalOpsReadWriteLock::new(local_dirs.local_state_operations());
        let mut loaded_crdt = crate::crdt::load_persisted(
            &local_dirs.local_state_crdt().snapshot_path(),
            &master_key,
            device_id,
        )?;
        loaded_crdt.set_device_id(device_id);
        reconcile_snapshot_with_operation_log(
            &mut loaded_crdt,
            &local_ops_read_write_lock,
            &local_dirs,
            &master_key,
        )?;
        Ok(Library {
            inner: Arc::new(LibraryInner {
                master_key,
                library_id,
                local_dirs,
                username,
                state: parking_lot::RwLock::new(loaded_crdt),
                sync_policy: SyncPolicy::new(),
                library_json_read_write_lock: LibraryJsonReadWriteLock::new(),
                local_ops_read_write_lock,
                remote_media_list_lock: RemoteMediaListLock::new(),
            }),
        })
    }

    /// Rebuilds the disposable materialized snapshot from the durable operation log.
    ///
    /// The caller must have already authenticated and explicitly obtained user consent. The old
    /// snapshot is retained beside the replacement for diagnosis.
    pub(crate) fn recover_persisted_state(
        local_dirs: &LocalDirs,
        master_key: &MasterKey,
        device_id: crate::crdt::DeviceId,
    ) -> Result<()> {
        let snapshot_path = local_dirs.local_state_crdt().snapshot_path();
        if snapshot_path.exists() {
            let backup_path = snapshot_path.with_extension(format!(
                "enc.unrecoverable-{}",
                chrono::Utc::now().timestamp_millis()
            ));
            std::fs::rename(&snapshot_path, backup_path)?;
        }
        let operations = LocalOpsReadWriteLock::new(local_dirs.local_state_operations());
        let log = operations.lock(master_key).read_operations()?;
        let mut rebuilt = crate::crdt::CrdtState::new(device_id);
        rebuilt.merge_all(log.iter());
        crate::crdt::save_persisted(&snapshot_path, master_key, &rebuilt)?;
        Ok(())
    }

    #[must_use]
    pub fn library_id(&self) -> LibraryId {
        self.inner.library_id
    }

    #[must_use]
    pub fn username(&self) -> &LibraryUsername {
        &self.inner.username
    }

    #[must_use]
    pub fn master_key(&self) -> &MasterKey {
        &self.inner.master_key
    }

    #[must_use]
    pub fn protocol_version(&self) -> u32 {
        PROTOCOL_VERSION
    }

    /// Atomically records a local CRDT operation in `CrdtState` and the durable log.
    pub(crate) fn record_local_operation(
        &self,
        timestamp: chrono::DateTime<chrono::Utc>,
        content: crate::crdt::OperationContent,
    ) -> Result<()> {
        let mut state = self.inner.state.write();
        let crdt_operation = crate::crdt::CrdtOperation {
            dot: state.next_local_dot(),
            author: self.inner.username.clone(),
            timestamp,
            content,
        };
        // The append-only log is the operation source of truth for Push, so make the
        // operation durable there before replacing the derived state snapshot.
        self.local_ops_read_write()
            .append_operation(&crdt_operation)?;
        state.apply(&crdt_operation);
        crate::crdt::save_persisted(
            &self.inner.local_dirs.local_state_crdt().snapshot_path(),
            &self.inner.master_key,
            &state,
        )?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the local media cache directory cannot be read.
    pub fn pending_media_count(&self) -> Result<usize> {
        let count = self
            .local_ops_read_write()
            .read_operations()?
            .iter()
            .filter(|operation| {
                matches!(
                    operation.content,
                    crate::crdt::OperationContent::MediaCreation(_)
                )
            })
            .count();
        Ok(count)
    }

    /// # Errors
    ///
    /// Returns an error if the remote's synchronization metadata cannot be read.
    pub fn has_unpushed_changes(&self, remote_id: RemoteUuid) -> Result<bool> {
        let remote_last_known_state_dir = self
            .inner
            .local_dirs
            .remote_last_known_state_dir(&remote_id.to_string());
        let master_key = &self.inner.master_key;
        // The log is the complete operation history, including operations received
        // during Fetch that may still need delivery to this remote.
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

    /// # Errors
    ///
    /// Returns an error if locally persisted operations cannot be read or decoded.
    pub fn list_operations(&self) -> Result<Vec<crate::crdt::CrdtOperation>> {
        self.local_ops_read_write().read_operations()
    }

    /// Returns a newest-first slice of the persisted operation log.
    pub fn list_operations_range(
        &self,
        start_pos: u64,
        end_pos_exclusive: u64,
    ) -> Result<Vec<crate::crdt::CrdtOperation>> {
        self.local_ops_read_write()
            .read_operations_range(start_pos, end_pos_exclusive)
    }
}

/// The encrypted operation log is the recovery authority. Replaying it is
/// idempotent and repairs a snapshot that was not written after a successful
/// log append.
fn reconcile_snapshot_with_operation_log(
    crdt: &mut crate::crdt::CrdtState,
    operations: &LocalOpsReadWriteLock,
    local_dirs: &LocalDirs,
    master_key: &MasterKey,
) -> Result<()> {
    let log = operations.lock(master_key).read_operations()?;
    if log.is_empty() {
        return Ok(());
    }
    crdt.merge_all(log.iter());
    crate::crdt::save_persisted(
        &local_dirs.local_state_crdt().snapshot_path(),
        master_key,
        crdt,
    )?;
    Ok(())
}
