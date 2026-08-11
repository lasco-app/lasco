use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

use crate::identifiers::{AlbumUuid, MediaUuid, OpUuid};
use crate::operations::{LibraryUsername, Operation, OperationGroup};
use crate::storage::{AtomicWriteMode, Storage, StorageMockMemory};

use super::super::remote_access::{StorageRead, StorageReadWrite};
use super::super::test_utils::{REMOTE_ID, make_library, stamp_remote_id, write_file};
use super::super::{PushMediaSource, SyncError};

const SOURCE_REMOTE_ID: &str = "22222222-2222-2222-2222-222222222222";

async fn stamp_source_remote(storage: &dyn Storage) {
    storage
        .put_atomic(
            &format!("remote_id_{SOURCE_REMOTE_ID}"),
            b"",
            AtomicWriteMode::CreateIfAbsent,
        )
        .await
        .unwrap();
}

fn remove_local_media(lib: &crate::library::Library, media_id: MediaUuid) {
    let entry = lib
        .inner
        .operation_state
        .read()
        .reconstructed
        .media
        .get(&media_id)
        .unwrap()
        .clone();
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

/// Inject `count` synthetic op groups directly into the main operations log, bypassing pending.
/// Used by compaction/tier tests that need N distinct groups to exercise push logic.
fn inject_op_groups(lib: &crate::library::Library, count: usize) {
    let local_dirs = &lib.inner.local_dirs;
    let master_key = &lib.inner.master_key;
    for _ in 0..count {
        let op_id = OpUuid::new();
        let media_id = MediaUuid::from_uuid(Uuid::new_v4());
        let group = OperationGroup {
            op_id,
            parent_op_id: None,
            author: LibraryUsername("test".into()),
            operations: vec![Operation::MediaCreation {
                timestamp: Utc::now(),
                media_id,
                filename_original: crate::operations::MediaFilename("x.jpg".into()),
                date: Utc::now(),
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
            }],
        };
        crate::operations::local_ops::append_op_group(
            &local_dirs.local_state_operations().operations_log_path(),
            master_key,
            &group,
        )
        .unwrap();
    }
}

/// Writes `count` fresh `.op1` compaction files (20 fake op groups each) directly to the
/// remote and records them in `lib`'s last known state, as if a prior push had put them
/// there. Used by tests that need tier-1 already close to its file limit.
async fn seed_tier1_files(
    lib: &crate::library::Library,
    storage: &StorageMockMemory,
    count: usize,
) {
    use crate::operations::remote_ops::write_compaction_file;
    use crate::operations::{CompactionEntry, CompactionFile};
    use crate::remote::LastKnownState;

    let t = chrono::Utc::now();
    let master_key = &lib.inner.master_key;

    for _ in 0..count {
        let comp_uuid = crate::identifiers::CompactedOpId::new();
        let mut contents = Vec::new();
        for _ in 0..20usize {
            let media_id = MediaUuid::from_uuid(Uuid::new_v4());
            let op_id = OpUuid::from_uuid(Uuid::new_v4());
            let group = OperationGroup {
                op_id,
                parent_op_id: None,
                author: LibraryUsername("test".to_string()),
                operations: vec![Operation::MediaCreation {
                    timestamp: Utc::now(),
                    media_id,
                    filename_original: crate::operations::MediaFilename("x.jpg".into()),
                    date: t,
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
                }],
            };
            contents.push(CompactionEntry { op_id, group });
        }
        let n = contents.len();
        let key = format!("operations/{comp_uuid}.op1_{n}");
        let comp_file = CompactionFile { tier: 1, contents };
        write_compaction_file(
            &StorageReadWrite::new(storage),
            master_key,
            &key,
            &comp_uuid,
            &comp_file,
        )
            .await
            .unwrap();
        LastKnownState::open(&lib.inner.local_dirs.remote_last_known_state_dir(REMOTE_ID))
            .unwrap()
            .write_compaction_file(master_key, &comp_uuid, 1, n as u32, &comp_file)
            .unwrap();
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

    // Inject 5 op groups directly into A's main log (bypassing pending).
    inject_op_groups(&lib_a, 5);

    // Simulate a partial push by reading groups from the log and uploading only the first 2 to storage,
    // recording them in the last known state cache as push itself would.
    let local_dirs = &lib_a.inner.local_dirs;
    let master_key = &lib_a.inner.master_key;

    let local_groups = crate::operations::local_ops::read_op_groups(
        &local_dirs.local_state_operations().operations_log_path(),
        master_key,
    )
    .unwrap();
    assert_eq!(local_groups.len(), 5, "should have 5 local ops");

    // Simulate a crash mid-push: upload the first 2 groups as a compaction file, and record
    // it in the cache (both steps happen together in a real push before a crash could occur).
    {
        use crate::operations::remote_ops::write_compaction_file;
        use crate::operations::{CompactionEntry, CompactionFile};
        use crate::remote::LastKnownState;
        let partial: Vec<CompactionEntry> = local_groups
            .iter()
            .take(2)
            .map(|g| CompactionEntry {
                op_id: g.op_id,
                group: g.clone(),
            })
            .collect();
        let partial_uuid = crate::identifiers::CompactedOpId::new();
        let partial_key = format!("operations/{partial_uuid}.op1_2");
        let partial_file = CompactionFile {
            tier: 1,
            contents: partial,
        };
        write_compaction_file(
            &StorageReadWrite::new(&storage),
            master_key,
            &partial_key,
            &partial_uuid,
            &partial_file,
        )
        .await
        .unwrap();
        LastKnownState::open(&local_dirs.remote_last_known_state_dir(REMOTE_ID))
            .unwrap()
            .write_compaction_file(master_key, &partial_uuid, 1, 2, &partial_file)
            .unwrap();
    }

    // push_impl determines coverage from the last known state cache, so it sees the 2
    // already-recorded ops and uploads only the remaining 3.
    let report = lib_a.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 3, "should upload remaining 3 ops");
}

#[tokio::test]
// An op covered by a compaction file recorded in the last known state must not be
// re-uploaded by push, since push determines coverage from that cache, not the remote.
async fn push_skips_ops_already_covered_by_compaction() {
    use crate::operations::remote_ops::write_compaction_file;
    use crate::operations::{CompactionEntry, CompactionFile};
    use crate::remote::LastKnownState;

    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    // Inject one op group directly into the main log (bypassing pending).
    inject_op_groups(&lib, 1);

    // Read the local op group.
    let local_groups = crate::operations::local_ops::read_op_groups(
        &lib.inner
            .local_dirs
            .local_state_operations()
            .operations_log_path(),
        &lib.inner.master_key,
    )
    .unwrap();
    assert_eq!(local_groups.len(), 1);
    let group = local_groups.into_iter().next().unwrap();

    // Write a compaction file covering this op to the remote and record it in the last
    // known state cache, as a prior successful push would have done.
    let comp_uuid = crate::identifiers::CompactedOpId::new();
    let comp_file = CompactionFile {
        tier: 1,
        contents: vec![CompactionEntry {
            op_id: group.op_id,
            group,
        }],
    };
    let comp_key = format!("operations/{comp_uuid}.op1_{}", comp_file.contents.len());
    write_compaction_file(
        &StorageReadWrite::new(&storage),
        &lib.inner.master_key,
        &comp_key,
        &comp_uuid,
        &comp_file,
    )
    .await
    .unwrap();
    LastKnownState::open(&lib.inner.local_dirs.remote_last_known_state_dir(REMOTE_ID))
        .unwrap()
        .write_compaction_file(&lib.inner.master_key, &comp_uuid, 1, 1, &comp_file)
        .unwrap();

    // Push must see the op as already covered and upload nothing.
    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        report.ops_uploaded, 0,
        "op covered by compaction must not be re-uploaded"
    );
}

