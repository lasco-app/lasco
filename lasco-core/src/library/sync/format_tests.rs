use tempfile::TempDir;

use crate::error::{LibraryError, SyncError};
use crate::library::library_format_sentinel;
use crate::storage::{AtomicWriteMode, Storage, StorageMockMemory};

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
