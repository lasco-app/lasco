pub mod conflict;
pub mod compaction;
pub mod error;

pub(super) mod fetch;
pub(super) mod push;

use crate::error::{LibraryError, OperationError, SyncError};
use crate::identifiers::RemoteUuid;
use crate::library::Library;
use crate::storage::StorageError;

#[derive(Debug)]
pub struct SyncReportFetch {
    pub ops_downloaded: usize,
}

#[derive(Debug)]
pub struct SyncReportPush {
    pub ops_uploaded: usize,
    pub media_uploaded: usize,
    pub compactions_run: usize,
}

#[derive(Debug)]
pub struct SyncReport {
    pub fetch: SyncReportFetch,
    pub push: SyncReportPush,
}

impl Library {
    /// Copy local crypto files (`library/`) to the remote if not already present.
    /// Idempotent. Does nothing if `library/` already exists on the remote.
    pub async fn initialize_remote(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_uuid: RemoteUuid,
    ) -> Result<(), LibraryError> {
        let marker_key = format!("remote_id_{remote_uuid}");
        storage
            .put_if_absent(&marker_key, b"")
            .await
            .map_err(|e| LibraryError::Io(std::io::Error::other(e.to_string())))?;

        let existing = match storage.list("library/").await {
            Ok(keys) => keys,
            Err(crate::storage::StorageError::NotFound) => Vec::new(),
            Err(e) => return Err(LibraryError::Io(std::io::Error::other(e.to_string()))),
        };
        if !existing.is_empty() {
            return Ok(());
        }
        let lib_dir = self.inner.local_dirs.local_library_dir();
        for entry in std::fs::read_dir(&lib_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| std::io::Error::other("non-UTF8 filename in library dir"))?;
            let data = std::fs::read(&path)?;
            let key = format!("library/{filename}");
            storage.put(&key, &data).await
                .map_err(|e| LibraryError::Io(std::io::Error::other(e.to_string())))?;
        }
        Ok(())
    }

    pub async fn sync(&self, storage: &dyn crate::storage::Storage, remote_id: &str) -> Result<SyncReport, LibraryError> {
        let _remote_guard = self
            .try_acquire_remote_sync(remote_id)
            .ok_or(SyncError::AlreadyRunning)?;
        let fetch_report = {
            let _fetch_guard = self
                .try_acquire_fetch_slot()
                .ok_or(SyncError::AlreadyRunning)?;
            self.fetch_impl(storage, remote_id).await?
        };
        let push_report = self.push_impl(storage, remote_id).await?;
        Ok(SyncReport { fetch: fetch_report, push: push_report })
    }
}

/// Reads the remote's `remote_id_{uuid}` marker file and returns the UUID it holds.
pub async fn discover_remote_uuid(
    storage: &dyn crate::storage::Storage,
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
pub async fn verify_remote_identity(
    storage: &dyn crate::storage::Storage,
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
mod test_utils;

#[cfg(test)]
mod tests;
