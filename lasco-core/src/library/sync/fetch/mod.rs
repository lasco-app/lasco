use std::collections::HashSet;

use crate::crdt::{CrdtOperation, CrdtStateReplica, OperationContent};
use crate::encryption::master_key::{MasterKey, parse_mk_filename};
use crate::error::{LibraryError, SyncError};
use crate::identifiers::{LibraryId, RemoteUuid};
use crate::library::Library;
use crate::library::local_dirs::{
    LocalStateCrdt, LocalStateLibraryDir, RemoteLastKnownStateDir, RemoteMediaList,
    RemoteMergedRemoteFiles,
};
use crate::library::local_ops_read_write::LocalOpsReadWriteLock;
use crate::library::remote_media_list_lock::RemoteMediaListLock;
use crate::operations::remote_ops::RemoteOpFile;
use crate::remote::{LastKnownState, MediaList, MergedRemoteFiles};

use super::remote_access::StorageRead;
use super::{SyncReportFetch, verify_remote_identity};

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
        let remote = StorageRead::new(storage);
        let local_state_library_dir = self.inner.local_dirs.local_state_library_dir();
        let remote_last_known_state_dir =
            self.inner.local_dirs.remote_last_known_state_dir(remote_id);
        let remote_media_list = self.inner.local_dirs.remote_media_list(remote_id);
        let remote_merged_remote_files =
            self.inner.local_dirs.remote_merged_remote_files(remote_id);
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
        Ok(report)
    }
}

pub(super) async fn fetch_impl(
    storage: &StorageRead<'_>,
    remote_id: &str,
    library_id: LibraryId,
    local_state_library_dir: &LocalStateLibraryDir,
    remote_last_known_state_dir: &RemoteLastKnownStateDir,
    remote_media_list: &RemoteMediaList,
    remote_merged_remote_files: &RemoteMergedRemoteFiles,
    local_ops_read_write_lock: &LocalOpsReadWriteLock,
    remote_media_list_lock: &RemoteMediaListLock,
    master_key: &MasterKey,
    replica_state: &parking_lot::RwLock<CrdtStateReplica>,
    local_state_crdt: &LocalStateCrdt,
) -> Result<SyncReportFetch, LibraryError> {
    let remote_uuid = remote_id
        .parse::<uuid::Uuid>()
        .map(RemoteUuid::from_uuid)
        .map_err(|e| {
            SyncError::RemoteIdMismatch(format!("invalid remote id '{remote_id}': {e}"))
        })?;
    verify_remote_identity(storage, remote_uuid).await?;

    // Step 1: pull any mk_*.enc files present on the remote but missing locally,
    // so users added from another device become available on this one.
    fetch_library_dir(storage, local_state_library_dir, library_id).await?;

    // Load merge progress for immutable remote operation files. It controls only whether a file
    // must be merged again; cache presence is determined independently by LastKnownState.
    let merged_files_path = remote_merged_remote_files.merged_remote_files_path();
    let mut merged_files = MergedRemoteFiles::load_or_default(&merged_files_path)?;

    let last_known_state =
        LastKnownState::download(storage, remote_last_known_state_dir, master_key).await?;

    let mut ops_downloaded = 0usize;
    let mut merged_files_changed = false;
    let mut local_state_rebuild_required = false;
    let inventory_operations = {
        let mut replica = replica_state.write();
        let mut inventory_operations = Vec::new();
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
                        if !replica.state.causal_context.contains(&operation.dot) {
                            local_ops_read_write_lock
                                .lock(master_key)
                                .append_operation(operation)?;
                            replica.state.apply(operation);
                            inventory_operations.push(operation.clone());
                            ops_downloaded += 1;
                        }
                    }
                    merged_files.insert(file_uuid);
                    merged_files_changed = true;
                    local_state_rebuild_required = true;
                }
            }
        }

        if merged_files_changed {
            merged_files.save(&merged_files_path)?;
        }
        if local_state_rebuild_required {
            crate::crdt::save_persisted(&local_state_crdt.snapshot_path(), master_key, &replica)?;
        }
        inventory_operations
    };
    for operation in &inventory_operations {
        update_media_list_from_group(
            storage,
            operation,
            remote_id,
            remote_media_list,
            remote_media_list_lock,
        )
        .await;
    }

    Ok(SyncReportFetch {
        ops_downloaded,
        local_state_rebuild_required,
    })
}

/// Step 1 of fetch. Verifies the remote's `library/library_id_{uuid}` matches this
/// library, then downloads any `mk_*.enc` file present on the remote but missing
/// locally, so users added from another device become available on this one.
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

/// For each MediaCreation operation, checks if the file exists on the remote and
/// records it in `media_list` if so. Inventory errors are intentionally ignored.
async fn update_media_list_from_group(
    storage: &StorageRead<'_>,
    operation: &CrdtOperation,
    remote_id: &str,
    remote_media_list: &RemoteMediaList,
    remote_media_list_lock: &RemoteMediaListLock,
) {
    if let OperationContent::MediaCreation(creation) = &operation.content {
        let media_id = creation.media_id;
        let storage_date = creation.storage_date;
        let key = format!(
            "media/{}/{:02}/{}.data",
            storage_date.year, storage_date.month, media_id.0
        );
        if matches!(storage.exists(&key).await, Ok(true)) {
            remote_media_list_lock.with_lock(remote_id, remote_media_list, |remote_media_list| {
                let path = remote_media_list.media_list_path();
                // This is a positive-only, opportunistic inventory. Its absence or corruption
                // must not prevent fetch from establishing the last-known operation state and local log.
                let Ok(mut media_list) = MediaList::load_or_default(&path) else {
                    return;
                };
                if media_list.insert_present(media_id) {
                    let _ = media_list.save(&path);
                }
            });
        }
    }
}