/// Injects a single op group containing `op_count` operations directly into the main
/// operations log, bypassing pending. Simulates a batch importer that bundles many
/// operations into one group per push.
fn inject_single_large_group(lib: &crate::library::Library, op_count: usize) {
    let local_dirs = &lib.inner.local_dirs;
    let master_key = &lib.inner.master_key;
    let operations = (0..op_count)
        .map(|_| Operation::MediaCreation {
            timestamp: Utc::now(),
            media_id: MediaUuid::from_uuid(Uuid::new_v4()),
            filename_original: crate::operations::MediaFilename("x.jpg".into()),
            date: Utc::now(),
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
        })
        .collect();
    let group = OperationGroup {
        op_id: OpUuid::new(),
        parent_op_id: None,
        author: LibraryUsername("test".into()),
        operations,
    };
    crate::operations::local_ops::append_op_group(
        &local_dirs.local_state_operations().operations_log_path(),
        master_key,
        &group,
    )
    .unwrap();
}

#[tokio::test]
// A single op group holding many operations (as a batch importer would produce) must land
// at the tier sized for its total operation count, not at tier 1 just because it is one group.
async fn push_single_large_group_lands_at_tier_sized_for_its_op_count() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    // One group, 200 operations: exceeds tier 1's 20-op limit even though it's a single group.
    inject_single_large_group(&lib, 200);

    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        report.ops_uploaded, 1,
        "a single group counts as one op group uploaded"
    );

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
        "a group with 200 operations must not land at tier 1"
    );
    assert_eq!(
        comp2_count, 1,
        "a group with 200 operations must land at tier 2, sized for its op count"
    );
}

