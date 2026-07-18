use thiserror::Error;

use crate::encryption::error::{BlobError, CryptoError};

#[derive(Error, Debug)]
pub enum OperationError {
    #[error("CBOR serialization failed: {0}")]
    Serialize(String),
    #[error("CBOR deserialization failed: {0}")]
    Deserialize(String),
    #[error(transparent)]
    Blob(#[from] BlobError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
