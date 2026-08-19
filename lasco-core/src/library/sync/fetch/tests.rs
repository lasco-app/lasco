use tempfile::TempDir;
use uuid::Uuid;

use crate::identifiers::AlbumUuid;
use crate::storage::{AtomicWriteMode, Storage, StorageMockMemory};

use super::super::remote_access::StorageReadWrite;
use super::super::test_utils::{
    REMOTE_ID, make_library, make_library_with_same_keys, remote_uuid, write_file,
};

/// Writes `operations` to the remote as a single tier 1 compaction file, the same
/// encoding Push produces.
async fn write_compaction_file(
    storage: &StorageMockMemory,
    master_key: &crate::encryption::master_key::MasterKey,
    operations: Vec<crate::crdt::CrdtOperation>,
) {
    use crate::operations::CompactionFile;

    let file_uuid = crate::identifiers::CompactedOpId::new();
    let op_count = operations.len();
    let comp_file = CompactionFile {
        tier: 1,
        operations,
    };
    let bytes = crate::operations::encrypt_compaction_file(master_key, &file_uuid, &comp_file)
        .unwrap()
        .to_bytes();
    let key = format!("operations/{file_uuid}.op1_{op_count}");
    crate::operations::remote_ops::write_compaction_bytes(
        &StorageReadWrite::new(storage),
        &key,
        &bytes,
    )
    .await
    .unwrap();
}

#[tokio::test]
// fetch is idempotent. A second fetch downloads nothing new.
async fn fetch_idempotent() {
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
    let src = write_file(tmp_a.path(), "img.jpg", b"hello");
    lib_a
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src),
            Some(album_id),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    lib_a.push(&storage, REMOTE_ID).await.unwrap();

    let r1 = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    let r2 = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    // media_add into an album records two operations, MediaCreation then AlbumMediaAdd.
    assert_eq!(r1.ops_downloaded, 2);
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
    lib.initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let before = storage.list_call_count();

    lib.fetch(&storage, REMOTE_ID).await.unwrap();
    let after_first = storage.list_call_count();
    lib.fetch(&storage, REMOTE_ID).await.unwrap();
    let after_second = storage.list_call_count();

    assert_eq!(
        after_first - before,
        3,
        "first fetch: 3 list calls (remote identity, library/, operations/)"
    );
    assert_eq!(
        after_second - before,
        6,
        "second fetch: 3 more list calls regardless of prior state"
    );
}

#[tokio::test]
// After lib_a pushes a second file, lib_b's next fetch sees it (no stale last-known state).
async fn fetch_sees_externally_modified_remote() {
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

    let src1 = write_file(tmp_a.path(), "first.jpg", b"first");
    let mid1 = lib_a
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src1),
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
        .media_show(mid1)
        .expect("B must have mid1 after first fetch");

    let src2 = write_file(tmp_a.path(), "second.jpg", b"second");
    let mid2 = lib_a
        .media_add(
            crate::library::media::upload::MediaAddSource::CopyFrom(src2),
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
        .media_show(mid2)
        .expect("B must see mid2 on the next fetch without needing a restart");
}

#[tokio::test]
// A compaction file on the remote carries operations that B has never seen.
// B's fetch must decrypt the compaction file and absorb them.
async fn fetch_absorbs_op_from_compaction_file() {
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
    let src = write_file(tmp_a.path(), "packed.jpg", b"packed");
    let mid = lib_a
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

    let operations = lib_a.list_operations().unwrap();
    assert!(!operations.is_empty(), "media_add must record operations");
    let op_count = operations.len();
    write_compaction_file(&storage, &lib_a.inner.master_key, operations).await;

    let report = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        report.ops_downloaded, op_count,
        "B must absorb the ops from the compaction file"
    );
    lib_b
        .media_show(mid)
        .expect("B must have the media after absorbing compaction file");
}

#[tokio::test]
// An op present in two separate compaction files (e.g. after concurrent pushes) is not
// double-appended to the local log.
async fn fetch_does_not_double_append_op_in_two_compaction_files() {
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
    let src = write_file(tmp_a.path(), "dup.jpg", b"dup");
    let mid = lib_a
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

    let operations = lib_a.list_operations().unwrap();
    let op_count = operations.len();

    // Write the same operations into two different compaction files.
    for _ in 0u32..2 {
        write_compaction_file(&storage, &lib_a.inner.master_key, operations.clone()).await;
    }

    let report = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        report.ops_downloaded, op_count,
        "each op must be appended exactly once"
    );
    lib_b.media_show(mid).expect("B must have the media");

    let report2 = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        report2.ops_downloaded, 0,
        "second fetch must download nothing"
    );
}

#[tokio::test]
// Client B has never fetched. After A pushes 20 ops as a single .op1 compaction file,
// B fetches and gets all ops correctly.
async fn fetch_converges_after_compaction_push() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    // Twenty media, each added outside any album so each records exactly one operation.
    for i in 0..20usize {
        let src = write_file(tmp_a.path(), &format!("x{i}.jpg"), format!("data-{i}").as_bytes());
        lib_a
            .media_add(
                crate::library::media::upload::MediaAddSource::CopyFrom(src),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
    }

    let report = lib_a.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 20);
    assert_eq!(report.compactions_run, 0);

    let fetch_report = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(
        fetch_report.ops_downloaded, 20,
        "B must absorb all 20 ops from the .op1 file"
    );
}

