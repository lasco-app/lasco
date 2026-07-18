use tempfile::TempDir;
use uuid::Uuid;

use crate::identifiers::AlbumUuid;
use crate::storage::{Storage, StorageMockMemory};

use super::super::test_utils::{make_library, make_library_with_same_keys, remote_uuid, write_file, REMOTE_ID};

#[tokio::test]
// fetch is idempotent. A second fetch downloads nothing new.
async fn fetch_idempotent() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp_a.path(), "img.jpg", b"hello");
    lib_a
        .media_add(crate::library::media::upload::MediaAddSource::CopyFrom(src), Some(album_id), None, None, None)
        .await
        .unwrap();
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    let r1 = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    let r2 = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    assert_eq!(r1.ops_downloaded, 1);
    assert_eq!(r2.ops_downloaded, 0, "second fetch must download nothing");
}

#[tokio::test]
// fetch calls storage.list() exactly three times per invocation: once to verify remote
// identity, once for library/ (step 1), and once for operations/ (step 2). Media
// listing is handled separately.
async fn fetch_calls_list_on_every_invocation() {
    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();
    let lib = make_library(&tmp).await;
    lib.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let before = storage.list_call_count();

    lib.fetch(&storage, REMOTE_ID).await.unwrap();
    let after_first = storage.list_call_count();
    lib.fetch(&storage, REMOTE_ID).await.unwrap();
    let after_second = storage.list_call_count();

    assert_eq!(after_first - before, 3, "first fetch: 3 list calls (remote identity, library/, operations/)");
    assert_eq!(after_second - before, 6, "second fetch: 3 more list calls regardless of prior state");
}

#[tokio::test]
// After lib_a pushes a second file, lib_b's next fetch sees it (no stale remote cache).
async fn fetch_sees_externally_modified_remote() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());

    let src1 = write_file(tmp_a.path(), "first.jpg", b"first");
    let mid1 = lib_a
        .media_add(crate::library::media::upload::MediaAddSource::CopyFrom(src1), Some(album_id), None, None, None)
        .await
        .unwrap().id();
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    lib_b.media_show(mid1).expect("B must have mid1 after first fetch");

    let src2 = write_file(tmp_a.path(), "second.jpg", b"second");
    let mid2 = lib_a
        .media_add(crate::library::media::upload::MediaAddSource::CopyFrom(src2), Some(album_id), None, None, None)
        .await
        .unwrap().id();
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    lib_b.media_show(mid2).expect("B must see mid2 on the next fetch without needing a restart");
}

#[tokio::test]
// A compaction file on the remote carries an op that B has never seen.
// B's fetch must decrypt the compaction file and absorb the op.
async fn fetch_absorbs_op_from_compaction_file() {
    use crate::operations::{CompactionEntry, CompactionFile};
    use crate::operations::remote_ops::write_compaction_file;

    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp_a.path(), "packed.jpg", b"packed");
    let mid = lib_a
        .media_add(crate::library::media::upload::MediaAddSource::CopyFrom(src), Some(album_id), None, None, None)
        .await
        .unwrap().id();

    let group = crate::operations::local_ops::read_pending_op_group(
        &lib_a.inner.local_dirs.pending_op_path(),
        &lib_a.inner.master_key,
    )
    .unwrap()
    .expect("pending group must exist after media_add");

    let comp_uuid = crate::identifiers::CompactedOpId::new();
    let comp_file = CompactionFile {
        tier: 1,
        contents: vec![CompactionEntry { op_id: group.op_id, group }],
    };
    let comp_key = format!("operations/{comp_uuid}.op1_{}", comp_file.contents.len());
    write_compaction_file(&storage, &lib_a.inner.master_key, &comp_key, &comp_uuid, &comp_file)
        .await
        .unwrap();

    let report = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_downloaded, 1, "B must absorb the op from the compaction file");
    lib_b.media_show(mid).expect("B must have the media after absorbing compaction file");
}

#[tokio::test]
// An op present in two separate compaction files (e.g. after concurrent pushes) is not
// double-appended to the local log.
async fn fetch_does_not_double_append_op_in_two_compaction_files() {
    use crate::operations::{CompactionEntry, CompactionFile};
    use crate::operations::remote_ops::write_compaction_file;

    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp_a.path(), "dup.jpg", b"dup");
    let mid = lib_a
        .media_add(crate::library::media::upload::MediaAddSource::CopyFrom(src), Some(album_id), None, None, None)
        .await
        .unwrap()
        .id();

    let group = crate::operations::local_ops::read_pending_op_group(
        &lib_a.inner.local_dirs.pending_op_path(),
        &lib_a.inner.master_key,
    )
    .unwrap()
    .expect("pending group must exist after media_add");

    // Write the same op group into two different compaction files.
    for i in 0u32..2 {
        let comp_uuid = crate::identifiers::CompactedOpId::new();
        let comp_file = CompactionFile {
            tier: 1,
            contents: vec![CompactionEntry { op_id: group.op_id, group: group.clone() }],
        };
        let comp_key = format!("operations/{comp_uuid}.op1_{i}");
        write_compaction_file(
            &storage,
            &lib_a.inner.master_key,
            &comp_key,
            &comp_uuid,
            &comp_file,
        )
        .await
        .unwrap();
    }

    let report = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_downloaded, 1, "op must be appended exactly once");
    lib_b.media_show(mid).expect("B must have the media");

    let report2 = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report2.ops_downloaded, 0, "second fetch must download nothing");
}

