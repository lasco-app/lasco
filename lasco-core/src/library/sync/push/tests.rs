use tempfile::TempDir;
use uuid::Uuid;

use crate::crdt::CrdtOperation;
use crate::identifiers::{AlbumUuid, MediaUuid, RemoteUuid};
use crate::storage::{AtomicWriteMode, Storage, StorageMockMemory};

use super::super::remote_access::{StorageRead, StorageReadWrite};
use super::super::test_utils::{REMOTE_ID, make_library, stamp_remote, stamp_remote_id, write_file};
use super::super::{PushMediaSource, SyncError};

const SOURCE_REMOTE_ID: RemoteUuid =
    RemoteUuid(Uuid::from_u128(0x2222_2222_2222_2222_2222_2222_2222_2222));

async fn stamp_source_remote(storage: &dyn Storage) {
    stamp_remote(storage, SOURCE_REMOTE_ID).await;
}

fn remove_local_media(lib: &crate::library::Library, media_id: MediaUuid) {
    let entry = lib.inner.state.read().media(media_id).unwrap();
    let media_dir = lib.inner.local_dirs.local_state_media_dir();
    std::fs::remove_file(media_dir.data_path(
        entry.storage_date.year,
        entry.storage_date.month,
        &media_id,
    ))
    .unwrap();
    let thumb = media_dir.thumb_path(entry.storage_date.year, entry.storage_date.month, &media_id);
    if thumb.exists() {
        std::fs::remove_file(thumb).unwrap();
    }
}

/// Records `count` media, each outside any album so each contributes exactly one
/// operation to the log. Media bytes land in the local cache, which push requires.
async fn add_media(lib: &crate::library::Library, tmp: &TempDir, count: usize) {
    for i in 0..count {
        let src = write_file(
            tmp.path(),
            &format!("op_{i}.jpg"),
            format!("data-{i}").as_bytes(),
        );
        lib.media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    }
}

/// Builds `count` synthetic operations that no client has ever pushed. They only
/// ever go into compaction files, never into a local log, so no media bytes exist
/// for them and push never has to upload any.
fn synthetic_operations(count: usize) -> Vec<CrdtOperation> {
    use crate::crdt::{DeviceId, Dot, MediaCreation, OperationContent};

    (0..count)
        .map(|i| CrdtOperation {
            dot: Dot {
                device_id: DeviceId(u128::MAX),
                lamport_counter: u64::try_from(i).unwrap() + 1,
            },
            author: crate::operations::LibraryUsername("test".into()),
            timestamp: chrono::Utc::now(),
            content: OperationContent::MediaCreation(MediaCreation {
                media_id: MediaUuid::from_uuid(Uuid::new_v4()),
                filename_original: crate::operations::MediaFilename("x.jpg".into()),
                date: chrono::Utc::now(),
                storage_date: crate::operations::StorageDate {
                    year: 2024,
                    month: 1,
                },
                size_bytes: 100,
                content_hash: crate::library::media::MediaHash::zeroed(),
                modified_at: None,
                gps: None,
                apple_aae_media_id: None,
                apple_live_photo_media_id: None,
            }),
        })
        .collect()
}

/// Writes one tier 1 compaction file holding `operations` to the remote and records
/// it in `lib`'s last known state for `REMOTE_ID`, exactly as a prior push would have.
async fn seed_compaction_file(
    lib: &crate::library::Library,
    storage: &StorageMockMemory,
    operations: Vec<CrdtOperation>,
) {
    use crate::operations::CompactionFile;
    use crate::remote::LastKnownState;

    let master_key = &lib.inner.master_key;
    let comp_uuid = crate::identifiers::CompactedOpId::new();
    let op_count = u32::try_from(operations.len()).unwrap();
    let comp_file = CompactionFile {
        tier: 1,
        operations,
    };
    let bytes = crate::operations::encrypt_compaction_file(master_key, &comp_uuid, &comp_file)
        .unwrap()
        .to_bytes();
    let key = format!("operations/{comp_uuid}.op1_{op_count}");
    crate::operations::remote_ops::write_compaction_bytes(
        &StorageReadWrite::new(storage),
        &key,
        &bytes,
    )
    .await
    .unwrap();
    LastKnownState::open(
        &lib.inner
            .local_dirs
            .remote_last_known_state_dir(&REMOTE_ID.to_string()),
    )
    .unwrap()
    .write_compaction_bytes(&comp_uuid, 1, op_count, &bytes)
    .unwrap();
}

