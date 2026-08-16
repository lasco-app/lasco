use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob_key::derive_blob_key;
use crate::error::{LibraryError, SyncError};
use crate::identifiers::{CompactedOpId, MediaUuid, RemoteUuid};
use crate::library::Library;
use crate::library::local_dirs::{
    LocalStateLibraryDir, LocalStateMediaDir, RemoteLastKnownStateDir, RemoteMediaList,
};
use crate::operations::remote_ops::{self as op_io, RemoteOpFile};
use crate::operations::{CompactionFile, StorageDate};
use crate::remote::last_known_state::collect_dots_from_dir;
use crate::remote::{LastKnownState, MediaList};
use crate::storage::AtomicWriteMode;

use super::compaction::{
    self, appropriate_tier, count_tier_files, release_lock, tier_needs_compaction, try_acquire_lock,
};
use super::remote_access::StorageReadWrite;
use super::{PushMediaSource, SyncReportPush, map_op_err, verify_remote_identity};

struct FileToPush {
    media_id: MediaUuid,
    storage_date: StorageDate,
}

pub(super) struct PushAccess<'a> {
    pub storage: &'a StorageReadWrite<'a>,
    pub local_state_media_dir: &'a LocalStateMediaDir,
    pub remote_last_known_state_dir: &'a RemoteLastKnownStateDir,
    pub remote_media_list: &'a RemoteMediaList,
    pub local_state_library_dir: &'a LocalStateLibraryDir,
}

impl Library {
    /// # Errors
    ///
    /// Returns an error if remote identity verification, operation upload, or required media upload fails.
    pub async fn push(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
    ) -> Result<SyncReportPush, LibraryError> {
        self.push_with_media_source(storage, remote_id, PushMediaSource::LocalOnly)
            .await
    }

    /// Push with an explicit policy for media absent from the local cache.
    ///
    /// # Errors
    ///
    /// Returns an error if another push is active, remote validation/upload fails, or the selected media source cannot supply required blobs.
    pub async fn push_with_media_source(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
        media_source: PushMediaSource<'_>,
    ) -> Result<SyncReportPush, LibraryError> {
        let remote_id_string = remote_id.to_string();
        let _guard = self
            .try_acquire_remote_sync(&remote_id_string)
            .ok_or(SyncError::AlreadyRunning)?;
        let remote = StorageReadWrite::new(storage);
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let local_state_library_dir = self.inner.local_dirs.local_state_library_dir();
        let remote_last_known_state_dir = self
            .inner
            .local_dirs
            .remote_last_known_state_dir(&remote_id_string);
        let remote_media_list = self.inner.local_dirs.remote_media_list(&remote_id_string);
        self.push_impl(
            PushAccess {
                storage: &remote,
                local_state_media_dir: &local_state_media_dir,
                remote_last_known_state_dir: &remote_last_known_state_dir,
                remote_media_list: &remote_media_list,
                local_state_library_dir: &local_state_library_dir,
            },
            remote_id,
            media_source,
        )
        .await
    }

