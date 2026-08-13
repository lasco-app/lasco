use std::fmt;
use std::path::{Path, PathBuf};

use crate::identifiers::{LibraryId, MediaUuid};

/// Builds the local paths used by a library and creates restricted path objects
/// for each independently owned part of the layout.
#[derive(Clone)]
pub struct LocalDirs {
    root: PathBuf,
    library_id: LibraryId,
}

impl fmt::Debug for LocalDirs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalDirs")
            .field("root", &self.root)
            .field("library_id", &self.library_id)
            .finish()
    }
}

/// `local_state/library/`: library metadata and encrypted master-key files.
#[derive(Clone, Debug)]
pub struct LocalStateLibraryDir {
    path: PathBuf,
}

impl LocalStateLibraryDir {
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// `local_state/operations.log`: append-only encrypted individual CRDT operations.
#[derive(Clone, Debug)]
pub struct LocalStateOperations {
    local_state_dir: PathBuf,
}

/// `local_state/crdt-state.enc`: encrypted materialized `CrdtState`.
#[derive(Clone, Debug)]
pub struct LocalStateCrdt {
    local_state_dir: PathBuf,
}

impl LocalStateCrdt {
    #[must_use]
    pub fn snapshot_path(&self) -> PathBuf {
        self.local_state_dir.join("crdt-state.enc")
    }
}

impl LocalStateOperations {
    #[must_use]
    pub fn operations_log_path(&self) -> PathBuf {
        self.local_state_dir.join("operations.log")
    }
}

/// `local_state/media/`: locally available media data and thumbnails.
#[derive(Clone, Debug)]
pub struct LocalStateMediaDir {
    path: PathBuf,
}

impl LocalStateMediaDir {
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    #[must_use]
    pub fn data_path(&self, year: u16, month: u8, file_id: &MediaUuid) -> PathBuf {
        self.path
            .join(format!("{year}"))
            .join(format!("{month:02}"))
            .join(format!("{}.data", file_id.0))
    }

    #[must_use]
    pub fn thumb_path(&self, year: u16, month: u8, file_id: &MediaUuid) -> PathBuf {
        self.path
            .join(format!("{year}"))
            .join(format!("{month:02}"))
            .join(format!("{}.thumb", file_id.0))
    }
}

/// `remotes/{remote_id}/state/operations/`: this client's last-known operation state for one remote.
///
/// This deliberately excludes `compact_op_id_merged_to_local.json` and `media/media_list.json`, despite
/// their shared `state/` parent directory.
#[derive(Clone, Debug)]
pub struct RemoteLastKnownStateDir {
    path: PathBuf,
}

impl RemoteLastKnownStateDir {
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    #[must_use]
    pub fn operations_dir(&self) -> PathBuf {
        self.path.clone()
    }
}

/// `remotes/{remote_id}/state/media/media_list.json`: positive media inventory for one remote.
#[derive(Clone, Debug)]
pub struct RemoteMediaList {
    path: PathBuf,
}

impl RemoteMediaList {
    #[must_use]
    pub fn media_list_path(&self) -> PathBuf {
        self.path.clone()
    }
}

/// `remotes/{remote_id}/state/compact_op_id_merged_to_local.json`: fetch merge progress for one remote.
#[derive(Clone, Debug)]
pub struct RemoteCompactOpIdMergedToLocal {
    path: PathBuf,
}

impl RemoteCompactOpIdMergedToLocal {
    /// Records immutable remote operation files already merged into `operations.log`.
    #[must_use]
    pub fn compact_op_id_merged_to_local_path(&self) -> PathBuf {
        self.path.clone()
    }
}

impl LocalDirs {
    #[must_use]
    pub fn new(path_base: &Path, library_id: &LibraryId) -> LocalDirs {
        LocalDirs {
            root: path_base.join("libraries").join(library_id.0.to_string()),
            library_id: *library_id,
        }
    }

    #[must_use]
    pub fn library_id(&self) -> LibraryId {
        self.library_id
    }

    #[must_use]
    pub fn library_json_path(&self) -> PathBuf {
        self.root.join("library.json")
    }

    #[must_use]
    pub fn local_state_library_dir(&self) -> LocalStateLibraryDir {
        LocalStateLibraryDir {
            path: self.root.join("local_state").join("library"),
        }
    }

    #[must_use]
    pub fn local_state_operations(&self) -> LocalStateOperations {
        LocalStateOperations {
            local_state_dir: self.root.join("local_state"),
        }
    }

    #[must_use]
    pub fn local_state_crdt(&self) -> LocalStateCrdt {
        LocalStateCrdt {
            local_state_dir: self.root.join("local_state"),
        }
    }

    #[must_use]
    pub fn local_state_media_dir(&self) -> LocalStateMediaDir {
        LocalStateMediaDir {
            path: self.root.join("local_state").join("media"),
        }
    }

    #[must_use]
    pub fn remote_last_known_state_dir(&self, remote_id: &str) -> RemoteLastKnownStateDir {
        RemoteLastKnownStateDir {
            path: self
                .root
                .join("remotes")
                .join(remote_id)
                .join("state")
                .join("operations"),
        }
    }

    #[must_use]
    pub fn remote_media_list(&self, remote_id: &str) -> RemoteMediaList {
        RemoteMediaList {
            path: self
                .root
                .join("remotes")
                .join(remote_id)
                .join("state")
                .join("media")
                .join("media_list.json"),
        }
    }

    #[must_use]
    pub fn remote_compact_op_id_merged_to_local(
        &self,
        remote_id: &str,
    ) -> RemoteCompactOpIdMergedToLocal {
        RemoteCompactOpIdMergedToLocal {
            path: self
                .root
                .join("remotes")
                .join(remote_id)
                .join("state")
                .join("compact_op_id_merged_to_local.json"),
        }
    }

    pub(crate) fn ensure_state_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.local_state_library_dir().path())?;
        std::fs::create_dir_all(self.local_state_media_dir().path())?;
        Ok(())
    }

    pub(crate) fn ensure_sync_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.root.join("remotes"))
    }
}
