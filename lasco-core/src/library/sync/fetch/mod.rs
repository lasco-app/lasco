use std::collections::HashSet;

use crate::encryption::master_key::parse_mk_filename;
use crate::error::{LibraryError, SyncError};
use crate::identifiers::{LibraryId, RemoteUuid};
use crate::library::local_dirs::LocalDirs;
use crate::library::Library;
use crate::operations::local_ops as op_log;
use crate::operations::remote_ops::RemoteOpFile;
use crate::operations::{Operation, OperationGroup};
use crate::remote::{LastKnownState, MediaList, ProcessedFiles};

use super::{verify_remote_identity, SyncReportFetch};

impl Library {
    pub async fn fetch(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: &str,
    ) -> Result<SyncReportFetch, LibraryError> {
        let _remote_guard = self
            .try_acquire_remote_sync(remote_id)
            .ok_or(SyncError::AlreadyRunning)?;
        let _fetch_guard = self
            .try_acquire_fetch_slot()
            .ok_or(SyncError::AlreadyRunning)?;
        self.fetch_impl(storage, remote_id).await
    }

    pub(super) async fn fetch_impl(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: &str,
    ) -> Result<SyncReportFetch, LibraryError> {
        let local_dirs = &self.inner.local_dirs;
        let master_key = &self.inner.master_key;

        let remote_uuid = remote_id
            .parse::<uuid::Uuid>()
            .map(RemoteUuid::from_uuid)
            .map_err(|e| SyncError::RemoteIdMismatch(format!("invalid remote id '{remote_id}': {e}")))?;
        verify_remote_identity(storage, remote_uuid).await?;

        // Step 1: pull any mk_*.enc files present on the remote but missing locally,
        // so users added from another device become available on this one.
        fetch_library_dir(storage, local_dirs, self.inner.library_id).await?;

        // Load processed-file tracking for this remote.
        let processed_path = local_dirs.processed_files_path(remote_id);
        let mut processed = ProcessedFiles::load_or_default(&processed_path)?;

        // Load the media list for lazy update during merge.
        let media_list_path = local_dirs.remote_media_list_path(remote_id);
        let mut media_list = MediaList::load_or_default(&media_list_path)?;
        let mut media_list_changed = false;

        let last_known_state = LastKnownState::download(storage, local_dirs, remote_id, &processed, master_key).await?;

        let local_valid_ids =
            self.with_op_lock(|| Ok(op_log::read_op_ids(&local_dirs.operations_log_path())?))?;

        let mut ops_downloaded = 0usize;
        let mut processed_changed = false;
        // Track op_ids appended this run to avoid double-appending within one merge pass
        let mut appended_this_run: HashSet<_> = HashSet::new();

        for file in last_known_state.files() {
            let file_uuid = LastKnownState::file_uuid(file);
            if processed.contains(&file_uuid) {
                continue;
            }
            match file {
                RemoteOpFile::Compaction { uuid, tier, op_count } => {
                    let compaction = last_known_state.read_compaction_file(master_key, uuid, *tier, *op_count)?;
                    for entry in &compaction.contents {
                        if !local_valid_ids.contains(&entry.op_id)
                            && !appended_this_run.contains(&entry.op_id)
                        {
                            self.with_op_lock(|| {
                                Ok(op_log::append_op_group(
                                    &local_dirs.operations_log_path(),
                                    master_key,
                                    &entry.group,
                                )?)
                            })?;
                            appended_this_run.insert(entry.op_id);
                            update_media_list_from_group(
                                storage,
                                &entry.group,
                                &mut media_list,
                                &mut media_list_changed,
                            )
                            .await?;
                            ops_downloaded += 1;
                        }
                    }
                    processed.insert(file_uuid);
                    processed_changed = true;
                }
            }
        }

        if processed_changed {
            processed.save(&processed_path)?;
        }

        if media_list_changed {
            media_list.save(&media_list_path)?;
        }

        if ops_downloaded > 0 {
            self.load_local_state().await?;
        }

        Ok(SyncReportFetch { ops_downloaded })
    }
}

/// Step 1 of fetch. Verifies the remote's `library/library_id_{uuid}` matches this
/// library, then downloads any `mk_*.enc` file present on the remote but missing
/// locally, so users added from another device become available on this one.
async fn fetch_library_dir(
    storage: &dyn crate::storage::Storage,
    local_dirs: &LocalDirs,
    library_id: LibraryId,
) -> Result<(), LibraryError> {
    let remote_files = storage
        .list("library/")
        .await
        .map_err(SyncError::RemoteUnreachable)?;

    let remote_uuid = remote_files
        .iter()
        .find_map(|k| {
            let name = k.rsplit('/').next().unwrap_or(k);
            name.strip_prefix("library_id_")
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
        })
        .ok_or_else(|| {
            SyncError::LibraryIdMismatch("remote is missing library_id_{uuid} file".to_string())
        })?;
    if remote_uuid != library_id.0 {
        return Err(SyncError::LibraryIdMismatch(format!(
            "remote={remote_uuid} local={}",
            library_id.0
        ))
        .into());
    }

    let remote_mk_names: Vec<&str> = remote_files
        .iter()
        .filter_map(|k| {
            let name = k.rsplit('/').next().unwrap_or(k);
            parse_mk_filename(name).map(|_| name)
        })
        .collect();

    let local_dir = local_dirs.local_library_dir();
    let local_mk_names: HashSet<String> = match std::fs::read_dir(&local_dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                parse_mk_filename(&name).map(|_| name)
            })
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashSet::new(),
        Err(e) => return Err(SyncError::Io(e).into()),
    };

    for name in remote_mk_names {
        if local_mk_names.contains(name) {
            continue;
        }
        let bytes = storage
            .get(&format!("library/{name}"))
            .await
            .map_err(SyncError::RemoteUnreachable)?;
        std::fs::create_dir_all(&local_dir).map_err(SyncError::Io)?;
        std::fs::write(local_dir.join(name), &bytes).map_err(SyncError::Io)?;
    }

    Ok(())
}

/// For each MediaCreation op in `group`, checks if the file exists on the remote and
/// inserts it into `media_list` if so.
async fn update_media_list_from_group(
    storage: &(dyn crate::storage::Storage + Send + Sync),
    group: &OperationGroup,
    media_list: &mut MediaList,
    changed: &mut bool,
) -> Result<(), LibraryError> {
    for op in &group.operations {
        if let Operation::MediaCreation {
            media_id,
            storage_date,
            ..
        } = op
        {
            let key = format!("media/{}/{:02}/{}.data", storage_date.year, storage_date.month, media_id.0);
            if storage
                .exists(&key)
                .await
                .map_err(SyncError::RemoteUnreachable)?
            {
                if media_list.insert_present(*media_id) {
                    *changed = true;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
