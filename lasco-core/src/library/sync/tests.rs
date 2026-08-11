use std::collections::HashSet;

use tempfile::TempDir;
use uuid::Uuid;

use chrono::{TimeZone, Utc};

use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid, OpUuid};
use crate::library::Library;
use crate::library::media::query::MediaListScope;
use crate::operations::{LibraryUsername, Operation, OperationGroup};
use crate::storage::StorageMockMemory;

use super::test_utils::{
    REMOTE_ID, make_library, make_library_with_same_keys, remote_uuid, write_file,
};

#[tokio::test]
// After Client A pushes and Client B fetches, the file is present in B's state.
async fn push_then_fetch_syncs_state() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp_a.path(), "photo.jpg", b"image data");
    let media_id = lib_a
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
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    lib_b
        .media_show(media_id)
        .expect("media must be in B's state after fetch");
}

#[tokio::test]
// When both clients add files and then both sync, both see all files.
async fn both_clients_sync_converge() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());

    let src_a = write_file(tmp_a.path(), "a.jpg", b"from a");
    let mid_a = lib_a
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_a),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();

    let src_b = write_file(tmp_b.path(), "b.jpg", b"from b");
    let mid_b = lib_b
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_b),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();

    lib_a.sync(&storage, REMOTE_ID).await.unwrap();
    lib_b.sync(&storage, REMOTE_ID).await.unwrap();
    // A must re-fetch to see B's op (B pushed after A's sync).
    lib_a.fetch(&storage, REMOTE_ID).await.unwrap();

    lib_a.media_show(mid_a).expect("A must have mid_a");
    lib_a
        .media_show(mid_b)
        .expect("A must have mid_b after sync");
    lib_b
        .media_show(mid_a)
        .expect("B must have mid_a after sync");
    lib_b.media_show(mid_b).expect("B must have mid_b");
}

#[tokio::test]
// Two concurrent sync calls where one succeeds and the other returns AlreadyRunning.
// Tasks interleave at .await points even on the single-threaded runtime.
async fn concurrent_sync_one_returns_already_running() {
    use crate::error::SyncError;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();
    let lib = Arc::new(make_library(&tmp).await);
    lib.initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();

    let success_count = Arc::new(AtomicUsize::new(0));
    let already_running_count = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..2)
        .map(|_| {
            let lib = Arc::clone(&lib);
            let storage = storage.clone();
            let success_count = Arc::clone(&success_count);
            let already_running_count = Arc::clone(&already_running_count);
            tokio::spawn(async move {
                match lib.sync(&storage, REMOTE_ID).await {
                    Ok(_) => {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(crate::error::LibraryError::Sync(SyncError::AlreadyRunning)) => {
                        already_running_count.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(e) => panic!("unexpected error: {e}"),
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let s = success_count.load(Ordering::SeqCst);
    let r = already_running_count.load(Ordering::SeqCst);
    assert_eq!(s + r, 2, "both calls must resolve");
    assert!(s >= 1, "at least one sync must succeed");
}

#[tokio::test]
// A fetch is exclusive across all remotes, not just the one it targets, since two
// fetches racing on different remotes could both append the same op group from
// stale local-op-log snapshots.
async fn fetch_rejects_across_different_remotes() {
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    let _remote_guard = lib
        .try_acquire_remote_sync("other-remote")
        .expect("remote lock must be free");
    let _fetch_guard = lib
        .try_acquire_fetch_slot()
        .expect("fetch slot must be free");

    assert!(
        lib.try_acquire_fetch_slot().is_none(),
        "a second fetch must be rejected even for a different remote"
    );

    // Meanwhile a push against a distinct remote is unaffected.
    assert!(
        lib.try_acquire_remote_sync(REMOTE_ID).is_some(),
        "push/fetch against a different remote must still be allowed"
    );
}

#[tokio::test]
// media_add is not blocked by the sync lock (different lock path).
async fn media_add_not_blocked_by_sync_lock() {
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;

    // Acquire the sync lock manually to simulate a sync in progress.
    let _guard = lib
        .try_acquire_remote_sync(REMOTE_ID)
        .expect("sync lock must be free");

    // media_add should still succeed while the sync lock is held.
    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp.path(), "blocked.jpg", b"data");
    lib.media_add(
        crate::library::media::upload::MediaAddSource::CopyFrom(src),
        Some(album_id),
        None,
        None,
        None,
    )
    .await
    .expect("media_add must not be blocked by the sync lock");
}

#[tokio::test]
// Partial sync where fetch succeeds then push fails. Fetch results are committed and push pending ops remain.
async fn sync_partial_fetch_ok_push_fails() {
    use crate::error::SyncError;

    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    // A pushes a file.
    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src_a = write_file(tmp_a.path(), "a.jpg", b"from a");
    let mid_a = lib_a
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_a),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    // B adds a local file (pending upload).
    let src_b = write_file(tmp_b.path(), "b.jpg", b"from b");
    lib_b
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_b),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    lib_b
        .media_show(mid_a)
        .expect("B must have mid_a after fetch");

    // Now go offline and try to push.
    storage.set_offline(true);
    let err = lib_b.push(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        matches!(
            err,
            crate::error::LibraryError::Sync(SyncError::RemoteUnreachable(_))
        ),
        "push must fail with RemoteUnreachable"
    );

    // Fetch results are still committed (B still has mid_a).
    lib_b
        .media_show(mid_a)
        .expect("fetch results must still be present after push failure");
}

#[tokio::test]
// Three clients interleaving media_add and album_create all converge after syncing.
async fn three_clients_converge_with_albums() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();
    let tmp_c = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;
    let lib_c = make_library_with_same_keys(&tmp_c, &lib_a).await;

    let album_a = AlbumUuid::from_uuid(Uuid::new_v4());
    let album_b = AlbumUuid::from_uuid(Uuid::new_v4());
    let album_c = AlbumUuid::from_uuid(Uuid::new_v4());

    let src_a = write_file(tmp_a.path(), "a.jpg", b"from a");
    let mid_a = lib_a
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_a),
            Some(album_a),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();

    let src_b = write_file(tmp_b.path(), "b.jpg", b"from b");
    let mid_b = lib_b
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_b),
            Some(album_b),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();

    let src_c = write_file(tmp_c.path(), "c.jpg", b"from c");
    let mid_c = lib_c
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_c),
            Some(album_c),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();

    lib_a.sync(&storage, REMOTE_ID).await.unwrap();
    lib_b.sync(&storage, REMOTE_ID).await.unwrap();
    lib_c.sync(&storage, REMOTE_ID).await.unwrap();

    lib_a.fetch(&storage, REMOTE_ID).await.unwrap();
    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    lib_c.fetch(&storage, REMOTE_ID).await.unwrap();

    for lib in [&lib_a, &lib_b, &lib_c] {
        lib.media_show(mid_a).expect("must have mid_a");
        lib.media_show(mid_b).expect("must have mid_b");
        lib.media_show(mid_c).expect("must have mid_c");
    }
}