#[tokio::test]
// After fetch absorbs a MediaCreation op whose file is on the remote,
// media_list.json is updated on the fetching client.
async fn fetch_updates_media_list_from_ops() {
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
    let src = write_file(tmp_a.path(), "photo.jpg", b"data");
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

    let media_list_path = lib_b
        .inner
        .local_dirs
        .remote_media_list(&REMOTE_ID.to_string())
        .media_list_path();
    let media_list =
        crate::remote::local_state::media_list_json::MediaList::load_or_default(&media_list_path)
            .unwrap();
    assert!(
        media_list.has_full(&media_id),
        "media_list.json must contain media_id after fetch"
    );
}

#[tokio::test]
// A blob uploaded to the remote after its creation op was already merged locally is still
// discovered, because fetch confirms every media the reconstructed state knows about and
// not only the ones created by the operations merged in this run.
async fn fetch_confirms_media_uploaded_after_its_op_was_merged() {
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
    let src = write_file(tmp_a.path(), "photo.jpg", b"data");
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

    let entry = lib_a.media_show(media_id).unwrap();
    let data_key = format!(
        "media/{}/{:02}/{}.data",
        entry.storage_date.year, entry.storage_date.month, media_id
    );
    let blob = storage.get(&data_key).await.unwrap();

    // The op file is on the remote but the blob is not there yet.
    storage.delete(&data_key).await.unwrap();
    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    let media_list_path = lib_b
        .inner
        .local_dirs
        .remote_media_list(&REMOTE_ID.to_string())
        .media_list_path();
    let media_list =
        crate::remote::local_state::media_list_json::MediaList::load_or_default(&media_list_path)
            .unwrap();
    assert!(
        !media_list.has_full(&media_id),
        "an absent blob must stay unconfirmed"
    );

    storage
        .put_atomic(&data_key, &blob, AtomicWriteMode::CreateIfAbsent)
        .await
        .unwrap();

    // This fetch merges no new operation file at all.
    let report = lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_downloaded, 0);

    let media_list =
        crate::remote::local_state::media_list_json::MediaList::load_or_default(&media_list_path)
            .unwrap();
    assert!(
        media_list.has_full(&media_id),
        "the late blob must be confirmed by a later fetch"
    );
}

#[tokio::test]
// A user added on another device (its mk file uploaded to the remote's library/ dir)
// becomes available on this one after fetch.
async fn fetch_downloads_new_user_mk_file() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
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
    let mk_bytes = std::fs::read(
        lib_a
            .inner
            .local_dirs
            .local_state_library_dir()
            .path()
            .join(&mk_name),
    )
    .unwrap();
    storage
        .put_atomic(
            &format!("library/{mk_name}"),
            &mk_bytes,
            AtomicWriteMode::Replace,
        )
        .await
        .unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();

    let downloaded = lib_b
        .inner
        .local_dirs
        .local_state_library_dir()
        .path()
        .join(&mk_name);
    assert!(
        downloaded.exists(),
        "B must have downloaded bob's mk file after fetch"
    );
    assert_eq!(std::fs::read(&downloaded).unwrap(), mk_bytes);
}

#[tokio::test]
// A second fetch does not re-download an mk file already present locally.
async fn fetch_does_not_redownload_existing_mk_file() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();
    let tmp_b = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let lib_b = make_library_with_same_keys(&tmp_b, &lib_a).await;

    let bob_uuid = lib_a
        .user_add(
            crate::operations::LibraryUsername("bob".into()),
            crate::operations::LibraryPassword("bob-pass".into()),
        )
        .await
        .unwrap();
    let mk_name = format!("mk_bob_{bob_uuid}.enc");
    let mk_bytes = std::fs::read(
        lib_a
            .inner
            .local_dirs
            .local_state_library_dir()
            .path()
            .join(&mk_name),
    )
    .unwrap();
    storage
        .put_atomic(
            &format!("library/{mk_name}"),
            &mk_bytes,
            AtomicWriteMode::Replace,
        )
        .await
        .unwrap();

    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    let before = storage.get_call_count();
    lib_b.fetch(&storage, REMOTE_ID).await.unwrap();
    let after = storage.get_call_count();

    assert_eq!(
        after, before,
        "second fetch must not re-download an mk file already present locally"
    );
}

#[tokio::test]
// If the remote's library_id_{uuid} does not match the local library, fetch must error
// instead of silently merging state from a different library.
async fn fetch_errors_on_library_id_mismatch() {
    let storage = StorageMockMemory::new();
    let tmp_a = TempDir::new().unwrap();

    let lib_a = make_library(&tmp_a).await;
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();

    // Corrupt the remote's library_id_{uuid} marker to simulate a different library.
    let wrong_marker = format!("library/library_id_{}", Uuid::new_v4());
    storage
        .delete(&format!("library/library_id_{}", lib_a.library_id().0))
        .await
        .unwrap();
    storage
        .put_atomic(&wrong_marker, b"", AtomicWriteMode::Replace)
        .await
        .unwrap();

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
    lib_a
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();

    // Corrupt the remote's remote_id_{uuid} marker to simulate a different remote.
    let wrong_marker = format!("remote_id_{}", Uuid::new_v4());
    storage
        .delete(&format!("remote_id_{REMOTE_ID}"))
        .await
        .unwrap();
    storage
        .put_atomic(&wrong_marker, b"", AtomicWriteMode::Replace)
        .await
        .unwrap();

    let err = lib_a.fetch(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        matches!(
            err,
            crate::error::LibraryError::Sync(crate::error::SyncError::RemoteIdMismatch(_))
        ),
        "fetch must fail with RemoteIdMismatch, got: {err}"
    );
}
