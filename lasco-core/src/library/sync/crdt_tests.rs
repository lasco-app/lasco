use tempfile::TempDir;

use crate::identifiers::{AlbumUuid, RemoteUuid};
use crate::library::media::upload::MediaAddSource;
use crate::storage::StorageMockMemory;

use super::test_utils::{
    REMOTE_ID, make_library, make_library_with_same_keys, remote_uuid, write_file,
};

#[tokio::test]
async fn push_and_fetch_individual_crdt_operations() {
    let storage = StorageMockMemory::new();
    let source_dir = TempDir::new().unwrap();
    let replica_dir = TempDir::new().unwrap();
    let source = make_library(&source_dir).await;
    source
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let replica = make_library_with_same_keys(&replica_dir, &source).await;

    let original = write_file(source_dir.path(), "photo.jpg", b"image");
    let media_id = source
        .media_add(
            MediaAddSource::CopyFrom(original),
            Some(AlbumUuid::from_uuid(uuid::Uuid::new_v4())),
            None,
            None,
            None,
        )
        .await
        .unwrap()
        .id();
    assert_eq!(source.list_operations().unwrap().len(), 2);
    let report = source.push(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(report.ops_uploaded, 2);

    let fetched = replica.fetch(&storage, REMOTE_ID).await.unwrap();
    assert_eq!(fetched.ops_downloaded, 2);
    assert!(replica.media_show(media_id).is_ok());
}

#[tokio::test]
async fn repeated_fetch_is_idempotent_by_dot() {
    let storage = StorageMockMemory::new();
    let source_dir = TempDir::new().unwrap();
    let replica_dir = TempDir::new().unwrap();
    let source = make_library(&source_dir).await;
    source
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let replica = make_library_with_same_keys(&replica_dir, &source).await;
    let album_id = source.album_create("album".into(), None).await.unwrap();
    source.push(&storage, REMOTE_ID).await.unwrap();

    assert_eq!(
        replica
            .fetch(&storage, REMOTE_ID)
            .await
            .unwrap()
            .ops_downloaded,
        1
    );
    assert_eq!(
        replica
            .fetch(&storage, REMOTE_ID)
            .await
            .unwrap()
            .ops_downloaded,
        0
    );
    assert!(replica.album_node_by_id(album_id).is_some());
    assert_eq!(replica.list_operations().unwrap().len(), 1);
}

#[tokio::test]
async fn push_relays_operations_learned_by_fetch() {
    let source_storage = StorageMockMemory::new();
    let target_storage = StorageMockMemory::new();
    let source_dir = TempDir::new().unwrap();
    let replica_dir = TempDir::new().unwrap();
    let source = make_library(&source_dir).await;
    source
        .initialize_remote(&source_storage, remote_uuid())
        .await
        .unwrap();
    let replica = make_library_with_same_keys(&replica_dir, &source).await;

    source.album_create("shared".into(), None).await.unwrap();
    source.push(&source_storage, REMOTE_ID).await.unwrap();
    assert_eq!(replica.fetch(&source_storage, REMOTE_ID).await.unwrap().ops_downloaded, 1);

    let target_remote_id = "33333333-3333-3333-3333-333333333333";
    replica
        .initialize_remote(
            &target_storage,
            RemoteUuid::from_uuid(target_remote_id.parse().unwrap()),
        )
        .await
        .unwrap();
    let report = replica.push(&target_storage, target_remote_id).await.unwrap();

    assert_eq!(report.ops_uploaded, 1);
}
