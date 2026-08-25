pub mod compaction;
pub mod conflict;
pub mod error;

pub mod remote_access;

pub(crate) mod media_inventory;

pub(super) mod fetch;
pub(super) mod push;

use crate::error::{LibraryError, OperationError, SyncError};
use crate::identifiers::{MediaUuid, RemoteUuid};
use crate::library::Library;
use crate::storage::{AtomicWriteMode, StorageError};
use fetch::{FetchAccess, fetch_impl};
use push::PushAccess;
use remote_access::{StorageRead, StorageReadWrite};
use std::collections::HashMap;

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

/// Receives the user-facing portion of Push progress.
///
/// Push determines the total only after it has confirmed the target's media
/// inventory. Both values count full media blobs, never thumbnails, operations,
/// or compaction work.
pub trait PushProgressObserver: Send + Sync {
    fn media_upload_progress(&self, uploaded: usize, total: usize);
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
        remote_id: RemoteUuid,
        storage: StorageRead<'a>,
    },
    Plan(PushMediaPlan<'a>),
}

/// The two blobs of one media. They are resolved, relayed and confirmed separately, since a
/// remote pushed to before a thumbnail existed holds the original without it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MediaBlob {
    Data,
    Thumb,
}

/// A fully resolved, per-blob relay plan. Core never reads a source outside this plan.
pub struct PushMediaPlan<'a> {
    pub assignments: HashMap<(MediaUuid, MediaBlob), PlannedMediaSource>,
    pub sources: HashMap<RemoteUuid, StorageRead<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub enum PlannedMediaSource {
    Local,
    Remote(RemoteUuid),
}

/// What push preparation resolved, from local knowledge alone.
///
/// A data blob with nowhere to read it from is reported in `unresolved_data` and must fail the
/// push before anything is uploaded. A thumbnail with nowhere to read it from is simply absent
/// from `assignments`, because nothing records whether a media ever had one.
#[derive(Debug, Default)]
pub struct PushMediaResolution {
    pub assignments: HashMap<(MediaUuid, MediaBlob), PlannedMediaSource>,
    pub unresolved_data: Vec<MediaUuid>,
}

impl PushMediaResolution {
    /// The remotes the plan names, which are the only ones the caller must build storage for.
    #[must_use]
    pub fn source_remote_ids(&self) -> std::collections::HashSet<RemoteUuid> {
        self.assignments
            .values()
            .filter_map(|source| match source {
                PlannedMediaSource::Local => None,
                PlannedMediaSource::Remote(id) => Some(*id),
            })
            .collect()
    }
}

impl std::fmt::Debug for PushMediaSource<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalOnly => formatter.write_str("PushMediaSource::LocalOnly"),
            Self::FromRemote { remote_id, .. } => formatter
                .debug_struct("PushMediaSource::FromRemote")
                .field("remote_id", remote_id)
                .finish_non_exhaustive(),
            Self::Plan(_) => formatter.write_str("PushMediaSource::Plan(..)"),
        }
    }
}

#[derive(Debug)]
pub struct SyncReport {
    pub fetch: SyncReportFetch,
    pub push: SyncReportPush,
}

impl Library {
    /// Confirms which media blobs `remote_id` holds and records them in its media inventory.
    ///
    /// This is media availability confirmation run on its own, which is how a client repairs
    /// incomplete knowledge of a remote without performing a full fetch. It lists media folders
    /// only, so it never reads an operation file and cannot become an implicit fetch.
    ///
    /// Returns how many blobs it newly confirmed.
    ///
    /// # Errors
    ///
    /// Returns an error if a sync is already running for this remote, or if the remote does not
    /// identify as belonging to this library.
    pub async fn confirm_remote_media(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
    ) -> Result<usize, LibraryError> {
        let remote_id_string = remote_id.to_string();
        let _remote_guard = self
            .try_acquire_remote_sync(&remote_id_string)
            .ok_or(SyncError::AlreadyRunning)?;
        let remote = StorageRead::new(storage);
        verify_remote_identity(&remote, remote_id).await?;

        let known_media: Vec<media_inventory::KnownMedia> = {
            let state = self.inner.state.read();
            state
                .media_entries()
                .iter()
                .map(|entry| media_inventory::KnownMedia {
                    media_id: entry.media_id,
                    storage_date: entry.storage_date,
                    expects_thumb: entry.companion_kind.is_none(),
                })
                .collect()
        };
        let remote_media_list = self.inner.local_dirs.remote_media_list(&remote_id_string);
        Ok(media_inventory::confirm_known_media(
            &remote,
            &known_media,
            &remote_id_string,
            &remote_media_list,
            &self.inner.remote_media_list_lock,
        )
        .await)
    }

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
            .put_atomic(&marker_key, b"", AtomicWriteMode::Replace)
            .await
            .map_err(|e| LibraryError::Io(std::io::Error::other(e.to_string())))?;

