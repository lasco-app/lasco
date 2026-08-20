use std::collections::HashSet;

use crate::crdt::CrdtState;
use crate::encryption::master_key::{MasterKey, parse_mk_filename};
use crate::error::{LibraryError, SyncError};
use crate::identifiers::{LibraryId, RemoteUuid};
use crate::library::Library;
use crate::library::local_dirs::{
    LocalStateCrdt, LocalStateLibraryDir, RemoteCompactOpIdMergedToLocal, RemoteLastKnownStateDir,
    RemoteMediaList,
};
use crate::library::local_ops_read_write::LocalOpsReadWriteLock;
use crate::library::remote_media_list_lock::RemoteMediaListLock;
use crate::operations::remote_ops::RemoteOpFile;
use crate::remote::{CompactOpIdMergedToLocal, LastKnownState};

use super::media_inventory::{KnownMedia, confirm_known_media};
use super::remote_access::StorageRead;
use super::{SyncReportFetch, verify_remote_identity};

pub(super) struct FetchAccess<'a> {
    pub storage: &'a StorageRead<'a>,
    pub local_state_library_dir: &'a LocalStateLibraryDir,
    pub remote_last_known_state_dir: &'a RemoteLastKnownStateDir,
    pub remote_media_list: &'a RemoteMediaList,
    pub remote_compact_op_id_merged_to_local: &'a RemoteCompactOpIdMergedToLocal,
    pub local_ops_read_write_lock: &'a LocalOpsReadWriteLock,
    pub remote_media_list_lock: &'a RemoteMediaListLock,
}

impl Library {
    /// # Errors
    ///
    /// Returns an error if remote identity or operations cannot be read, decoded, stored, or used to rebuild local state.
    pub async fn fetch(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
    ) -> Result<SyncReportFetch, LibraryError> {
        let remote_id_string = remote_id.to_string();
        let _remote_guard = self
            .try_acquire_remote_sync(&remote_id_string)
            .ok_or(SyncError::AlreadyRunning)?;
        let _fetch_guard = self
            .try_acquire_fetch_slot()
            .ok_or(SyncError::AlreadyRunning)?;
        let remote = StorageRead::new(storage);
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
        let report = fetch_impl(
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
        .await?;
        Ok(report)
    }
}

pub(super) async fn fetch_impl(
    access: FetchAccess<'_>,
    remote_id: RemoteUuid,
    library_id: LibraryId,
    master_key: &MasterKey,
    state_lock: &parking_lot::RwLock<CrdtState>,
    local_state_crdt: &LocalStateCrdt,
) -> Result<SyncReportFetch, LibraryError> {
    verify_remote_identity(access.storage, remote_id).await?;
    let remote_id_string = remote_id.to_string();

    // Step 1: pull any mk_*.enc files present on the remote but missing locally,
    // so users added from another device become available on this one.
    fetch_library_dir(access.storage, access.local_state_library_dir, library_id).await?;

    // Load merge progress for immutable remote operation files. It controls only whether a file
    // must be merged again; cache presence is determined independently by LastKnownState.
    let merged_files_path = access
        .remote_compact_op_id_merged_to_local
        .compact_op_id_merged_to_local_path();
    let mut merged_files = CompactOpIdMergedToLocal::load_or_default(&merged_files_path)?;

    let last_known_state = LastKnownState::download(
        access.storage,
        access.remote_last_known_state_dir,
        master_key,
    )
    .await?;

    let mut ops_downloaded = 0usize;
    let mut merged_files_changed = false;
    // This is a transient log-write deduplication set, not materialized CRDT state.
    // CrdtState itself remains idempotent when an operation is applied again.
    let mut local_log_dots = access
        .local_ops_read_write_lock
        .lock(master_key)
        .known_dots()?;
    {
        let mut state = state_lock.write();
        let mut merged_operations = Vec::new();
        for file in last_known_state.files() {
            let file_uuid = LastKnownState::file_uuid(file);
            if merged_files.contains(&file_uuid) {
                continue;
            }
            match file {
                RemoteOpFile::Compaction {
                    uuid,
                    tier,
                    op_count,
                } => {
                    let compaction = last_known_state
                        .read_compaction_file(master_key, uuid, *tier, *op_count)?;
                    for operation in &compaction.operations {
                        if local_log_dots.insert(operation.dot) {
                            access
                                .local_ops_read_write_lock
                                .lock(master_key)
                                .append_operation(operation)?;
                            ops_downloaded += 1;
                        }
                        merged_operations.push(operation.clone());
                    }
                    merged_files.insert(file_uuid);
                    merged_files_changed = true;
                }
            }
        }

        if merged_files_changed {
            merged_files.save(&merged_files_path)?;
        }
        if merged_files_changed {
            state.apply_batch(merged_operations.iter());
            crate::crdt::save_persisted(&local_state_crdt.snapshot_path(), master_key, &state)?;
        }
    }
    // Confirm the remote presence of every media the reconstructed state knows about and that
    // the inventory has not confirmed yet, not only the ones created by the operations merged
    // in this run. A blob uploaded by another client after its creation operation was merged
    // here would otherwise never be discovered.
    let known_media: Vec<KnownMedia> = {
        let state = state_lock.read();
        state
            .media_entries()
            .iter()
            .map(|entry| KnownMedia {
                media_id: entry.media_id,
                storage_date: entry.storage_date,
                expects_thumb: entry.companion_kind.is_none(),
            })
            .collect()
    };
    confirm_known_media(
        access.storage,
        &known_media,
        &remote_id_string,
        access.remote_media_list,
        access.remote_media_list_lock,
    )
    .await;

    Ok(SyncReportFetch { ops_downloaded })
}

/// Step 1 of fetch. Verifies the remote's `library/library_id_{uuid}` matches this library
/// and that its format sentinel is one this build understands, then downloads any `mk_*.enc`
/// file present on the remote but missing locally, so users added from another device become
/// available on this one.
async fn fetch_library_dir(
    storage: &StorageRead<'_>,
    local_state_library_dir: &LocalStateLibraryDir,
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

    crate::library::sync::verify_remote_library_format_with_keys(&remote_files)?;

    let remote_mk_names: Vec<&str> = remote_files
        .iter()
        .filter_map(|k| {
            let name = k.rsplit('/').next().unwrap_or(k);
            parse_mk_filename(name).map(|_| name)
        })
        .collect();

    let local_dir = local_state_library_dir.path();
    let local_mk_names: HashSet<String> = match std::fs::read_dir(local_dir) {
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
        std::fs::create_dir_all(local_dir).map_err(SyncError::Io)?;
        crate::atomic_file::write(&local_dir.join(name), &bytes).map_err(SyncError::Io)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