#[tokio::test]
// Sync round-trip where both clients add media, both sync, and then both have all ops.
async fn sync_round_trip_both_have_all_ops() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());

    let src_a = write_file(tmp_a.path(), "a.jpg", b"from a");
    let mid_a = lib_a
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_a),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();

    let src_b = write_file(tmp_b.path(), "b.jpg", b"from b");
    let mid_b = lib_b
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src_b),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();

    lib_a.sync(&storage, REMOTE_ID).await.unwrap();
    lib_b.sync(&storage, REMOTE_ID).await.unwrap();

    lib_a.fetch(&storage, REMOTE_ID).await.unwrap();

    lib_a.media_show(mid_a).expect("A must have mid_a");
    lib_a.media_show(mid_b).expect("A must have mid_b");
    lib_b.media_show(mid_a).expect("B must have mid_a");
    lib_b.media_show(mid_b).expect("B must have mid_b");
}

#[tokio::test]
// Reachability convergence where A removes a file from its last album while B removes it from
// its last group. After both fetch, the file is unreachable on both sides.
async fn reachability_convergence_file_removed_from_last_album_and_group() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let media_id = MediaUuid::from_uuid(Uuid::new_v4());
    let group_id = GroupUuid::from_uuid(Uuid::new_v4());
    let date = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

    async fn write_op(lib: &Library, ops: Vec<Operation>) -> OpUuid {
        let op_id = OpUuid::new();
        let op_group = OperationGroup {
            op_id,
            parent_op_id: None,
            author: LibraryUsername("test".to_string()),
            operations: ops,
        };
        crate::operations::local_ops::append_op_group(
            &lib.inner
                .local_dirs
                .local_state_operations()
                .operations_log_path(),
            &lib.inner.master_key,
            &op_group,
        )
        .unwrap();
        lib.load_local_state().await.unwrap();
        op_id
    }

    use crate::operations::{AlbumName, MediaFilename};
    let album_id = lib_a
        .album_create(AlbumName("Album".into()), None)
        .await
        .unwrap();
    write_op(
        &lib_a,
        vec![Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id,
            album_id_parent: album_id,
        }],
    )
    .await;

    write_op(
        &lib_a,
        vec![Operation::MediaCreation {
            timestamp: Utc::now(),
            media_id,
            filename_original: MediaFilename("photo.jpg".into()),
            date,
            storage_date: crate::operations::StorageDate {
                year: 2024,
                month: 1,
            },
            size_bytes: 1024,
            content_hash: crate::library::media::MediaHash::zeroed(),
            modified_at: None,
            gps: None,
            apple_aae_media_id: None,
            apple_live_photo_media_id: None,
        }],
    )
    .await;

    lib_a.album_add_media(album_id, media_id).await.unwrap();
    write_op(
        &lib_a,
        vec![Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id,
            media_id,
        }],
    )
    .await;

    lib_a.push(&storage, REMOTE_ID).await.unwrap();
    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    let reachable_a: HashSet<_> = lib_a
        .media_list(MediaListScope::Reachable)
        .iter()
        .map(|f| f.media_id)
        .collect();
    let reachable_b: HashSet<_> = lib_b
        .media_list(MediaListScope::Reachable)
        .iter()
        .map(|f| f.media_id)
        .collect();
    assert!(
        reachable_a.contains(&media_id),
        "media must be reachable on A"
    );
    assert!(
        reachable_b.contains(&media_id),
        "media must be reachable on B"
    );

    lib_a.album_remove_media(album_id, media_id).await.unwrap();
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    write_op(
        &lib_b,
        vec![Operation::GroupMediaRemove {
            timestamp: Utc::now(),
            group_id,
            media_id,
        }],
    )
    .await;
    lib_b.push(&storage, REMOTE_ID).await.unwrap();

    lib_a.fetch(&storage, REMOTE_ID).await.unwrap();
    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    let reachable_a: HashSet<_> = lib_a
        .media_list(MediaListScope::Reachable)
        .iter()
        .map(|f| f.media_id)
        .collect();
    let reachable_b: HashSet<_> = lib_b
        .media_list(MediaListScope::Reachable)
        .iter()
        .map(|f| f.media_id)
        .collect();
    assert!(
        !reachable_a.contains(&media_id),
        "media must be unreachable on A after both removals"
    );
    assert!(
        !reachable_b.contains(&media_id),
        "media must be unreachable on B after both removals"
    );
}