/// Writes `count` fresh `.op1` compaction files (20 synthetic operations each) to the
/// remote and to `lib`'s last known state. Used by tests that need tier 1 already close
/// to its file limit.
async fn seed_tier1_files(lib: &crate::library::Library, storage: &StorageMockMemory, count: usize) {
    for _ in 0..count {
        seed_compaction_file(lib, storage, synthetic_operations(20)).await;
    }
}

#[tokio::test]
// push with no pending ops or files returns zero counts.
async fn push_no_pending_returns_zeros() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 0);
    assert_eq!(report.media_uploaded, 0);
}

#[tokio::test]
// push calls storage.list() exactly once per invocation, to verify remote identity.
// It never lists remote op files: coverage and compaction decisions come from the
// local last known state cache only. Media uses media_list.json.
async fn push_calls_list_on_every_invocation() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    lib.push(&storage, REMOTE_ID).await.unwrap();
    let after_first = storage.list_call_count();
    lib.push(&storage, REMOTE_ID).await.unwrap();
    let after_second = storage.list_call_count();

    assert_eq!(
        after_first, 1,
        "first push: 1 list call (remote identity only)"
    );
    assert_eq!(
        after_second, 2,
        "second push: 1 more list call regardless of prior state"
    );
}

#[tokio::test]
// push with an unreachable remote returns RemoteUnreachable. LocalIndex is unchanged.
async fn push_with_unreachable_remote_returns_error() {
    use crate::error::SyncError;

    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp.path(), "offline.jpg", b"data");
    lib.media_add(
        crate::library::media::upload::MediaAddSource::CopyFrom(src),
        Some(album_id),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Go offline before pushing.
    storage.set_offline(true);

    let err = lib.push(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        matches!(
            err,
            crate::error::LibraryError::Sync(SyncError::RemoteUnreachable(_))
        ),
        "expected RemoteUnreachable, got: {err:?}"
    );
}

#[tokio::test]
// media_add succeeds with no remote (offline).
async fn media_add_succeeds_offline() {
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp.path(), "offline.jpg", b"data");
    lib.media_add(
        crate::library::media::upload::MediaAddSource::CopyFrom(src),
        Some(album_id),
        None,
        None,
        None,
    )
    .await
    .expect("media_add must succeed even when remote is unreachable");
}

#[tokio::test]
// Simulate a crash mid-push: 2 of 5 ops were uploaded and recorded in the last known
// state before the crash. The second push uploads only the remaining 3, since push
// determines coverage from its own cache, not by listing the remote.
async fn incremental_push_resilience_uploads_remaining_after_failure() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp_a = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    add_media(&lib_a, &tmp_a, 5).await;

    let local_operations = lib_a.list_operations().unwrap();
    assert_eq!(local_operations.len(), 5, "should have 5 local ops");

    // Simulate a crash mid-push: upload the first 2 operations as a compaction file and
    // record it in the cache (both steps happen together in a real push).
    seed_compaction_file(
        &lib_a,
        &storage,
        local_operations.into_iter().take(2).collect(),
    )
    .await;

    // push_impl determines coverage from the last known state cache, so it sees the 2
    // already-recorded ops and uploads only the remaining 3.
    let report = lib_a.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 3, "should upload remaining 3 ops");
}

#[tokio::test]
// An op covered by a compaction file recorded in the last known state must not be
// re-uploaded by push, since push determines coverage from that cache, not the remote.
async fn push_skips_ops_already_covered_by_compaction() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    add_media(&lib, &tmp, 1).await;

    let local_operations = lib.list_operations().unwrap();
    assert_eq!(local_operations.len(), 1);

    // Write a compaction file covering this op to the remote and record it in the last
    // known state cache, as a prior successful push would have done.
    seed_compaction_file(&lib, &storage, local_operations).await;

    // Push must see the op as already covered and upload nothing.
    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        report.ops_uploaded, 0,
        "op covered by compaction must not be re-uploaded"
    );
}

