use crate::library::FfiMediaId;
use lasco_core::error::{LibraryError, SyncError};

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum LascoError {
    #[error("wrong password or username")]
    InvalidCredentials,
    #[error("library not found")]
    NotFound,
    #[error("sync already in progress")]
    SyncBusy,
    #[error("media missing from local cache")]
    MissingLocalMedia { media_ids: Vec<FfiMediaId> },
    #[error("storage error: {msg}")]
    Storage { msg: String },
    #[error("{msg}")]
    Other { msg: String },
}

impl From<LibraryError> for LascoError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::MediaNotFound(_) | LibraryError::AlbumNotFound(_) => LascoError::NotFound,
            LibraryError::Storage(_) => LascoError::Storage { msg: e.to_string() },
            LibraryError::Sync(SyncError::AlreadyRunning) => LascoError::SyncBusy,
            LibraryError::Sync(SyncError::MissingLocalMedia(ids)) => {
                LascoError::MissingLocalMedia {
                    media_ids: ids.into_iter().map(FfiMediaId::from).collect(),
                }
            }
            _ => LascoError::Other { msg: e.to_string() },
        }
    }
}

impl From<anyhow::Error> for LascoError {
    fn from(e: anyhow::Error) -> Self {
        LascoError::Other { msg: e.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lasco_core::identifiers::MediaUuid;

    #[test]
    fn missing_local_media_keeps_typed_ids() {
        let id = MediaUuid::from_uuid(uuid::Uuid::nil());
        let error = LascoError::from(LibraryError::Sync(SyncError::MissingLocalMedia(vec![id])));

        assert!(matches!(
            error,
            LascoError::MissingLocalMedia { media_ids }
                if media_ids.len() == 1 && media_ids[0].value == uuid::Uuid::nil().to_string()
        ));
    }
}
