use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlobError {
    #[error("unknown blob format version {0}")]
    UnknownVersion(u8),
    #[error("blob truncated: expected at least {expected} bytes, got {got}")]
    Truncated { expected: usize, got: usize },
}

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("AES-GCM authentication failed")]
    AuthenticationFailed,
}

#[derive(Error, Debug)]
pub enum KeychainError {
    #[error("CBOR serialization failed: {0}")]
    Serialize(String),
    #[error("CBOR deserialization failed: {0}")]
    Deserialize(String),
    #[error("AES-GCM authentication failed")]
    AuthenticationFailed,
    #[error("encrypted blob too short")]
    TooShort,
    #[error("invalid data length: expected {expected} bytes, got {got}")]
    InvalidLength { expected: usize, got: usize },
    #[error("{0}")]
    NotFound(String),
    #[error("unsupported protocol version {0}")]
    UnsupportedProtocolVersion(u32),
    #[error("io error: {0}")]
    Io(String),
}