#[tokio::test]
// A batch within tier 1's ops limit is uploaded as a single .op1 compaction file.
async fn push_small_batch_writes_compaction_file() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    add_media(&lib, &tmp, 5).await;

    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 5);
    assert_eq!(report.compactions_run, 0, "no compaction for small batch");

    let remote_files = crate::operations::remote_ops::list_remote_op_files(&StorageRead::new(&storage))
        .await
        .unwrap();
    let comp1_count = remote_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 1, .. }
            )
        })
        .count();
    assert_eq!(comp1_count, 1, "must have exactly 1 .op1 file");
}

#[tokio::test]
// When N >= 20 ops, push writes a single .opT compaction file at the correct tier.
async fn push_large_batch_writes_single_compaction_file() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    add_media(&lib, &tmp, 20).await;

    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 20);

    let remote_files = crate::operations::remote_ops::list_remote_op_files(&StorageRead::new(&storage))
        .await
        .unwrap();
    let comp1_count = remote_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 1, .. }
            )
        })
        .count();
    assert_eq!(comp1_count, 1, "exactly one .op1 file for batch of 20");
}

#[tokio::test]
// A batch too large for tier 1's ops limit (20) is uploaded directly at the tier
// whose ops limit fits it, still as a single file.
async fn push_large_batch_writes_op2_file() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    add_media(&lib, &tmp, 200).await;

    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 200);

    let remote_files = crate::operations::remote_ops::list_remote_op_files(&StorageRead::new(&storage))
        .await
        .unwrap();
    let comp1_count = remote_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 1, .. }
            )
        })
        .count();
    let comp2_count = remote_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 2, .. }
            )
        })
        .count();
    assert_eq!(
        comp1_count, 0,
        "batch of 200 exceeds tier 1's ops limit of 20"
    );
    assert_eq!(
        comp2_count, 1,
        "batch of 200 fits tier 2's ops limit of 200"
    );
}

#[tokio::test]
// With 9 existing .op1 files already recorded in the last known state, one new push
// creates the 10th, triggering tier-1 compaction.
async fn compaction_cascades_across_two_tiers() {
    use crate::remote::LastKnownState;

    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    // Write 9 existing .op1 files to the remote (each covering 20 synthetic ops).
    seed_tier1_files(&lib, &storage, 9).await;

    // Push 20 new ops. They go as .op1, bringing tier-1 to 10 and triggering a cascade to .op2.
    add_media(&lib, &tmp, 20).await;

    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 20);
    assert!(
        report.compactions_run >= 1,
        "at least one compaction must run"
    );

    let remote_files = crate::operations::remote_ops::list_remote_op_files(&StorageRead::new(&storage))
        .await
        .unwrap();
    let op1_count = remote_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 1, .. }
            )
        })
        .count();
    let op2_count = remote_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 2, .. }
            )
        })
        .count();
    assert_eq!(op1_count, 0, "all .op1 files must be merged into .op2");
    assert_eq!(op2_count, 1, "one .op2 file must exist after cascade");

    // The last known state cache must exactly mirror the remote after compaction: no
    // leftover tier-1 entries, and the merged tier-2 file recorded.
    let cached_files = LastKnownState::list_cached_files(
        &lib.inner.local_dirs.remote_last_known_state_dir(&REMOTE_ID.to_string()),
    )
    .unwrap();
    let cached_op1_count = cached_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 1, .. }
            )
        })
        .count();
    let cached_op2_count = cached_files
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::operations::remote_ops::RemoteOpFile::Compaction { tier: 2, .. }
            )
        })
        .count();
    assert_eq!(
        cached_op1_count, 0,
        "cache must not keep stale tier-1 entries after compaction"
    );
    assert_eq!(
        cached_op2_count, 1,
        "cache must record the merged tier-2 file"
    );
}

#[tokio::test]
// Push records the file it uploads in the last known state cache, without needing
// a subsequent fetch, so a following push (or another push to the same remote) sees it.
async fn push_records_uploaded_file_in_last_known_state() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    add_media(&lib, &tmp, 5).await;
    lib.push(&storage, REMOTE_ID).await.unwrap();

    let remote_files = crate::operations::remote_ops::list_remote_op_files(&StorageRead::new(&storage))
        .await
        .unwrap();
    let mut cached_files = crate::remote::LastKnownState::list_cached_files(
        &lib.inner.local_dirs.remote_last_known_state_dir(&REMOTE_ID.to_string()),
    )
    .unwrap();
    let mut remote_files = remote_files;
    let sort_key = |f: &crate::operations::remote_ops::RemoteOpFile| {
        let crate::operations::remote_ops::RemoteOpFile::Compaction { uuid, .. } = f;
        *uuid
    };
    cached_files.sort_by_key(sort_key);
    remote_files.sort_by_key(sort_key);
    assert_eq!(
        cached_files, remote_files,
        "last known state cache must match the remote right after push, with no fetch"
    );
}

