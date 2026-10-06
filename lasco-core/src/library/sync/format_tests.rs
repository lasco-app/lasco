use tempfile::TempDir;
use uuid::Uuid;

use crate::error::{LibraryError, SyncError};
use crate::identifiers::RemoteUuid;
use crate::library::library_format_sentinel;
use crate::storage::{
    AtomicWriteMode, Storage, StorageError, StorageLocalFs, StorageMockMemory,
    StorageMockMemoryFaulty, StorageMockOperation,
};

use super::remote_access::StorageRead;
use super::test_utils::{REMOTE_ID, make_library, remote_uuid};

fn sentinel_key() -> String {
    format!("library/{}", library_format_sentinel())
}

fn is_unsupported_format(error: &LibraryError) -> bool {
    matches!(
        error,
        LibraryError::Sync(SyncError::UnsupportedRemoteFormat { .. })
    )
}

#[tokio::test]
// A remote whose library format sentinel is absent must not be merged into local state.
async fn fetch_errors_when_the_remote_sentinel_is_missing() {
    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();
    let library = make_library(&tmp).await;
    library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    storage.delete(&sentinel_key()).await.unwrap();

    let error = library.fetch(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        is_unsupported_format(&error),
        "fetch must fail with UnsupportedRemoteFormat, got: {error}"
    );
}

#[tokio::test]
// Push must refuse the remote before it writes anything to it.
async fn push_errors_when_the_remote_sentinel_is_missing() {
    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();
    let library = make_library(&tmp).await;
    library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    storage.delete(&sentinel_key()).await.unwrap();
    let before = storage.list("library/").await.unwrap().len();

    let error = library.push(&storage, REMOTE_ID).await.unwrap_err();
    assert!(
        is_unsupported_format(&error),
        "push must fail with UnsupportedRemoteFormat, got: {error}"
    );
    assert_eq!(
        storage.list("library/").await.unwrap().len(),
        before,
        "push must reject the remote before uploading to it"
    );
}

#[tokio::test]
// A remote already holding a library directory with no sentinel is not an initialized
// remote this build can use, so it must be rejected rather than accepted as ready.
async fn initialize_remote_errors_when_the_remote_sentinel_is_missing() {
    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();
    let library = make_library(&tmp).await;
    storage
        .put_atomic("library/library_salt", b"salt", AtomicWriteMode::Replace)
        .await
        .unwrap();

    let error = library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap_err();
    assert!(
        is_unsupported_format(&error),
        "initialize_remote must fail with UnsupportedRemoteFormat, got: {error}"
    );
}

#[tokio::test]
// A fresh remote must never write its marker into a folder already claimed by
// another remote identity.
async fn initialize_remote_rejects_an_existing_remote_identity_before_writing() {
    let tmp = TempDir::new().unwrap();
    let storage = StorageLocalFs::new(tmp.path().join("remote"));
    let library = make_library(&tmp).await;
    let other_remote = RemoteUuid(Uuid::new_v4());
    storage
        .put_atomic(
            &format!("remote_id_{other_remote}"),
            b"",
            AtomicWriteMode::Replace,
        )
        .await
        .unwrap();
    let before = storage.list_recursive("").await.unwrap();

    let error = library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            LibraryError::Sync(SyncError::RemoteAlreadyInitialized(_))
        ),
        "initialize_remote must reject an existing remote identity, got: {error}"
    );
    assert!(
        !storage
            .exists(&format!("remote_id_{}", remote_uuid()))
            .await
            .unwrap()
    );
    assert_eq!(storage.list_recursive("").await.unwrap(), before);
    assert_eq!(
        storage
            .get(&format!("remote_id_{other_remote}"))
            .await
            .unwrap(),
        b""
    );
}

#[tokio::test]
async fn initialize_remote_creates_missing_local_destination_and_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("local_fs_test/new-remote");
    let storage = StorageLocalFs::new(&root);
    let library = make_library(&tmp).await;
    let remote = StorageRead::new(&storage);

    // Existing-remote and USB selection checks must still reject a missing folder.
    assert!(matches!(
        super::verify_remote_identity(&remote, remote_uuid()).await,
        Err(SyncError::RemoteUnreachable(StorageError::NotFound))
    ));
    assert!(matches!(
        super::ensure_remote_identity_absent(&remote).await,
        Err(SyncError::RemoteUnreachable(StorageError::NotFound))
    ));
    assert!(library.push(&storage, REMOTE_ID).await.is_err());
    assert!(library.fetch(&storage, REMOTE_ID).await.is_err());
    assert!(!root.exists());

    library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    assert!(root.is_dir());
    assert!(storage.exists(&sentinel_key()).await.unwrap());
    super::verify_remote_identity(&remote, remote_uuid())
        .await
        .unwrap();
    let mut before = storage.list_recursive("").await.unwrap();
    before.sort();

    library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();
    let mut after = storage.list_recursive("").await.unwrap();
    after.sort();
    assert_eq!(after, before);
    library.push(&storage, REMOTE_ID).await.unwrap();
    library.fetch(&storage, REMOTE_ID).await.unwrap();
}

#[tokio::test]
async fn initialize_remote_rejects_listing_failure_before_writing() {
    let tmp = TempDir::new().unwrap();
    let library = make_library(&tmp).await;
    let storage = StorageMockMemoryFaulty::new();
    storage.fail_next(StorageMockOperation::List, "");

    let error = library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LibraryError::Sync(SyncError::RemoteUnreachable(StorageError::Unavailable(_)))
    ));
    assert!(storage.list_recursive("").await.unwrap().is_empty());
}

#[tokio::test]
async fn initialize_remote_rejects_non_directory_destination_before_writing() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("remote");
    std::fs::write(&root, b"existing file").unwrap();
    let storage = StorageLocalFs::new(&root);
    let library = make_library(&tmp).await;

    let error = library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LibraryError::Sync(SyncError::RemoteUnreachable(StorageError::Other(_)))
    ));
    assert_eq!(std::fs::read(&root).unwrap(), b"existing file");
}

#[tokio::test]
// A freshly initialized remote carries the sentinel, so both directions succeed.
async fn initialized_remote_carries_the_sentinel() {
    let storage = StorageMockMemory::new();
    let tmp = TempDir::new().unwrap();
    let library = make_library(&tmp).await;
    library
        .initialize_remote(&storage, remote_uuid())
        .await
        .unwrap();

    assert!(storage.exists(&sentinel_key()).await.unwrap());
    library.push(&storage, REMOTE_ID).await.unwrap();
    library.fetch(&storage, REMOTE_ID).await.unwrap();
}
