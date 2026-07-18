use lasco_core::error::{LibraryError, SyncError};

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum LascoError {
    #[error("wrong password or username")]
    InvalidCredentials,
    #[error("library not found")]
    NotFound,
    #[error("sync already in progress")]
    SyncBusy,
    #[error("storage error: {msg}")]
    Storage { msg: String },
    #[error("{msg}")]
    Other { msg: String },
}

impl From<LibraryError> for LascoError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::MediaNotFound(_) | LibraryError::AlbumNotFound(_) => {
                LascoError::NotFound
            }
            LibraryError::Storage(_) => LascoError::Storage { msg: e.to_string() },
            LibraryError::Sync(SyncError::AlreadyRunning) => LascoError::SyncBusy,
            _ => LascoError::Other { msg: e.to_string() },
        }
    }
}

impl From<anyhow::Error> for LascoError {
    fn from(e: anyhow::Error) -> Self {
        LascoError::Other { msg: e.to_string() }
    }
}