#[tokio::test]
// If another client holds the compaction lock, push skips the whole cascade rather than
// erroring, and leaves the other client's lock untouched.
async fn push_skips_cascade_when_lock_held() {
    use crate::operations::remote_ops::RemoteOpFile;

    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    // Seed 9 existing .op1 files so the 10th, pushed below, would normally trigger
    // tier-1 compaction (same setup as `compaction_cascades_across_two_tiers`).
    seed_tier1_files(&lib, &storage, 9).await;

    // Manually place a lock as if another client is mid-compaction.
    storage
        .put_atomic("operations/LOCK.op", b"lock", AtomicWriteMode::Replace)
        .await
        .unwrap();

    add_media(&lib, &tmp, 20).await;
    let report = lib.push(&storage, REMOTE_ID).await.unwrap();

    assert_eq!(report.ops_uploaded, 20);
    assert_eq!(
        report.compactions_run, 0,
        "no compaction must run while the lock is held by another client"
    );
    assert!(
        storage.exists("operations/LOCK.op").await.unwrap(),
        "the other client's lock must still be present"
    );

    let remote_files = crate::operations::remote_ops::list_remote_op_files(&StorageRead::new(&storage))
        .await
        .unwrap();
    let op1_count = remote_files
        .iter()
        .filter(|f| matches!(f, RemoteOpFile::Compaction { tier: 1, .. }))
        .count();
    assert_eq!(
        op1_count, 10,
        "tier-1 files must simply accumulate to 10, uncompacted, while the lock is held"
    );
}

#[tokio::test]
// After push uploads a media file, media_list.json contains that media_id.
async fn push_writes_media_list_after_upload() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp.path(), "img.jpg", b"pixels");
    let media_id = lib
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();
    lib.push(&storage, REMOTE_ID).await.unwrap();

    let media_list_path = lib
        .inner
        .local_dirs
        .remote_media_list(&REMOTE_ID.to_string())
        .media_list_path();
    let media_list =
        crate::remote::local_state::media_list_json::MediaList::load_or_default(&media_list_path)
            .unwrap();
    assert!(
        media_list.contains(&media_id),
        "media_list.json must contain the uploaded media_id"
    );
}

#[tokio::test]
// Second push does not re-upload media already in media_list.json.
async fn push_skips_media_already_in_media_list() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp.path(), "img.jpg", b"data");
    lib.media_add(
        crate::library::media::upload::MediaAddSource::CopyFrom(src),
        Some(album_id),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let r1 = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(r1.media_uploaded, 1, "first push must upload 1 file");

    // Push again — nothing new.
    let r2 = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        r2.media_uploaded, 0,
        "second push must not re-upload media already in media_list"
    );
}

#[tokio::test]
// A corrupt frame in operations.log must abort push instead of being silently skipped.
async fn push_errors_on_corrupt_local_frame() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    add_media(&lib, &tmp, 1).await;

    // Corrupt the blob version byte of the single frame.
    let log_path = lib
        .inner
        .local_dirs
        .local_state_operations()
        .operations_log_path();
    let mut data = std::fs::read(&log_path).unwrap();
    assert!(data.len() > 20, "log must contain at least one frame");
    data[20] = 0;
    std::fs::write(&log_path, &data).unwrap();

    let result = lib.push(&storage, REMOTE_ID).await;
    assert!(result.is_err(), "push must abort on a corrupt local frame");
}

#[tokio::test]
async fn push_without_relay_reports_missing_local_media() {
    let source = StorageMockMemory::new();
    stamp_source_remote(&source).await;
    let target = StorageMockMemory::new();
    stamp_remote_id(&target).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;
    let media_id = lib
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(write_file(
                tmp.path(),
                "relay.jpg",
                b"pixels",
            )),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();
    lib.push(&source, SOURCE_REMOTE_ID).await.unwrap();
    remove_local_media(&lib, media_id);

    let error = lib.push(&target, REMOTE_ID).await.unwrap_err();
    assert!(
        matches!(error, crate::error::LibraryError::Sync(SyncError::MissingLocalMedia(ids)) if ids == vec![media_id])
    );
}

