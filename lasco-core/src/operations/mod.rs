pub mod error;
pub mod local_ops;
pub mod remote_ops;

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::crdt::CrdtOperation;
use crate::encryption::blob::{BlobEncrypted, encrypt_blob};
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::error::OperationError;
use crate::identifiers::CompactedOpId;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AlbumName(pub String);

impl From<String> for AlbumName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AlbumName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for AlbumName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaName(pub String);

impl From<String> for MediaName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MediaName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for MediaName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaFilename(pub String);

impl From<String> for MediaFilename {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MediaFilename {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for MediaFilename {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LibraryUsername(pub String);

impl From<String> for LibraryUsername {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for LibraryUsername {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for LibraryUsername {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[derive(zeroize::ZeroizeOnDrop)]
pub struct LibraryPassword(pub String);

impl From<String> for LibraryPassword {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for LibraryPassword {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for LibraryPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GpsCoords {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDate {
    pub year: u16,
    pub month: u8,
}

/// An encrypted compaction file containing individual CRDT operations at a given tier.
/// Stored as `operations/{uuid}.opN` on the remote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionFile {
    pub tier: u8,
    pub operations: Vec<CrdtOperation>,
}

pub(crate) fn compaction_file_to_cbor(file: &CompactionFile) -> Result<Vec<u8>, OperationError> {
    let mut cbor = Vec::new();
    ciborium::ser::into_writer(file, &mut cbor)
        .map_err(|e| OperationError::Serialize(e.to_string()))?;
    Ok(cbor)
}

pub(crate) fn compaction_file_from_cbor(bytes: &[u8]) -> Result<CompactionFile, OperationError> {
    ciborium::de::from_reader(bytes).map_err(|e| OperationError::Deserialize(e.to_string()))
}

/// Encodes and encrypts a compaction file once, so the same ciphertext can be written to both
/// remote storage and the local cache instead of encrypting twice with two different nonces.
pub(crate) fn encrypt_compaction_file(
    master_key: &MasterKey,
    file_uuid: &CompactedOpId,
    file: &CompactionFile,
) -> Result<BlobEncrypted, OperationError> {
    let cbor = compaction_file_to_cbor(file)?;
    let file_key = derive_blob_key(master_key, &file_uuid.0);
    Ok(encrypt_blob(&file_key, &cbor))
}
