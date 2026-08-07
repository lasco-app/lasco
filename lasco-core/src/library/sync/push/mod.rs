use crate::error::{LibraryError, SyncError};
use crate::identifiers::{CompactedOpId, MediaUuid, RemoteUuid};
use crate::library::Library;
use crate::library::local_dirs::{LocalStateMediaDir, RemoteLastKnownStateDir};
use crate::operations::remote_ops::{self as op_io, RemoteOpFile};
use crate::operations::{CompactionEntry, CompactionFile, StorageDate};
use crate::remote::last_known_state::collect_group_ids_from_dir;
use crate::remote::{LastKnownState, MediaList};

use super::compaction::{
    self, appropriate_tier, count_tier_files, release_lock, tier_needs_compaction, try_acquire_lock,
};
use super::remote_access::StorageReadWrite;
use super::{SyncReportPush, map_op_err, verify_remote_identity};

struct FileToPush {
    media_id: MediaUuid,
    storage_date: StorageDate,
}

impl Library {
    pub async fn push(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: &str,
    ) -> Result<SyncReportPush, LibraryError> {
        let _guard = self
            .try_acquire_remote_sync(remote_id)
            .ok_or(SyncError::AlreadyRunning)?;
        let remote = StorageReadWrite::new(storage);
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let remote_last_known_state_dir =
            self.inner.local_dirs.remote_last_known_state_dir(remote_id);
        self.push_impl(
            &remote,
            remote_id,
            &local_state_media_dir,
            &remote_last_known_state_dir,
        )
        .await
    }

