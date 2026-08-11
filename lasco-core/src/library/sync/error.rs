use thiserror::Error;

use crate::identifiers::MediaUuid;
use crate::operations::error::OperationError;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("remote unreachable: {0}")]
    RemoteUnreachable(crate::storage::StorageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Operation(#[from] OperationError),
    #[error("sync already running on this client")]
    AlreadyRunning,
    #[error("remote history was rewritten: {0}")]
    RemoteHistoryRewritten(String),
    #[error("remote operation file is invalid: {0}")]
    RemoteOperationInvalid(String),
    #[error("local remote-state cache is unreadable: {0}")]
    LocalCacheCorrupt(String),
    #[error("remote library id does not match local library: {0}")]
    LibraryIdMismatch(String),
    #[error("remote id does not match configured remote: {0}")]
    RemoteIdMismatch(String),
    #[error("media missing from local cache: {0:?}")]
    MissingLocalMedia(Vec<MediaUuid>),
}