#[tokio::test]
// Offline add followed by a later sync. media_add succeeds offline. After the remote is fixed, push succeeds.
async fn offline_add_later_sync_push_succeeds() {
    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();

    let lib = make_library(&tmp).await;
    lib.initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp.path(), "offline.jpg", b"data");

    storage.set_offline(true);

    let mid = lib
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .expect("media_add must succeed offline")
        .id();

    let err = lib.push(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        matches!(
            err,
            crate::error::LibraryError::Sync(crate::error::SyncError::RemoteUnreachable(_))
        ),
        "push must fail with RemoteUnreachable"
    );

    storage.set_offline(false);

    let report = lib.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 1, "should upload 1 op");
    assert_eq!(report.media_uploaded, 1, "should upload 1 file");

    let tmp2 = TempDir::new().unwrap();
    let lib2 = make_library_with_same_keys(&tmp2, &lib).await;
    lib2.fetch(&storage, REMOTE_ID).await.unwrap();
    lib2.media_show(mid)
        .expect("second client must have the media");
}

#[tokio::test]
// Pushes media in chunks matching the iOS bulk importer (IosImportModel.importChunkSize = 32),
// closes and reopens the library, then fetches, pushes again, and reads every media item back.
async fn batched_push_survives_close_and_reopen() {
    use crate::library::media::upload::MediaAddSource;
    use crate::library::{Credentials, local_dirs::LocalDirs};
    use crate::operations::LibraryPassword;

    const CHUNK_SIZE: usize = 32;
    const NUM_CHUNKS: usize = 3;

    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();

    let lib = make_library(&tmp).await;
    lib.initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let library_id = lib.library_id();
    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());

    let mut media_ids = Vec::new();
    for chunk in 0..NUM_CHUNKS {
        for i in 0..CHUNK_SIZE {
            let name = format!("photo_{chunk}_{i}.jpg");
            let content = format!("data-{chunk}-{i}");
            let src = write_file(tmp.path(), &name, content.as_bytes());
            let mid = lib
                .media_add(
                    MediaAddSource::CopyFrom(src),
                    Some(album_id),
                    None,
                    None,
                    None,
                )
                .await
                .unwrap()
                .id();
            media_ids.push(mid);
        }
        lib.push(&storage, REMOTE_ID).await.unwrap();
    }
    assert_eq!(media_ids.len(), CHUNK_SIZE * NUM_CHUNKS);

    drop(lib);

    let local_dirs = LocalDirs::new(tmp.path(), &library_id);
    let reopened = Library::open(
        local_dirs,
        Credentials {
            username: LibraryUsername("alice".into()),
            password: LibraryPassword("secret".into()),
        },
    )
    .await
    .unwrap();
    reopened.load_local_state().await.unwrap();

    reopened.fetch(&storage, REMOTE_ID).await.unwrap();
    reopened.push(&storage, REMOTE_ID).await.unwrap();

    for mid in media_ids {
        reopened
            .media_get_bytes(mid, Some(&storage))
            .await
            .unwrap_or_else(|e| panic!("media_get_bytes failed for {mid:?}: {e:?}"));
    }
}