#[tokio::test]
async fn push_relays_selected_source_without_caching_media_locally() {
    let source = StorageMockMemory::new();
    stamp_source_remote(&source).await;
    let target = StorageMockMemory::new();
    stamp_remote_id(&target).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;
    let media_id = lib
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(write_file(
                tmp.path(),
                "relay.jpg",
                b"pixels",
            )),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();
    lib.push(&source, SOURCE_REMOTE_ID).await.unwrap();
    remove_local_media(&lib, media_id);

    let report = lib
        .push_with_media_source(
            &target,
            REMOTE_ID,
            PushMediaSource::FromRemote {
                remote_id: SOURCE_REMOTE_ID,
                storage: StorageRead::new(&source),
            },
        )
        .await
        .unwrap();
    assert_eq!(report.media_uploaded, 1);
    let entry = lib.inner.state.read().media(media_id).unwrap();
    assert!(
        target
            .exists(&format!(
                "media/{}/{:02}/{media_id}.data",
                entry.storage_date.year, entry.storage_date.month
            ))
            .await
            .unwrap()
    );
    assert!(
        !lib.inner
            .local_dirs
            .local_state_media_dir()
            .data_path(entry.storage_date.year, entry.storage_date.month, &media_id)
            .exists()
    );
    assert!(
        lib.inner
            .local_dirs
            .remote_media_list(&SOURCE_REMOTE_ID.to_string())
            .media_list_path()
            .exists()
    );
    assert!(
        !lib.inner
            .local_dirs
            .local_state_media_dir()
            .path()
            .join(".push-staging")
            .exists(),
        "relay staging must not be stored in library media state"
    );
}

#[tokio::test]
async fn corrupt_relay_media_is_not_uploaded_to_target() {
    let source = StorageMockMemory::new();
    stamp_source_remote(&source).await;
    let target = StorageMockMemory::new();
    stamp_remote_id(&target).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;
    let media_id = lib
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(write_file(
                tmp.path(),
                "relay.jpg",
                b"pixels",
            )),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();
    lib.push(&source, SOURCE_REMOTE_ID).await.unwrap();
    let entry = lib.inner.state.read().media(media_id).unwrap();
    let key = format!(
        "media/{}/{:02}/{media_id}.data",
        entry.storage_date.year, entry.storage_date.month
    );
    source
        .put_atomic(&key, b"corrupt", AtomicWriteMode::Replace)
        .await
        .unwrap();
    remove_local_media(&lib, media_id);

    assert!(
        lib.push_with_media_source(
            &target,
            REMOTE_ID,
            PushMediaSource::FromRemote {
                remote_id: SOURCE_REMOTE_ID,
                storage: StorageRead::new(&source)
            }
        )
        .await
        .is_err()
    );
    assert!(!target.exists(&key).await.unwrap());
}

#[tokio::test]
async fn push_propagates_master_key_files_without_overwriting() {
    let target = StorageMockMemory::new();
    stamp_remote_id(&target).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;
    let library_dir = lib.inner.local_dirs.local_state_library_dir();
    let name = std::fs::read_dir(library_dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().into_string().unwrap())
        .find(|name| name.starts_with("mk_") && name.ends_with(".enc"))
        .unwrap();
    let local = std::fs::read(library_dir.path().join(&name)).unwrap();
    target
        .put_atomic(
            &format!("library/{name}"),
            b"existing",
            AtomicWriteMode::Replace,
        )
        .await
        .unwrap();

    lib.push(&target, REMOTE_ID).await.unwrap();
    assert_eq!(
        target.get(&format!("library/{name}")).await.unwrap(),
        b"existing"
    );

    let fresh_target = StorageMockMemory::new();
    stamp_remote_id(&fresh_target).await;
    lib.push(&fresh_target, REMOTE_ID).await.unwrap();
    assert_eq!(
        fresh_target.get(&format!("library/{name}")).await.unwrap(),
        local
    );
}
