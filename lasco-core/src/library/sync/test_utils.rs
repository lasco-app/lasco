use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use crate::identifiers::{LibraryId, RemoteUuid};
use crate::library::local_dirs::LocalDirs;
use crate::library::{Credentials, Library};

pub const REMOTE_ID: &str = "11111111-1111-1111-1111-111111111111";

pub fn remote_uuid() -> RemoteUuid {
    RemoteUuid::from_uuid(REMOTE_ID.parse().unwrap())
}

pub async fn make_library(tmp: &TempDir) -> Library {
    let library_id = LibraryId(Uuid::new_v4());
    let local_dirs = LocalDirs::new(tmp.path(), &library_id);
    local_dirs.ensure_state_dirs().unwrap();
    Library::init(
        local_dirs,
        library_id,
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
    Library::open_with_master_key(
        local_dirs,
        source.master_key().clone(),
        library_id,
        source.username().clone(),
    )
    .unwrap()
}
