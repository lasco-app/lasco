use chrono::Utc;
use tempfile::TempDir;

use super::*;
use crate::identifiers::AlbumUuid;
use crate::operations::AlbumName;

fn make_local_dirs(tmp: &TempDir, library_id: &LibraryId) -> LocalDirs {
    LocalDirs::new(tmp.path(), library_id)
}

async fn make_library(tmp: &TempDir) -> (Library, LibraryId) {
    let library_id = LibraryId(Uuid::new_v4());
    let local_dirs = make_local_dirs(tmp, &library_id);
    local_dirs.ensure_state_dirs().unwrap();
    let (lib, _password_uuid) = Library::init(
        local_dirs,
        library_id,
        Credentials {
            username: LibraryUsername("alice".to_string()),
            password: LibraryPassword("pass".to_string()),
        },
    )
    .await
    .unwrap();
    (lib, library_id)
}

#[tokio::test]
// Verifies that a library opened after init reports the same identity and protocol version as the one returned by init.
async fn init_then_open_preserves_library_id_and_protocol_version() {
    let tmp = TempDir::new().unwrap();
    let (lib, library_id) = make_library(&tmp).await;

    let local_dirs = make_local_dirs(&tmp, &library_id);
    let lib2 = Library::open(
        local_dirs,
        Credentials {
            username: LibraryUsername("alice".to_string()),
            password: LibraryPassword("pass".to_string()),
        },
    )
    .await
    .unwrap();

    assert_eq!(lib.library_id(), lib2.library_id());
    assert_eq!(lib.protocol_version(), lib2.protocol_version());
    assert_eq!(lib.library_id(), library_id);
    assert_eq!(lib.protocol_version(), PROTOCOL_VERSION);
}

#[tokio::test]
// Ensures that supplying an incorrect password during open fails rather than silently decrypting garbage.
async fn open_with_wrong_password_returns_error() {
    let tmp = TempDir::new().unwrap();
    let (_lib, library_id) = make_library(&tmp).await;

    let local_dirs = make_local_dirs(&tmp, &library_id);
    let result = Library::open(
        local_dirs,
        Credentials {
            username: LibraryUsername("alice".to_string()),
            password: LibraryPassword("wrong".to_string()),
        },
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test]
// Checks that init writes the expected files on disk (library_salt, version_1, library_id, mk_{user}_{uuid}.enc).
async fn init_writes_expected_files_on_disk() {
    let tmp = TempDir::new().unwrap();
    let username = "alice";
    let library_id = LibraryId(Uuid::new_v4());
    let local_dirs = make_local_dirs(&tmp, &library_id);
    local_dirs.ensure_state_dirs().unwrap();

    Library::init(
        local_dirs.clone(),
        library_id,
        Credentials {
            username: LibraryUsername(username.to_string()),
            password: LibraryPassword("pass".to_string()),
        },
    )
    .await
    .unwrap()
    .0;

    let lib_dir = local_dirs.local_state_library_dir();
    assert!(
        lib_dir.path().join("library_salt").exists(),
        "library_salt must exist"
    );
    assert!(
        lib_dir.path().join("version_1").exists(),
        "version_1 sentinel must exist"
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

#[tokio::test]
// Concurrent append_to_pending calls must not lose updates to pending.op (regression test
// for the LocalOpsReadWriteLock that guards read-modify-write access to it).
async fn concurrent_append_to_pending_does_not_lose_operations() {
    let tmp = TempDir::new().unwrap();
    let (lib, _library_id) = make_library(&tmp).await;

    const THREAD_COUNT: usize = 16;
    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            let lib = lib.clone();
            std::thread::spawn(move || {
                lib.append_to_pending(Operation::AlbumCreation {
                    timestamp: Utc::now(),
                    album_id: AlbumUuid::from_uuid(Uuid::new_v4()),
                    name: AlbumName("test".to_string()),
                    album_id_parent: None,
                })
                .unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let groups = lib.list_operation_groups().unwrap();
    assert_eq!(
        groups.len(),
        1,
        "all appends should land in the single pending group"
    );
    assert_eq!(
        groups[0].operations.len(),
        THREAD_COUNT,
        "no operation should be lost to a racing read-modify-write on pending.op"
    );
}

#[tokio::test]
async fn local_edit_is_merged_and_queued_in_the_crdt_state_replica() {
    let tmp = TempDir::new().unwrap();
    let (lib, _library_id) = make_library(&tmp).await;
    let album_id = AlbumUuid::from_uuid(Uuid::new_v4());

    lib.append_to_pending(Operation::AlbumCreation {
        timestamp: Utc::now(),
        album_id,
        name: AlbumName("CRDT".to_string()),
        album_id_parent: None,
    })
    .unwrap();

    let replica = lib.inner.crdt_replica_state.read();
    assert_eq!(replica.outgoing.len(), 1);
    assert!(replica.state.is_album_created_and_live(album_id));
    assert_eq!(
        replica.outgoing[0].dot,
        replica.state.albums[&album_id]
            .creation
            .as_ref()
            .unwrap()
            .dot
    );
}