        let existing = match remote.list("library/").await {
            Ok(keys) => keys,
            Err(crate::storage::StorageError::NotFound) => Vec::new(),
            Err(e) => return Err(LibraryError::Io(std::io::Error::other(e.to_string()))),
        };
        if !existing.is_empty() {
            verify_remote_library_format_with_keys(&existing)?;
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
                .put_atomic(&key, &data, AtomicWriteMode::Replace)
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
        remote_id: RemoteUuid,
    ) -> Result<SyncReport, LibraryError> {
        let remote_id_string = remote_id.to_string();
        let _remote_guard = self
            .try_acquire_remote_sync(&remote_id_string)
            .ok_or(SyncError::AlreadyRunning)?;
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let local_state_library_dir = self.inner.local_dirs.local_state_library_dir();
        let remote_last_known_state_dir = self
            .inner
            .local_dirs
            .remote_last_known_state_dir(&remote_id_string);
        let remote_media_list = self.inner.local_dirs.remote_media_list(&remote_id_string);
        let remote_compact_op_id_merged_to_local = self
            .inner
            .local_dirs
            .remote_compact_op_id_merged_to_local(&remote_id_string);
        let local_state_crdt = self.inner.local_dirs.local_state_crdt();
        let fetch_report = {
            let _fetch_guard = self
                .try_acquire_fetch_slot()
                .ok_or(SyncError::AlreadyRunning)?;
            let remote = StorageRead::new(storage);
            fetch_impl(
                FetchAccess {
                    storage: &remote,
                    local_state_library_dir: &local_state_library_dir,
                    remote_last_known_state_dir: &remote_last_known_state_dir,
                    remote_media_list: &remote_media_list,
                    remote_compact_op_id_merged_to_local: &remote_compact_op_id_merged_to_local,
                    local_ops_read_write_lock: &self.inner.local_ops_read_write_lock,
                    remote_media_list_lock: &self.inner.remote_media_list_lock,
                },
                remote_id,
                self.inner.library_id,
                &self.inner.master_key,
                &self.inner.state,
                &local_state_crdt,
            )
            .await?
        };
        let remote = StorageReadWrite::new(storage);
        let push_report = self
            .push_impl(
                PushAccess {
                    storage: &remote,
                    local_state_media_dir: &local_state_media_dir,
                    remote_last_known_state_dir: &remote_last_known_state_dir,
                    remote_media_list: &remote_media_list,
                    local_state_library_dir: &local_state_library_dir,
                },
                remote_id,
                PushMediaSource::LocalOnly,
                None,
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

/// Verifies that the remote's `library/` directory carries the format sentinel this build
/// writes, given a listing of that directory.
///
/// # Errors
///
/// Returns an error if the sentinel is absent or names a different format version.
pub(crate) fn verify_remote_library_format_with_keys(keys: &[String]) -> Result<(), SyncError> {
    let expected = crate::library::library_format_sentinel();
    let basename = |key: &str| key.rsplit('/').next().unwrap_or(key).to_string();
    if keys.iter().any(|key| basename(key) == expected) {
        return Ok(());
    }
    let found = keys
        .iter()
        .map(|key| basename(key))
        .find(|name| name.starts_with("version_"))
        .unwrap_or_else(|| "(none)".to_string());
    Err(SyncError::UnsupportedRemoteFormat { found, expected })
}

/// Verifies that the remote's `library/` directory carries the format sentinel this build
/// writes, reading the remote directly.
///
/// # Errors
///
/// Returns an error if the remote is unreachable or the sentinel is absent.
pub(crate) async fn verify_remote_library_format(
    storage: &StorageRead<'_>,
) -> Result<(), SyncError> {
    let expected = crate::library::library_format_sentinel();
    let present = storage
        .exists(&format!("library/{expected}"))
        .await
        .map_err(SyncError::RemoteUnreachable)?;
    if present {
        return Ok(());
    }
    Err(SyncError::UnsupportedRemoteFormat {
        found: "(none)".to_string(),
        expected,
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
mod format_tests;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod tests;