#[tokio::test]
// A batch within tier 1's ops limit is uploaded as a single .op1 compaction file.
async fn push_small_batch_writes_compaction_file() {
    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    inject_op_groups(&lib, 5);

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

    inject_op_groups(&lib, 20);

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

    inject_op_groups(&lib, 200);

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
    use crate::operations::remote_ops::write_compaction_file;
    use crate::operations::{CompactionEntry, CompactionFile, Operation, OperationGroup};
    use crate::remote::LastKnownState;

    let storage = StorageMockMemory::new();
    stamp_remote_id(&storage).await;
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    let t = chrono::Utc::now();
    let master_key = &lib.inner.master_key;

    // Write 9 existing .op1 files to the remote (each covering 20 fake ops).
    for _ in 0..9usize {
        let comp_uuid = crate::identifiers::CompactedOpId::new();
        let mut contents = Vec::new();
        for _ in 0..20usize {
            let media_id = MediaUuid::from_uuid(Uuid::new_v4());
            let op_id = OpUuid::from_uuid(Uuid::new_v4());
            let group = OperationGroup {
                op_id,
                parent_op_id: None,
                author: LibraryUsername("test".to_string()),
                operations: vec![Operation::MediaCreation {
                    timestamp: Utc::now(),
                    media_id,
                    filename_original: crate::operations::MediaFilename("x.jpg".into()),
                    date: t,
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
                }],
            };
            contents.push(CompactionEntry { op_id, group });
        }
        let n = contents.len();
        let key = format!("operations/{comp_uuid}.op1_{n}");
        let comp_file = CompactionFile { tier: 1, contents };
        write_compaction_file(
            &StorageReadWrite::new(&storage),
            master_key,
            &key,
            &comp_uuid,
            &comp_file,
        )
            .await
            .unwrap();
        LastKnownState::open(&lib.inner.local_dirs.remote_last_known_state_dir(REMOTE_ID))
            .unwrap()
            .write_compaction_file(master_key, &comp_uuid, 1, n as u32, &comp_file)
            .unwrap();
    }

    // Push 20 new op groups. They go as .op1, bringing tier-1 to 10 and triggering a cascade to .op2.
    inject_op_groups(&lib, 20);

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
        &lib.inner.local_dirs.remote_last_known_state_dir(REMOTE_ID),
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

    inject_op_groups(&lib, 5);
    lib.push(&storage, REMOTE_ID).await.unwrap();

    let remote_files = crate::operations::remote_ops::list_remote_op_files(&StorageRead::new(&storage))
        .await
        .unwrap();
    let mut cached_files = crate::remote::LastKnownState::list_cached_files(
        &lib.inner.local_dirs.remote_last_known_state_dir(REMOTE_ID),
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

    inject_op_groups(&lib, 20);
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
        .remote_media_list(REMOTE_ID)
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

    inject_op_groups(&lib, 1);

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
    let entry = lib.inner.operation_state.read().reconstructed.media[&media_id].clone();
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
            .remote_media_list(SOURCE_REMOTE_ID)
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
    let entry = lib.inner.operation_state.read().reconstructed.media[&media_id].clone();
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