    #[allow(
        clippy::too_many_lines,
        reason = "The ordered push workflow shares one report and transaction context across its phases."
    )]
    pub(super) async fn push_impl(
        &self,
        access: PushAccess<'_>,
        remote_id: RemoteUuid,
        media_source: PushMediaSource<'_>,
    ) -> Result<SyncReportPush, LibraryError> {
        let master_key = &self.inner.master_key;

        verify_remote_identity(&access.storage.as_read(), remote_id).await?;
        let remote_id_string = remote_id.to_string();

        let relay_source = match media_source {
            PushMediaSource::LocalOnly => None,
            PushMediaSource::FromRemote { remote_id, storage } => {
                verify_remote_identity(&storage, remote_id).await?;
                Some((remote_id.to_string(), storage))
            }
        };

        // Master-key files are immutable credentials. Propagate every local one with a
        // non-overwriting write after identity verification; never delete or replace a remote key.
        for entry in std::fs::read_dir(access.local_state_library_dir.path())? {
            let entry = entry?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let is_master_key_file = name
                .strip_prefix("mk_")
                .and_then(|name| name.strip_suffix(".enc"))
                .is_some();
            if !path.is_file() || !is_master_key_file {
                continue;
            }
            let data = std::fs::read(&path)?;
            access
                .storage
                .put_atomic(
                    &format!("library/{name}"),
                    &data,
                    AtomicWriteMode::CreateIfAbsent,
                )
                .await
                .map_err(SyncError::RemoteUnreachable)?;
        }

        // Relay blobs never belong in normal library state. A unique OS temporary directory
        // keeps them outside the local media cache and removes them when this Push returns.
        let staging_dir = tempfile::tempdir()?;

        // Push only ever acts on op files recorded in its own last known state for this
        // remote, plus whatever it uploads or merges in this call. It never lists or reads
        // arbitrary remote files to decide what to upload or compact, so it can't turn into
        // an implicit fetch.
        let ops_dir = access.remote_last_known_state_dir.operations_dir();
        let mut last_known_state = LastKnownState::open(access.remote_last_known_state_dir)?;
        let remote_covered =
            collect_dots_from_dir(&ops_dir, master_key).map_err(SyncError::LocalCacheCorrupt)?;

        // Only the snapshot read is locked. Network storage awaits below must remain unlocked.
        let media_list = self.inner.remote_media_list_lock.with_lock(
            &remote_id_string,
            access.remote_media_list,
            |remote_media_list| MediaList::load_or_default(&remote_media_list.media_list_path()),
        )?;

        // Report missing media before uploading operations. The caller can then select a
        // source and retry without a partially completed default Push.
        if relay_source.is_none() {
            let missing: Vec<_> = {
                let state = self.inner.state.read();
                state
                    .media_entries()
                    .iter()
                    .filter(|entry| !media_list.contains(&entry.media_id))
                    .filter_map(|entry| {
                        (!access
                            .local_state_media_dir
                            .data_path(
                                entry.storage_date.year,
                                entry.storage_date.month,
                                &entry.media_id,
                            )
                            .exists())
                        .then_some(entry.media_id)
                    })
                    .collect()
            };
            if !missing.is_empty() {
                return Err(SyncError::MissingLocalMedia(missing).into());
            }
        }

        // The append-only log is the complete local operation history, including
        // operations learned by Fetch that must be relayed to this remote.
        // Read it synchronously and release its guard before any network await.
        let ops_to_upload: Vec<_> = self
            .local_ops_read_write()
            .read_operations()?
            .into_iter()
            .filter(|operation| !remote_covered.contains(&operation.dot))
            .collect();

        let ops_uploaded = ops_to_upload.len();
        let mut compactions_run = 0usize;

        if !ops_to_upload.is_empty() {
            let op_count = u32::try_from(ops_to_upload.len())
                .expect("an in-memory upload batch cannot contain more than u32::MAX operations");
            let tier = appropriate_tier(ops_to_upload.len());
            {
                // Upload as a single compaction file at the tier that fits this batch.
                let file_uuid = CompactedOpId::new();
                let key = format!("operations/{file_uuid}.op{tier}_{op_count}");
                let comp_file = CompactionFile {
                    tier,
                    operations: ops_to_upload.clone(),
                };
                let blob =
                    crate::operations::encrypt_compaction_file(master_key, &file_uuid, &comp_file)
                        .map_err(map_op_err)?;
                let bytes = blob.to_bytes();
                op_io::write_compaction_bytes(access.storage, &key, &bytes)
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
                let device_id = self.inner.state.read().device_id();
                let lock_token = try_acquire_lock(access.storage, device_id).await?;
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
                            access.storage,
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
                    release_lock(access.storage, lock_token).await?;
                    if let Some(error) = cascade_error {
                        return Err(error.into());
                    }
                    assert!(cascade_done, "compaction cascade exceeded 10 tiers");
                }
                // else: the lock is held by another client, skip compaction for this push.
            }
        }

        let media_pending: Vec<FileToPush> = {
            let state = self.inner.state.read();
            state
                .media_entries()
                .iter()
                .filter(|entry| !media_list.contains(&entry.media_id))
                .map(|entry| FileToPush {
                    media_id: entry.media_id,
                    storage_date: entry.storage_date,
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

            let data_path = access.local_state_media_dir.data_path(
                item.storage_date.year,
                item.storage_date.month,
                &item.media_id,
            );
            let (bytes, staged_data) = if data_path.exists() {
                (std::fs::read(&data_path)?, None)
            } else {
                let (source_id, source) = relay_source
                    .as_ref()
                    .expect("missing local media requires a relay source");
                let downloaded = source
                    .get(&data_key)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
                let bytes = stage_and_validate_media(
                    staging_dir.path(),
                    &downloaded,
                    item.media_id,
                    &self.inner.master_key,
                )?;
                self.record_remote_media_presence(source_id, item.media_id);
                (bytes.0, Some(bytes.1))
            };
            let data_uploaded = access
                .storage
                .put_atomic(&data_key, &bytes, AtomicWriteMode::CreateIfAbsent)
                .await
                .map_err(SyncError::RemoteUnreachable)?;
            if let Some(path) = staged_data {
                std::fs::remove_file(path)?;
            }

            let thumb_path = access.local_state_media_dir.thumb_path(
                item.storage_date.year,
                item.storage_date.month,
                &item.media_id,
            );
            if thumb_path.exists() {
                let bytes = std::fs::read(&thumb_path)?;
                access
                    .storage
                    .put_atomic(&thumb_key, &bytes, AtomicWriteMode::CreateIfAbsent)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
            } else if let Some((_, source)) = relay_source.as_ref() {
                match source.get(&thumb_key).await {
                    Ok(downloaded) => match stage_and_validate_media(
                        staging_dir.path(),
                        &downloaded,
                        item.media_id,
                        &self.inner.master_key,
                    ) {
                        Ok((bytes, path)) => {
                            access
                                .storage
                                .put_atomic(&thumb_key, &bytes, AtomicWriteMode::CreateIfAbsent)
                                .await
                                .map_err(SyncError::RemoteUnreachable)?;
                            std::fs::remove_file(path)?;
                        }
                        Err(error) => return Err(error),
                    },
                    Err(crate::storage::StorageError::NotFound) => {}
                    Err(error) => return Err(SyncError::RemoteUnreachable(error).into()),
                }
            }

            // Reload under the lock so this write preserves any concurrent fetch or on-demand
            // download observations made while the upload was in progress.
            self.inner.remote_media_list_lock.with_lock(
                &remote_id_string,
                access.remote_media_list,
                |remote_media_list| {
                    let path = remote_media_list.media_list_path();
                    let mut media_list = MediaList::load_or_default(&path)?;
                    if media_list.insert_present(item.media_id) {
                        media_list.save(&path)?;
                    }
                    Ok::<_, std::io::Error>(())
                },
            )?;

            if data_uploaded {
                media_uploaded += 1;
            }
        }

        Ok(SyncReportPush {
            ops_uploaded,
            media_uploaded,
            compactions_run,
        })
    }
}

/// Download one encrypted blob into an isolated staging file, prove it decrypts, then return
/// its bytes and path. Callers remove the path immediately after the target upload succeeds.
fn stage_and_validate_media(
    staging_dir: &std::path::Path,
    bytes: &[u8],
    media_id: MediaUuid,
    master_key: &crate::encryption::master_key::MasterKey,
) -> Result<(Vec<u8>, std::path::PathBuf), LibraryError> {
    let path = staging_dir.join(format!("{}.stage", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes)?;
    let staged = std::fs::read(&path)?;
    let blob = BlobEncrypted::from_bytes(&staged).map_err(crate::error::OperationError::Blob)?;
    let file_key = derive_blob_key(master_key, &media_id.0);
    crate::encryption::blob::decrypt_blob(&file_key, &blob)
        .map_err(crate::error::OperationError::Crypto)?;
    Ok((staged, path))
}
