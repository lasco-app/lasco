pub mod compaction;
pub mod conflict;
pub mod error;

pub mod remote_access;

pub(super) mod fetch;
pub(super) mod push;

use crate::error::{LibraryError, OperationError, SyncError};
use crate::identifiers::RemoteUuid;
use crate::library::Library;
use crate::storage::StorageError;
use fetch::fetch_impl;
use remote_access::{StorageRead, StorageReadWrite};

#[derive(Debug)]
pub struct SyncReportFetch {
    pub ops_downloaded: usize,
    /// True when this invocation merged a remote file and callers must rebuild state, even when
    /// every operation was already appended by an interrupted earlier invocation.
    pub(crate) local_state_rebuild_required: bool,
}

#[derive(Debug)]
pub struct SyncReportPush {
    pub ops_uploaded: usize,
    pub media_uploaded: usize,
    pub compactions_run: usize,
}

/// Controls how [`Library::push`] obtains media that is absent from the local cache.
///
/// The default intentionally does not download from another remote. This keeps Push from
/// becoming an implicit fetch and lets callers ask the user to select a source explicitly.
#[derive(Default)]
pub enum PushMediaSource<'a> {
    /// Upload only locally cached media. Missing files cause Push to return their IDs.
    #[default]
    LocalOnly,
    /// Relay missing media from exactly one verified, read-only remote.
    FromRemote {
        remote_id: &'a str,
        storage: StorageRead<'a>,
    },
}

impl std::fmt::Debug for PushMediaSource<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalOnly => formatter.write_str("PushMediaSource::LocalOnly"),
            Self::FromRemote { remote_id, .. } => formatter
                .debug_struct("PushMediaSource::FromRemote")
                .field("remote_id", remote_id)
                .finish_non_exhaustive(),
        }
    }
}

#[derive(Debug)]
pub struct SyncReport {
    pub fetch: SyncReportFetch,
    pub push: SyncReportPush,
}

impl Library {
    /// Copy local crypto files (`library/`) to the remote if not already present.
    /// Idempotent. Does nothing if `library/` already exists on the remote.
    ///
    /// # Errors
    ///
    /// Returns an error if the remote or local crypto directory cannot be read, or setup files cannot be written.
    pub async fn initialize_remote(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_uuid: RemoteUuid,
    ) -> Result<(), LibraryError> {
        let remote = StorageReadWrite::new(storage);
        let marker_key = format!("remote_id_{remote_uuid}");
        remote
            .put_if_absent(&marker_key, b"")
            .await
            .map_err(|e| LibraryError::Io(std::io::Error::other(e.to_string())))?;

        let existing = match remote.list("library/").await {
            Ok(keys) => keys,
            Err(crate::storage::StorageError::NotFound) => Vec::new(),
            Err(e) => return Err(LibraryError::Io(std::io::Error::other(e.to_string()))),
        };
        if !existing.is_empty() {
            return Ok(());
        }
        let lib_dir = self.inner.local_dirs.local_state_library_dir();
        for entry in std::fs::read_dir(lib_dir.path())? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| std::io::Error::other("non-UTF8 filename in library dir"))?;
            let data = std::fs::read(&path)?;
            let key = format!("library/{filename}");
            remote
                .put_atomic(&key, &data)
                .await
                .map_err(|e| LibraryError::Io(std::io::Error::other(e.to_string())))?;
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if another sync is active, or remote fetch/push, media transfer, or local state rebuilding fails.
    pub async fn sync(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: &str,
    ) -> Result<SyncReport, LibraryError> {
        let _remote_guard = self
            .try_acquire_remote_sync(remote_id)
            .ok_or(SyncError::AlreadyRunning)?;
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let local_state_library_dir = self.inner.local_dirs.local_state_library_dir();
        let remote_last_known_state_dir =
            self.inner.local_dirs.remote_last_known_state_dir(remote_id);
        let remote_media_list = self.inner.local_dirs.remote_media_list(remote_id);
        let remote_merged_remote_files =
            self.inner.local_dirs.remote_merged_remote_files(remote_id);
        let fetch_report = {
            let _fetch_guard = self
                .try_acquire_fetch_slot()
                .ok_or(SyncError::AlreadyRunning)?;
            let remote = StorageRead::new(storage);
            let report = fetch_impl(
                &remote,
                remote_id,
                self.inner.library_id,
                &local_state_library_dir,
                &remote_last_known_state_dir,
                &remote_media_list,
                &remote_merged_remote_files,
                &self.inner.local_ops_read_write_lock,
                &self.inner.remote_media_list_lock,
                &self.inner.master_key,
                &self.inner.crdt_replica_state,
                &self.inner.local_dirs.local_state_crdt(),
            )
            .await?;
            if report.local_state_rebuild_required {
                self.load_local_state().await?;
            }
            report
        };
        let remote = StorageReadWrite::new(storage);
        let push_report = self
            .push_impl(
                &remote,
                remote_id,
                &local_state_media_dir,
                &remote_last_known_state_dir,
                &remote_media_list,
                &local_state_library_dir,
                PushMediaSource::LocalOnly,
            )
            .await?;
        Ok(SyncReport {
            fetch: fetch_report,
            push: push_report,
        })
    }
}

/// Reads the remote's `remote_id_{uuid}` marker file and returns the UUID it holds.
pub(crate) async fn discover_remote_uuid(
    storage: &StorageRead<'_>,
) -> Result<RemoteUuid, SyncError> {
    let remote_files = storage
        .list("")
        .await
        .map_err(SyncError::RemoteUnreachable)?;

    remote_files
        .iter()
        .find_map(|k| {
            let name = k.rsplit('/').next().unwrap_or(k);
            name.strip_prefix("remote_id_")
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
        })
        .map(RemoteUuid::from_uuid)
        .ok_or_else(|| {
            SyncError::RemoteIdMismatch("remote is missing remote_id_{uuid} file".to_string())
        })
}

/// Verifies that the remote's `remote_id_{uuid}` marker file matches `expected`.
///
/// # Errors
///
/// Returns an error if the marker cannot be read, is missing, or names a different remote.
pub async fn verify_remote_identity(
    storage: &StorageRead<'_>,
    expected: RemoteUuid,
) -> Result<(), SyncError> {
    let remote_uuid = discover_remote_uuid(storage).await?.0;

    if remote_uuid != expected.0 {
        return Err(SyncError::RemoteIdMismatch(format!(
            "remote={remote_uuid} expected={}",
            expected.0
        )));
    }

    Ok(())
}

/// Maps an op read/write failure to a `SyncError`, preserving the underlying storage error
/// where there is one.
pub(super) fn map_op_err(error: OperationError) -> SyncError {
    match error {
        OperationError::Storage(storage_error) => SyncError::RemoteUnreachable(storage_error),
        other => SyncError::RemoteUnreachable(StorageError::Other(Box::new(other))),
    }
}

#[cfg(test)]
mod crdt_tests;
#[cfg(test)]
mod test_utils;