    pub(super) async fn push_impl(
        &self,
        storage: &StorageReadWrite<'_>,
        remote_id: &str,
        local_state_media_dir: &LocalStateMediaDir,
        remote_last_known_state_dir: &RemoteLastKnownStateDir,
    ) -> Result<SyncReportPush, LibraryError> {
        let master_key = &self.inner.master_key;

        let remote_uuid = remote_id
            .parse::<uuid::Uuid>()
            .map(RemoteUuid::from_uuid)
            .map_err(|e| {
                SyncError::RemoteIdMismatch(format!("invalid remote id '{remote_id}': {e}"))
            })?;
        verify_remote_identity(&storage.as_read(), remote_uuid).await?;

        // Push only ever acts on op files recorded in its own last known state for this
        // remote, plus whatever it uploads or merges in this call. It never lists or reads
        // arbitrary remote files to decide what to upload or compact, so it can't turn into
        // an implicit fetch.
        let ops_dir = remote_last_known_state_dir.operations_dir();
        let mut last_known_state = LastKnownState::open(remote_last_known_state_dir)?;
        let remote_covered = collect_group_ids_from_dir(&ops_dir, master_key)
            .map_err(SyncError::LocalCacheCorrupt)?;

        let media_list_path = remote_last_known_state_dir.media_list_path();
        let mut media_list = MediaList::load_or_default(&media_list_path)?;

        // Flush any pending (unpushed) op group into the main log before uploading.
        let flushed_pending = {
            let mut local_ops = self.local_ops_read_write();
            if let Some(pending) = local_ops.read_pending()? {
                local_ops.append_log(&pending)?;
                // Deleting only after the append makes an interruption safe: it can
                // leave a duplicate group, but the log reader deduplicates by op_id.
                local_ops.remove_pending()?;
                true
            } else {
                false
            }
        };
        if flushed_pending {
            self.load_local_state().await?;
        }

        // Read all local op groups from the log.
        let local_groups = self.local_ops_read_write().read_log_groups()?;
        let ops_to_upload: Vec<_> = local_groups
            .into_iter()
            .filter(|g| !remote_covered.contains(&g.op_id))
            .collect();

        let ops_uploaded = ops_to_upload.len();
        let mut compactions_run = 0usize;

        if !ops_to_upload.is_empty() {
            let op_count: u32 = ops_to_upload
                .iter()
                .map(|g| g.operations.len() as u32)
                .sum();
            let tier = appropriate_tier(op_count as usize);
            {
                // Upload as a single compaction file at the tier that fits this batch.
                let file_uuid = CompactedOpId::new();
                let contents: Vec<CompactionEntry> = ops_to_upload
                    .iter()
                    .map(|g| CompactionEntry {
                        op_id: g.op_id,
                        group: g.clone(),
                    })
                    .collect();
                let key = format!("operations/{file_uuid}.op{tier}_{op_count}");
                let comp_file = CompactionFile { tier, contents };
                let blob =
                    crate::operations::encrypt_compaction_file(master_key, &file_uuid, &comp_file)
                        .map_err(map_op_err)?;
                let bytes = blob.to_bytes();
                op_io::write_compaction_bytes(storage, &key, &bytes)
                    .await
                    .map_err(map_op_err)?;
                last_known_state.write_compaction_bytes(&file_uuid, tier, op_count, &bytes)?;
                last_known_state.files_mut().push(RemoteOpFile::Compaction {
                    uuid: file_uuid,
                    tier,
                    op_count,
                });
            }

            // Walk tiers from the upload tier upward for cascade compaction, based only on
            // this client's own last known state for the remote.
            // A library with 10^9 ops needs at most 8 tiers. Completing all 10 iterations is a bug.
            let file_counts = count_tier_files(last_known_state.files());
            let mut cascade_done =
                !tier_needs_compaction(file_counts.get(&tier).copied().unwrap_or(0));

            if !cascade_done {
                // The lock spans the whole cascade below, not just a single tier's merge, so
                // another client can never interleave a compaction between two of our tiers.
                let lock_token = try_acquire_lock(storage).await?;
                if let Some(lock_token) = lock_token {
                    let mut cascade_error = None;
                    for current_tier in tier..tier + 10 {
                        let file_counts = count_tier_files(last_known_state.files());
                        let file_count = file_counts.get(&current_tier).copied().unwrap_or(0);
                        if !tier_needs_compaction(file_count) {
                            cascade_done = true;
                            break;
                        }
                        match compaction::compact_tier(
                            storage,
                            master_key,
                            current_tier,
                            &last_known_state,
                            &lock_token,
                        )
                        .await
                        {
                            Ok(result) => {
                                // compact_tier already updated the on-disk last known state
                                // for the new file and every deleted source as it went, so
                                // this only needs to bring the in-memory view in line.
                                compactions_run += 1;
                                last_known_state
                                    .files_mut()
                                    .retain(|f| !result.sources.contains(f));
                                last_known_state.files_mut().push(result.new_file);
                            }
                            Err(error) => {
                                cascade_error = Some(error);
                                break;
                            }
                        }
                    }
                    release_lock(storage, lock_token).await?;
                    if let Some(error) = cascade_error {
                        return Err(error.into());
                    }
                    assert!(cascade_done, "compaction cascade exceeded 10 tiers");
                }
                // else: the lock is held by another client, skip compaction for this push.
            }
        }

        let media_pending: Vec<FileToPush> = {
            let state = self.inner.operation_state.read();
            state
                .reconstructed
                .media
                .iter()
                .filter(|(media_id, _)| !media_list.contains(media_id))
                .filter(|(media_id, file_meta)| {
                    local_state_media_dir
                        .data_path(
                            file_meta.storage_date.year,
                            file_meta.storage_date.month,
                            media_id,
                        )
                        .exists()
                })
                .map(|(media_id, file_meta)| FileToPush {
                    media_id: *media_id,
                    storage_date: file_meta.storage_date,
                })
                .collect()
        };

        let mut media_uploaded = 0usize;

        for item in &media_pending {
            let data_key = format!(
                "media/{}/{:02}/{}.data",
                item.storage_date.year, item.storage_date.month, item.media_id
            );
            let thumb_key = format!(
                "media/{}/{:02}/{}.thumb",
                item.storage_date.year, item.storage_date.month, item.media_id
            );

            let data_path = local_state_media_dir.data_path(
                item.storage_date.year,
                item.storage_date.month,
                &item.media_id,
            );
            let bytes = std::fs::read(&data_path)?;
            storage
                .put_atomic(&data_key, &bytes)
                .await
                .map_err(SyncError::RemoteUnreachable)?;

            let thumb_path = local_state_media_dir.thumb_path(
                item.storage_date.year,
                item.storage_date.month,
                &item.media_id,
            );
            if thumb_path.exists() {
                let bytes = std::fs::read(&thumb_path)?;
                storage
                    .put_atomic(&thumb_key, &bytes)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
            }

            media_list.insert_present(item.media_id);
            media_list.save(&media_list_path)?;

            media_uploaded += 1;
        }

        Ok(SyncReportPush {
            ops_uploaded,
            media_uploaded,
            compactions_run,
        })
    }
}

#[cfg(test)]
mod tests;
