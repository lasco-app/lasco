use thiserror::Error;

use crate::encryption::error::{BlobError, CryptoError};

#[derive(Error, Debug)]
pub enum OperationError {
    #[error("CBOR serialization failed: {0}")]
    Serialize(String),
    #[error("CBOR deserialization failed: {0}")]
    Deserialize(String),
    #[error("local operation log frame has a zero-length blob")]
    ZeroLengthBlob,
    #[error("local operation log blob length {declared} exceeds the {maximum}-byte limit")]
    BlobTooLarge { declared: usize, maximum: usize },
    #[error("local operation log frame is incomplete: expected {expected} bytes, found {found}")]
    IncompleteFrame { expected: usize, found: usize },
    #[error("unsupported local operation log format version {0}")]
    UnsupportedLocalOperationLogVersion(u32),
    #[error("pending operation file contains trailing data")]
    PendingTrailingData,
    #[error(transparent)]
    Blob(#[from] BlobError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
