use chrono::Utc;
use tempfile::TempDir;

use super::*;
use crate::crdt::OperationContent;
use crate::identifiers::AlbumUuid;
use crate::operations::AlbumName;

const DEVICE_ID: crate::crdt::DeviceId = crate::crdt::DeviceId(1);

fn make_local_dirs(tmp: &TempDir, library_id: &LibraryId) -> LocalDirs {
    LocalDirs::new(tmp.path(), library_id)
}

fn credentials(password: &str) -> Credentials {
    Credentials {
        username: LibraryUsername("alice".to_string()),
        password: LibraryPassword(password.to_string()),
    }
}

fn make_library(tmp: &TempDir) -> (Library, LibraryId) {
    let library_id = LibraryId(Uuid::new_v4());
    let local_dirs = make_local_dirs(tmp, &library_id);
    local_dirs.ensure_state_dirs().unwrap();
    let (lib, _password_uuid) =
        Library::init(local_dirs, library_id, DEVICE_ID, credentials("pass")).unwrap();
    (lib, library_id)
}

fn album_creation(album_id: AlbumUuid, name: &str) -> OperationContent {
    OperationContent::AlbumCreation {
        album_id,
        name: AlbumName(name.to_string()),
        parent_id: None,
    }
}

#[test]
// Verifies that a library opened after init reports the same identity and protocol version as the one returned by init.
fn init_then_open_preserves_library_id_and_protocol_version() {
    let tmp = TempDir::new().unwrap();
    let (lib, library_id) = make_library(&tmp);

    let local_dirs = make_local_dirs(&tmp, &library_id);
    let lib2 = Library::open(local_dirs, DEVICE_ID, credentials("pass")).unwrap();

    assert_eq!(lib.library_id(), lib2.library_id());
    assert_eq!(lib.protocol_version(), lib2.protocol_version());
    assert_eq!(lib.library_id(), library_id);
    assert_eq!(lib.protocol_version(), PROTOCOL_VERSION);
}

#[test]
// Ensures that supplying an incorrect password during open fails rather than silently decrypting garbage.
fn open_with_wrong_password_returns_error() {
    let tmp = TempDir::new().unwrap();
    let (_lib, library_id) = make_library(&tmp);

    let local_dirs = make_local_dirs(&tmp, &library_id);
    let result = Library::open(local_dirs, DEVICE_ID, credentials("wrong"));

    assert!(result.is_err());
}

#[test]
// Checks that init writes the expected files on disk (library_salt, version sentinel, library_id, mk_{user}_{uuid}.enc).
fn init_writes_expected_files_on_disk() {
    let tmp = TempDir::new().unwrap();
    let username = "alice";
    let library_id = LibraryId(Uuid::new_v4());
    let local_dirs = make_local_dirs(&tmp, &library_id);
    local_dirs.ensure_state_dirs().unwrap();

    Library::init(
        local_dirs.clone(),
        library_id,
        DEVICE_ID,
        credentials("pass"),
    )
    .unwrap();

    let lib_dir = local_dirs.local_state_library_dir();
    assert!(
        lib_dir.path().join("library_salt").exists(),
        "library_salt must exist"
    );
    assert!(
        lib_dir.path().join(library_format_sentinel()).exists(),
        "format sentinel must exist"
    );
    assert!(
        lib_dir
            .path()
            .join(format!("library_id_{}", library_id.0))
            .exists(),
        "library_id_{{uuid}} must exist"
    );
    let has_mk = std::fs::read_dir(lib_dir.path())
        .unwrap()
        .flatten()
        .any(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            s.starts_with(&format!("mk_{username}_")) && s.ends_with(".enc")
        });
    assert!(has_mk, "mk file for {username} must exist");
    assert!(
        !lib_dir.path().join("mk_other.enc").exists(),
        "no mk file for other user"
    );
}

#[test]
// Concurrent record_local_operation calls must not lose operations in the log (regression test
// for the LocalOpsReadWriteLock that guards read-modify-write access to it).
fn concurrent_record_local_operation_does_not_lose_operations() {
    let tmp = TempDir::new().unwrap();
    let (lib, _library_id) = make_library(&tmp);

    const THREAD_COUNT: usize = 16;
    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            let lib = lib.clone();
            std::thread::spawn(move || {
                lib.record_local_operation(
                    Utc::now(),
                    album_creation(AlbumUuid::from_uuid(Uuid::new_v4()), "test"),
                )
                .unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let operations = lib.list_operations().unwrap();
    assert_eq!(
        operations.len(),
        THREAD_COUNT,
        "no operation should be lost to a racing read-modify-write on the operation log"
    );
}

#[test]
fn local_edit_is_merged_into_state() {
    let tmp = TempDir::new().unwrap();
    let (lib, _library_id) = make_library(&tmp);
    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());

    lib.record_local_operation(Utc::now(), album_creation(album_id, "CRDT"))
        .unwrap();

    let state = lib.inner.state.read();
    assert!(state.is_album_created_and_live(album_id));
    assert_eq!(
        state.album(album_id).unwrap().name,
        AlbumName("CRDT".to_string())
    );

    let operations = lib.list_operations().unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].dot.device_id, state.device_id());
}
