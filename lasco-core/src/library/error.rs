use thiserror::Error;

use crate::encryption::error::KeychainError;
use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::operations::error::OperationError;
use crate::operations::AlbumName;

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error(transparent)]
    Keychain(#[from] KeychainError),
    #[error("unsupported library format version: found {found}, expected {expected}")]
    UnsupportedFormatVersion { found: String, expected: String },
    #[error(transparent)]
    Operation(#[from] OperationError),
    #[error(transparent)]
    CrdtPersistence(#[from] crate::crdt::PersistenceError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot remove the last user")]
    LastUser,
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),
    #[error("file {0} not found")]
    MediaNotFound(MediaUuid),
    #[error("album {0} not found")]
    AlbumNotFound(AlbumUuid),
    #[error("group {0} not found")]
    GroupNotFound(GroupUuid),
    #[error(transparent)]
    Sync(#[from] crate::library::sync::error::SyncError),
    #[error("album with name '{0}' not found")]
    AlbumNotFoundByName(AlbumName),
    #[error("album name '{0}' is ambiguous")]
    AlbumNameAmbiguous(AlbumName, Vec<(AlbumUuid, String)>),
    #[error("reparenting album would create a cycle")]
    AlbumReparentWouldCycle,
}