#[tokio::test]
// Client B has never fetched. After A pushes 20 ops as a single .op1 compaction file,
// B fetches and gets all ops correctly.
async fn fetch_converges_after_compaction_push() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    // Inject 20 op groups directly into A's main log (bypassing pending).
    for _ in 0..20usize {
        let op_id = crate::identifiers::OpUuid::new();
        let media_id = crate::identifiers::MediaUuid::from_uuid(uuid::Uuid::new_v4());
        let group = crate::operations::OperationGroup {
            op_id,
            parent_op_id: None,
            author: lib_a.inner.username.clone(),
            operations: vec![crate::operations::Operation::MediaCreation {
                timestamp: chrono::Utc::now(),
                media_id,
                filename_original: crate::operations::MediaFilename("x.jpg".into()),
                date: chrono::Utc::now(),
                storage_date: crate::operations::StorageDate { year: 2024, month: 1 },
                size_bytes: 100,
                content_hash: crate::library::media::MediaHash::zeroed(),
                modified_at: None,
                gps: None,
                apple_aae_media_id: None,
                apple_live_photo_media_id: None,
            }],
        };
        crate::operations::local_ops::append_op_group(
            &lib_a.inner.local_dirs.operations_log_path(),
            &lib_a.inner.master_key,
            &group,
        ).unwrap();
    }

    let report = lib_a.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 20);
    assert_eq!(report.compactions_run, 0);

    let fetch_report = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(fetch_report.ops_downloaded, 20, "B must absorb all 20 ops from the .op1 file");
}

#[tokio::test]
// After fetch absorbs a MediaCreation op whose file is on the remote,
// media_list.json is updated on the fetching client.
async fn fetch_updates_media_list_from_ops() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
    let src = write_file(tmp_a.path(), "photo.jpg", b"data");
    let media_id = lib_a
        .media_add(crate::library::media::upload::MediaAddSource::CopyFrom(src), Some(album_id), None, None, None)
        .await
        .unwrap().id();
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    let media_list_path = lib_b.inner.local_dirs.remote_media_list_path(REMOTE_ID);
    let media_list = crate::remote::local_state::media_list_json::MediaList::load_or_default(&media_list_path)
        .unwrap();
    assert!(media_list.contains(&media_id), "media_list.json must contain media_id after fetch");
}

#[tokio::test]
// A user added on another device (its mk file uploaded to the remote's library/ dir)
// becomes available on this one after fetch.
async fn fetch_downloads_new_user_mk_file() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    // Simulate a new user being added and propagated to the remote from another device.
    let bob_uuid = lib_a
        .user_add(
            crate::operations::LibraryUsername("bob".into()),
            crate::operations::LibraryPassword("bob-pass".into()),
        )
        .await
        .unwrap();
    let mk_name = format!("mk_bob_{bob_uuid}.enc");
    let mk_bytes = std::fs::read(lib_a.inner.local_dirs.local_library_dir().join(&mk_name)).unwrap();
    storage.put(&format!("library/{mk_name}"), &mk_bytes).await.unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    let downloaded = lib_b.inner.local_dirs.local_library_dir().join(&mk_name);
    assert!(downloaded.exists(), "B must have downloaded bob's mk file after fetch");
    assert_eq!(std::fs::read(&downloaded).unwrap(), mk_bytes);
}

#[tokio::test]
// A second fetch does not re-download an mk file already present locally.
async fn fetch_does_not_redownload_existing_mk_file() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let bob_uuid = lib_a
        .user_add(
            crate::operations::LibraryUsername("bob".into()),
            crate::operations::LibraryPassword("bob-pass".into()),
        )
        .await
        .unwrap();
    let mk_name = format!("mk_bob_{bob_uuid}.enc");
    let mk_bytes = std::fs::read(lib_a.inner.local_dirs.local_library_dir().join(&mk_name)).unwrap();
    storage.put(&format!("library/{mk_name}"), &mk_bytes).await.unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    let before = storage.get_call_count();
    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    let after = storage.get_call_count();

    assert_eq!(after, before, "second fetch must not re-download an mk file already present locally");
}

#[tokio::test]
// If the remote's library_id_{uuid} does not match the local library, fetch must error
// instead of silently merging state from a different library.
async fn fetch_errors_on_library_id_mismatch() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();

    // Corrupt the remote's library_id_{uuid} marker to simulate a different library.
    let wrong_marker = format!("library/library_id_{}", Uuid::new_v4());
    storage.delete(&format!("library/library_id_{}", lib_a.library_id().0)).await.unwrap();
    storage.put(&wrong_marker, b"").await.unwrap();

    let err = lib_a.fetch(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        matches!(
            err,
            crate::error::LibraryError::Sync(crate::error::SyncError::LibraryIdMismatch(_))
        ),
        "fetch must fail with LibraryIdMismatch, got: {err}"
    );
}

#[tokio::test]
// If the remote's remote_id_{uuid} does not match the configured remote, fetch must
// error instead of syncing against a remote that has been swapped out underneath it.
async fn fetch_errors_on_remote_id_mismatch() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a.initialize_remote(&storage, remote_uuid()).await.unwrap();

    // Corrupt the remote's remote_id_{uuid} marker to simulate a different remote.
    let wrong_marker = format!("remote_id_{}", Uuid::new_v4());
    storage.delete(&format!("remote_id_{}", REMOTE_ID)).await.unwrap();
    storage.put(&wrong_marker, b"").await.unwrap();

    let err = lib_a.fetch(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        matches!(
            err,
            crate::error::LibraryError::Sync(crate::error::SyncError::RemoteIdMismatch(_))
        ),
        "fetch must fail with RemoteIdMismatch, got: {err}"
    );
}
