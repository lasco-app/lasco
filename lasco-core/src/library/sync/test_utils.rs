use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use crate::crdt::DeviceId;
use crate::identifiers::{LibraryId, RemoteUuid};
use crate::library::local_dirs::LocalDirs;
use crate::library::{Credentials, Library};

pub const REMOTE_ID: RemoteUuid =
    RemoteUuid(Uuid::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111));

pub fn remote_uuid() -> RemoteUuid {
    REMOTE_ID
}

pub async fn make_library(tmp: &TempDir) -> Library {
    let library_id = LibraryId(Uuid::new_v4());
    let local_dirs = LocalDirs::new(tmp.path(), &library_id);
    local_dirs.ensure_state_dirs().unwrap();
    Library::init(
        local_dirs,
        library_id,
        DeviceId(1),
        Credentials {
            username: "alice".into(),
            password: "secret".into(),
        },
    )
    .unwrap()
    .0
}

pub fn write_file(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

/// Creates a second Library instance sharing the same master key as `source`.
pub async fn make_library_with_same_keys(tmp: &TempDir, source: &Library) -> Library {
    let library_id = source.library_id();
    let local_dirs = LocalDirs::new(tmp.path(), &library_id);
    local_dirs.ensure_state_dirs().unwrap();
    // A second device receives its library directory from a remote, sentinel included.
    std::fs::write(
        local_dirs
            .local_state_library_dir()
            .path()
            .join(crate::library::library_format_sentinel()),
        b"",
    )
    .unwrap();
    Library::open_with_master_key(
        local_dirs,
        source.master_key().clone(),
        library_id,
        DeviceId(2),
        source.username().clone(),
    )
    .unwrap()
}

/// Same as `make_library_with_same_keys` but with an explicit device id. Two
/// devices sharing an id would mint colliding dots, so any test with three or
/// more clients must give each one its own.
pub async fn make_library_with_same_keys_as_device(
    tmp: &TempDir,
    source: &Library,
    device_id: DeviceId,
) -> Library {
    let library_id = source.library_id();
    let local_dirs = LocalDirs::new(tmp.path(), &library_id);
    local_dirs.ensure_state_dirs().unwrap();
    std::fs::write(
        local_dirs
            .local_state_library_dir()
            .path()
            .join(crate::library::library_format_sentinel()),
        b"",
    )
    .unwrap();
    Library::open_with_master_key(
        local_dirs,
        source.master_key().clone(),
        library_id,
        device_id,
        source.username().clone(),
    )
    .unwrap()
}

/// Marks `storage` as a valid remote for `remote_id` without going through a
/// library, writing the identity marker and the library format sentinel that
/// Push and Fetch both verify.
pub async fn stamp_remote(storage: &dyn crate::storage::Storage, remote_id: RemoteUuid) {
    use crate::storage::AtomicWriteMode;
    storage
        .put_atomic(
            &format!("remote_id_{remote_id}"),
            b"",
            AtomicWriteMode::CreateIfAbsent,
        )
        .await
        .unwrap();
    storage
        .put_atomic(
            &format!("library/{}", crate::library::library_format_sentinel()),
            b"",
            AtomicWriteMode::CreateIfAbsent,
        )
        .await
        .unwrap();
}

pub async fn stamp_remote_id(storage: &dyn crate::storage::Storage) {
    stamp_remote(storage, REMOTE_ID).await;
}
