use std::fmt;
use std::path::PathBuf;

use crate::identifiers::{LibraryId, MediaUuid};

/// Builds all the local paths used by a library.
#[derive(Clone)]
pub struct LocalDirs {
    root: PathBuf,
    library_id: LibraryId,
}

impl fmt::Debug for LocalDirs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalDirs")
            .field("root", &self.root)
            .finish()
    }
}

impl LocalDirs {
    pub fn new(path_base: PathBuf, library_id: &LibraryId) -> LocalDirs {
        LocalDirs {
            root: path_base.join("libraries").join(library_id.0.to_string()),
            library_id: *library_id,
        }
    }

    pub fn library_id(&self) -> LibraryId {
        self.library_id
    }

    pub fn library_json_path(&self) -> PathBuf {
        self.root.join("library.json")
    }

    pub fn local_state_dir(&self) -> PathBuf {
        self.root.join("local_state")
    }

    /// Directory for local crypto files including salt.bin, the write format version, and mk_*.enc.
    pub fn local_library_dir(&self) -> PathBuf {
        self.root.join("local_state").join("library")
    }

    pub fn operations_log_path(&self) -> PathBuf {
        self.root.join("local_state").join("operations.log")
    }

    pub fn pending_op_path(&self) -> PathBuf {
        self.root.join("local_state").join("pending.op")
    }

    pub fn media_dir(&self) -> PathBuf {
        self.root.join("local_state").join("media")
    }

    pub fn remotes_dir(&self) -> PathBuf {
        self.root.join("remotes")
    }

    pub fn remote_state_dir(&self, remote_id: &str) -> PathBuf {
        self.root.join("remotes").join(remote_id).join("state")
    }

    pub fn remote_ops_dir(&self, remote_id: &str) -> PathBuf {
        self.remote_state_dir(remote_id).join("operations")
    }

    pub fn remote_media_dir(&self, remote_id: &str) -> PathBuf {
        self.remote_state_dir(remote_id).join("media")
    }

    pub fn processed_files_path(&self, remote_id: &str) -> PathBuf {
        self.root
            .join("remotes")
            .join(remote_id)
            .join("processed.json")
    }

    pub fn remote_media_list_path(&self, remote_id: &str) -> PathBuf {
        self.remote_media_dir(remote_id).join("media_list.json")
    }

    pub fn media_data_path(&self, year: u16, month: u8, file_id: &MediaUuid) -> PathBuf {
        self.media_dir()
            .join(format!("{year}"))
            .join(format!("{month:02}"))
            .join(format!("{}.data", file_id.0))
    }

    pub fn media_thumb_path(&self, year: u16, month: u8, file_id: &MediaUuid) -> PathBuf {
        self.media_dir()
            .join(format!("{year}"))
            .join(format!("{month:02}"))
            .join(format!("{}.thumb", file_id.0))
    }

    pub fn ensure_state_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.local_state_dir())?;
        std::fs::create_dir_all(self.local_library_dir())?;
        std::fs::create_dir_all(self.media_dir())?;
        Ok(())
    }

    pub fn ensure_sync_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.remotes_dir())?;
        Ok(())
    }
}
