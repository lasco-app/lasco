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
use futures_util::future::BoxFuture;
use futures_util::{
    FutureExt,
    stream::{FuturesUnordered, StreamExt},
};

use super::compaction::{
    self, appropriate_tier, count_tier_files, release_lock, tier_needs_compaction, try_acquire_lock,
};
use super::media_inventory::{KnownMedia, confirm_known_media};
use super::remote_access::{StorageRead, StorageReadWrite};
use super::{
    MediaBlob, PlannedMediaSource, PushMediaPlan, PushMediaResolution, PushMediaSource,
    PushProgressObserver, SyncReportPush, map_op_err, verify_remote_identity,
    verify_remote_library_format,
};
mod media_transfer;
use media_transfer::{stage_and_validate_media, thumb_source};

/// The local cache first, since using it costs no download, then the first candidate whose own
/// inventory confirms the blob.
fn resolve_blob(
    cached_locally: bool,
    candidates: &[(RemoteUuid, MediaList)],
    confirmed: impl Fn(&MediaList) -> bool,
) -> Option<PlannedMediaSource> {
    if cached_locally {
        return Some(PlannedMediaSource::Local);
    }
    candidates
        .iter()
        .find(|(_, inventory)| confirmed(inventory))
        .map(|(remote_id, _)| PlannedMediaSource::Remote(*remote_id))
}

struct FileToPush {
    media_id: MediaUuid,
    storage_date: StorageDate,
    needs_data: bool,
    needs_thumb: bool,
}

/// The maximum number of independent media transfers a Push drives at once.
///
/// This is deliberately small: two concurrent S3 PUTs hide request latency without
/// unnecessarily competing with a mobile device's upload bandwidth or memory.
const MAX_CONCURRENT_MEDIA_UPLOADS: usize = 2;

/// The remote-presence facts produced by one completed media transfer.
///
/// The worker performs the per-item byte preparation and network I/O. The parent Push records
/// these facts in the local media inventory after the worker completes, so inventory mutation
/// remains serialized and never spans an await.
struct MediaUploadOutcome {
    media_id: MediaUuid,
    data_uploaded: bool,
    data_present: bool,
    thumb_present: bool,
}

pub(super) struct CloudPushContext {
    runtime: crate::library::cloud::SharedCloudRuntime,
    remote_id: RemoteUuid,
}

pub(super) struct PushAccess<'a> {
    pub storage: &'a StorageReadWrite<'a>,
    pub local_state_media_dir: &'a LocalStateMediaDir,
    pub remote_last_known_state_dir: &'a RemoteLastKnownStateDir,
    pub remote_media_list: &'a RemoteMediaList,
    pub local_state_library_dir: &'a LocalStateLibraryDir,
}

impl Library {
    /// Resolves, for every blob the target is missing, where to get it.
    ///
    /// This is push preparation. It reads the local media cache and the media inventories of
    /// the target and of the candidate sources, in `source_priority` order, and contacts no
    /// remote at all, so it works offline and always terminates immediately.
    ///
    /// A blob the target's own inventory already confirms is left out of the plan, since push
    /// has nothing to do for it.
    ///
    /// # Errors
    ///
    /// Returns an error if a media inventory exists but cannot be read.
    pub fn resolve_push_media(
        &self,
        target: RemoteUuid,
        source_priority: &[RemoteUuid],
    ) -> Result<PushMediaResolution, LibraryError> {
        let load_inventory = |remote_id: RemoteUuid| -> Result<MediaList, LibraryError> {
            let remote_id = remote_id.to_string();
            let remote_media_list = self.inner.local_dirs.remote_media_list(&remote_id);
            Ok(self.inner.remote_media_list_lock.with_lock(
                &remote_id,
                &remote_media_list,
                |remote_media_list| {
                    MediaList::load_or_default(&remote_media_list.media_list_path())
                },
            )?)
        };

        let target_inventory = load_inventory(target)?;
        let mut candidates = Vec::new();
        for candidate in source_priority {
            if *candidate == target {
                continue;
            }
            candidates.push((*candidate, load_inventory(*candidate)?));
        }

        let media_dir = self.inner.local_dirs.local_state_media_dir();
        let mut resolution = PushMediaResolution::default();
        for entry in self.inner.state.read().media_entries() {
            let year = entry.storage_date.year;
            let month = entry.storage_date.month;

            if !target_inventory.has_full(&entry.media_id) {
                let cached = media_dir.data_path(year, month, &entry.media_id).exists();
                let source = resolve_blob(cached, &candidates, |inventory| {
                    inventory.has_full(&entry.media_id)
                });
                match source {
                    Some(source) => {
                        resolution
                            .assignments
                            .insert((entry.media_id, MediaBlob::Data), source);
                    }
                    // Uploading the library without the original would lose the media.
                    None => resolution.unresolved_data.push(entry.media_id),
                }
            }

            // A companion resource has no thumbnail to place, so it is left out of the plan
            // entirely once its data blob is resolved.
            if entry.companion_kind.is_none() && !target_inventory.has_thumb(&entry.media_id) {
                let cached = media_dir.thumb_path(year, month, &entry.media_id).exists();
                let source = resolve_blob(cached, &candidates, |inventory| {
                    inventory.has_thumb(&entry.media_id)
                });
                // A thumbnail nobody can provide is left out rather than reported. Nothing
                // records whether a media ever had one, so failing here would let a media
                // without a thumbnail block every push of the library.
                if let Some(source) = source {
                    resolution
                        .assignments
                        .insert((entry.media_id, MediaBlob::Thumb), source);
                }
            }
        }

        Ok(resolution)
    }

    pub async fn push_with_media_plan(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
        plan: PushMediaPlan<'_>,
    ) -> Result<SyncReportPush, LibraryError> {
        self.push_with_media_plan_and_progress(storage, remote_id, plan, None)
            .await
    }

    /// Push with a media plan and an optional observer for completed full-media uploads.
    pub async fn push_with_media_plan_and_progress(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
        plan: PushMediaPlan<'_>,
        progress: Option<&dyn PushProgressObserver>,
    ) -> Result<SyncReportPush, LibraryError> {
        self.push_with_media_source_and_progress(
            storage,
            remote_id,
            PushMediaSource::Plan(plan),
            progress,
        )
        .await
    }
    /// # Errors
    ///
    /// Returns an error if remote identity verification, operation upload, or required media upload fails.
    pub async fn push(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
    ) -> Result<SyncReportPush, LibraryError> {
        self.push_with_media_source_and_progress(
            storage,
            remote_id,
            PushMediaSource::LocalOnly,
            None,
        )
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
        self.push_with_media_source_and_progress(storage, remote_id, media_source, None)
            .await
    }

    /// Push with an explicit media source and an optional observer for completed full-media
    /// uploads.
    pub async fn push_with_media_source_and_progress(
        &self,
        storage: &dyn crate::storage::Storage,
        remote_id: RemoteUuid,
        media_source: PushMediaSource<'_>,
        progress: Option<&dyn PushProgressObserver>,
    ) -> Result<SyncReportPush, LibraryError> {
        let runtime = self.inner.cloud_runtime.clone();
        let cloud = runtime.has_remote(&remote_id).then(|| CloudPushContext { runtime, remote_id });
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
            progress,
            cloud.as_ref(),
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
        progress: Option<&dyn PushProgressObserver>,
        cloud: Option<&CloudPushContext>,
    ) -> Result<SyncReportPush, LibraryError> {
        let master_key = &self.inner.master_key;

        verify_remote_identity(&access.storage.as_read(), remote_id).await?;
        verify_remote_library_format(&access.storage.as_read()).await?;
        let remote_id_string = remote_id.to_string();

        let relay_source = match &media_source {
            PushMediaSource::LocalOnly => None,
            PushMediaSource::FromRemote { remote_id, storage } => {
                verify_remote_identity(&storage, *remote_id).await?;
                verify_remote_library_format(storage).await?;
                Some((*remote_id, storage))
            }
            PushMediaSource::Plan(plan) => {
                for (id, source) in &plan.sources {
                    verify_remote_identity(source, *id).await?;
                    verify_remote_library_format(source).await?;
                }
                None
            }
        };

        // All work before the admission check is read-only. In particular, a rejected
        // Cloud quota must not publish master keys, operations, or media.
        let known_media: Vec<KnownMedia> = {
            let state = self.inner.state.read();
            state.media_entries().iter().map(|entry| KnownMedia { media_id: entry.media_id, storage_date: entry.storage_date, expects_thumb: entry.companion_kind.is_none() }).collect()
        };
        confirm_known_media(&access.storage.as_read(), &known_media, &remote_id_string, access.remote_media_list, &self.inner.remote_media_list_lock).await;
        let media_list = self.inner.remote_media_list_lock.with_lock(&remote_id_string, access.remote_media_list, |remote_media_list| MediaList::load_or_default(&remote_media_list.media_list_path()))?;
        let pending_data: Vec<_> = {
            let state = self.inner.state.read();
            state.media_entries().iter().filter(|entry| !media_list.has_full(&entry.media_id)).map(|entry| (entry.media_id, entry.storage_date)).collect()
        };
        let mut media_bytes = 0_u64;
        for (media_id, storage_date) in pending_data {
            let data_path = access.local_state_media_dir.data_path(storage_date.year, storage_date.month, &media_id);
            if let Ok(metadata) = std::fs::metadata(&data_path) {
                media_bytes = media_bytes.saturating_add(metadata.len());
                continue;
            }
            let key = format!("media/{}/{:02}/{}.data", storage_date.year, storage_date.month, media_id);
            let source = match &media_source {
                PushMediaSource::Plan(plan) => match plan.assignments.get(&(media_id, MediaBlob::Data)) {
                    Some(PlannedMediaSource::Remote(source_id)) => plan.sources.get(source_id).ok_or_else(|| SyncError::MissingMediaOnConfiguredSources(vec![media_id]))?,
                    _ => return Err(SyncError::MissingMediaOnConfiguredSources(vec![media_id]).into()),
                },
                PushMediaSource::FromRemote { storage, .. } => storage,
                PushMediaSource::LocalOnly => continue, // the normal missing-media error is returned below
            };
            media_bytes = media_bytes.saturating_add(u64::try_from(source.get(&key).await.map_err(SyncError::RemoteUnreachable)?.len()).unwrap_or(u64::MAX));
        }
        if let Some(cloud) = cloud {
            let usage = cloud.runtime.check_storage_usage(&cloud.remote_id, media_bytes).await
                .map_err(|error| SyncError::CloudQuotaExceeded(error.to_string()))?;
            if !usage.allowed {
                return Err(SyncError::CloudQuotaExceeded(format!("{} bytes requested with {} of {} bytes already indicated", usage.proposed_media_bytes, usage.approximate_used_bytes, usage.storage_quota_bytes)).into());
            }
        }

        // Master-key files are immutable credentials. Do not replace a pre-existing remote key.
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
            let key = format!("library/{name}");
            if access
                .storage
                .exists(&key)
                .await
                .map_err(SyncError::RemoteUnreachable)?
            {
                let remote_data = access
                    .storage
                    .get(&key)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
                if remote_data != data {
                    return Err(SyncError::RemoteOperationInvalid(format!(
                        "existing master-key file differs: {key}"
                    ))
                    .into());
                }
            } else {
                access
                    .storage
                    .put_atomic(&key, &data, AtomicWriteMode::Replace)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
            }
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

        // Confirm what the target already holds before deciding what to upload, so a blob
        // another client uploaded is neither reported missing nor sent a second time. This
        // lists media folders only, never operation files, so push still cannot turn into an
        // implicit fetch.
        /*let known_media: Vec<KnownMedia> = {
            let state = self.inner.state.read();
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
            &access.storage.as_read(),
            &known_media,
            &remote_id_string,
            access.remote_media_list,
            &self.inner.remote_media_list_lock,
        )
        .await;

        // Only the snapshot read is locked. Network storage awaits below must remain unlocked.
        let media_list = self.inner.remote_media_list_lock.with_lock(
            &remote_id_string,
            access.remote_media_list,
            |remote_media_list| MediaList::load_or_default(&remote_media_list.media_list_path()),
        )?;*/

        // Report missing media before uploading operations. The caller can then select a
        // source and retry without a partially completed default Push.
        if relay_source.is_none() && !matches!(media_source, PushMediaSource::Plan(_)) {
            let missing: Vec<_> = {
                let state = self.inner.state.read();
                state
                    .media_entries()
                    .iter()
                    .filter(|entry| !media_list.has_full(&entry.media_id))
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
                .map(|entry| FileToPush {
                    media_id: entry.media_id,
                    storage_date: entry.storage_date,
                    needs_data: !media_list.has_full(&entry.media_id),
                    // A companion resource never has a thumbnail, so asking a source for one
                    // would fail on every push for as long as the media exists.
                    needs_thumb: entry.companion_kind.is_none()
                        && !media_list.has_thumb(&entry.media_id),
                })
                .filter(|item| item.needs_data || item.needs_thumb)
                .collect()
        };

        let mut media_uploaded = 0usize;
        let media_upload_total = media_pending.iter().filter(|item| item.needs_data).count();
        if media_upload_total > 0 {
            if let Some(observer) = progress {
                observer.media_upload_progress(0, media_upload_total);
            }
        }

        let mut pending = media_pending.into_iter();
        let mut in_flight: FuturesUnordered<
            BoxFuture<'_, Result<MediaUploadOutcome, LibraryError>>,
        > = FuturesUnordered::new();
        self.fill_media_upload_slots(
            &mut pending,
            &mut in_flight,
            &access,
            &media_source,
            &relay_source,
            staging_dir.path(),
        );

        let mut first_error = None;
        while let Some(result) = in_flight.next().await {
            match result {
                Ok(outcome) => {
                    self.record_media_upload(
                        &remote_id_string,
                        access.remote_media_list,
                        &outcome,
                    )?;
                    if outcome.data_uploaded {
                        media_uploaded += 1;
                        if let Some(observer) = progress {
                            observer.media_upload_progress(media_uploaded, media_upload_total);
                        }
                    }
                    if first_error.is_none() {
                        self.fill_media_upload_slots(
                            &mut pending,
                            &mut in_flight,
                            &access,
                            &media_source,
                            &relay_source,
                            staging_dir.path(),
                        );
                    }
                }
                Err(error) => {
                    // Do not admit more work after an error. Existing workers are drained so a
                    // sibling that has already reached S3 is still recorded in the inventory.
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }

        if let Some(cloud) = cloud {
            cloud.runtime.confirm_storage_usage(&cloud.remote_id, media_bytes).await
                .map_err(|error| SyncError::CloudQuotaExceeded(error.to_string()))?;
        }
        Ok(SyncReportPush {
            ops_uploaded,
            media_uploaded,
            compactions_run,
        })
    }

    fn fill_media_upload_slots<'a>(
        &'a self,
        pending: &mut impl Iterator<Item = FileToPush>,
        in_flight: &mut FuturesUnordered<BoxFuture<'a, Result<MediaUploadOutcome, LibraryError>>>,
        access: &'a PushAccess<'a>,
        media_source: &'a PushMediaSource<'a>,
        relay_source: &'a Option<(RemoteUuid, &'a StorageRead<'a>)>,
        staging_dir: &'a std::path::Path,
    ) {
        while in_flight.len() < MAX_CONCURRENT_MEDIA_UPLOADS {
            let Some(item) = pending.next() else {
                break;
            };
            in_flight.push(
                self.upload_media_item(access, item, media_source, relay_source, staging_dir)
                    .boxed(),
            );
        }
    }

    async fn upload_media_item(
        &self,
        access: &PushAccess<'_>,
        item: FileToPush,
        media_source: &PushMediaSource<'_>,
        relay_source: &Option<(RemoteUuid, &StorageRead<'_>)>,
        staging_dir: &std::path::Path,
    ) -> Result<MediaUploadOutcome, LibraryError> {
        let (data_uploaded, data_present) = self
            .upload_media_data(access, &item, media_source, relay_source, staging_dir)
            .await?;
        let thumb_present = self
            .upload_media_thumbnail(access, &item, media_source, relay_source, staging_dir)
            .await?;

        Ok(MediaUploadOutcome {
            media_id: item.media_id,
            data_uploaded,
            data_present,
            thumb_present,
        })
    }

    async fn upload_media_data(
        &self,
        access: &PushAccess<'_>,
        item: &FileToPush,
        media_source: &PushMediaSource<'_>,
        relay_source: &Option<(RemoteUuid, &StorageRead<'_>)>,
        staging_dir: &std::path::Path,
    ) -> Result<(bool, bool), LibraryError> {
        if !item.needs_data {
            return Ok((false, false));
        }
        let data_key = format!(
            "media/{}/{:02}/{}.data",
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
            let (source_id, source) = match media_source {
                PushMediaSource::Plan(plan) => {
                    match plan.assignments.get(&(item.media_id, MediaBlob::Data)) {
                        Some(PlannedMediaSource::Remote(id)) => (
                            *id,
                            plan.sources.get(id).ok_or_else(|| {
                                SyncError::MissingMediaOnConfiguredSources(vec![item.media_id])
                            })?,
                        ),
                        _ => {
                            return Err(SyncError::MissingMediaOnConfiguredSources(vec![
                                item.media_id,
                            ])
                            .into());
                        }
                    }
                }
                _ => {
                    let (id, source) = relay_source
                        .as_ref()
                        .expect("missing local media requires a relay source");
                    (*id, *source)
                }
            };
            let downloaded =
                source
                    .get(&data_key)
                    .await
                    .map_err(|error| SyncError::SourceUnavailable {
                        source_remote_id: source_id,
                        media_id: item.media_id,
                        error,
                    })?;
            let bytes = stage_and_validate_media(
                staging_dir,
                &downloaded,
                item.media_id,
                &self.inner.master_key,
            )
            .map_err(|_| SyncError::CorruptRemoteMedia {
                source_remote_id: source_id,
                media_id: item.media_id,
            })?;
            (bytes.0, Some(bytes.1))
        };
        let data_uploaded = access
            .storage
            .put_atomic(&data_key, &bytes, AtomicWriteMode::Replace)
            .await
            .map_err(SyncError::RemoteUnreachable)?;
        if let Some(path) = staged_data {
            std::fs::remove_file(path)?;
        }
        Ok((data_uploaded, true))
    }

    async fn upload_media_thumbnail(
        &self,
        access: &PushAccess<'_>,
        item: &FileToPush,
        media_source: &PushMediaSource<'_>,
        relay_source: &Option<(RemoteUuid, &StorageRead<'_>)>,
        staging_dir: &std::path::Path,
    ) -> Result<bool, LibraryError> {
        if !item.needs_thumb {
            return Ok(false);
        }
        let thumb_key = format!(
            "media/{}/{:02}/{}.thumb",
            item.storage_date.year, item.storage_date.month, item.media_id
        );
        let thumb_path = access.local_state_media_dir.thumb_path(
            item.storage_date.year,
            item.storage_date.month,
            &item.media_id,
        );
        if thumb_path.exists() {
            let bytes = std::fs::read(&thumb_path)?;
            access
                .storage
                .put_atomic(&thumb_key, &bytes, AtomicWriteMode::Replace)
                .await
                .map_err(SyncError::RemoteUnreachable)?;
            return Ok(true);
        }
        let Some(source) = thumb_source(media_source, relay_source, item.media_id) else {
            return Ok(false);
        };
        match source.get(&thumb_key).await {
            Ok(downloaded) => {
                let (bytes, path) = stage_and_validate_media(
                    staging_dir,
                    &downloaded,
                    item.media_id,
                    &self.inner.master_key,
                )?;
                access
                    .storage
                    .put_atomic(&thumb_key, &bytes, AtomicWriteMode::Replace)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
                std::fs::remove_file(path)?;
                Ok(true)
            }
            Err(crate::storage::StorageError::NotFound) => Ok(false),
            Err(error) => Err(SyncError::RemoteUnreachable(error).into()),
        }
    }

    fn record_media_upload(
        &self,
        remote_id: &str,
        remote_media_list: &RemoteMediaList,
        outcome: &MediaUploadOutcome,
    ) -> Result<(), LibraryError> {
        self.inner.remote_media_list_lock.with_lock(
            remote_id,
            remote_media_list,
            |remote_media_list| {
                let path = remote_media_list.media_list_path();
                let mut media_list = MediaList::load_or_default(&path)?;
                if media_list.record(
                    outcome.media_id,
                    outcome.data_present,
                    outcome.thumb_present,
                ) {
                    media_list.save(&path)?;
                }
                Ok::<_, std::io::Error>(())
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
