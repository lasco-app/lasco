pub mod error;
pub mod local_ops;
pub mod remote_ops;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::encryption::blob::{encrypt_blob, BlobEncrypted};
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::identifiers::{AlbumUuid, CompactedOpId, GroupUuid, MediaUuid, OpUuid};
use crate::error::OperationError;
use crate::library::media::MediaHash;


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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    MediaCreation {
        timestamp: DateTime<Utc>,
        media_id: MediaUuid,
        filename_original: MediaFilename,
        date: DateTime<Utc>,
        storage_date: StorageDate,
        size_bytes: u64,
        content_hash: MediaHash,
        modified_at: Option<DateTime<Utc>>,
        gps: Option<GpsCoords>,
        apple_aae_media_id: Option<MediaUuid>,
        apple_live_photo_media_id: Option<MediaUuid>,
    },
    /// Sets or clears the user-facing display name for a media (independent of its original filename).
    MediaRename {
        timestamp: DateTime<Utc>,
        media_id: MediaUuid,
        name: Option<MediaName>,
    },
    /// `key` is a stable namespaced string (e.g. `exif.camera_make`, `embedding.clip_v1`).
    /// `value` is a JSON string. Rich schemas deferred.
    MediaPropsUpdate {
        timestamp: DateTime<Utc>,
        media_id: MediaUuid,
        key: String,
        value: String,
    },
    AlbumCreation {
        timestamp: DateTime<Utc>,
        album_id: AlbumUuid,
        name: AlbumName,
        album_id_parent: Option<AlbumUuid>,
    },
    AlbumMediaAdd {
        timestamp: DateTime<Utc>,
        album_id: AlbumUuid,
        media_id: MediaUuid,
    },
    AlbumMediaRemove {
        timestamp: DateTime<Utc>,
        album_id: AlbumUuid,
        media_id: MediaUuid,
    },
    AlbumDeletion {
        timestamp: DateTime<Utc>,
        album_id: AlbumUuid,
    },
    AlbumRename {
        timestamp: DateTime<Utc>,
        album_id: AlbumUuid,
        name: AlbumName,
    },
    AlbumReparent {
        timestamp: DateTime<Utc>,
        album_id: AlbumUuid,
        new_parent_id: Option<AlbumUuid>,
    },
    /// Sets or clears the explicit thumbnail media for an album. `None` clears it.
    AlbumThumbnailSet {
        timestamp: DateTime<Utc>,
        album_id: AlbumUuid,
        media_id: Option<MediaUuid>,
    },
    /// Groups are always leaves bound permanently to `album_id_parent` (no reparent op).
    GroupCreation {
        timestamp: DateTime<Utc>,
        group_id: GroupUuid,
        album_id_parent: AlbumUuid,
    },
    GroupMediaAdd {
        timestamp: DateTime<Utc>,
        group_id: GroupUuid,
        media_id: MediaUuid,
    },
    GroupMediaRemove {
        timestamp: DateTime<Utc>,
        group_id: GroupUuid,
        media_id: MediaUuid,
    },
    GroupDeletion {
        timestamp: DateTime<Utc>,
        group_id: GroupUuid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationGroup {
    pub op_id: OpUuid,
    /// Causal predecessor. Holds the op_id of the last group this client had applied when it wrote this one.
    /// `None` for the very first group in a library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_op_id: Option<OpUuid>,
    pub author: LibraryUsername,
    pub operations: Vec<Operation>,
}

/// One entry inside a compaction file holding the original op_id and the full group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEntry {
    pub op_id: OpUuid,
    pub group: OperationGroup,
}

/// An encrypted compaction file grouping multiple op groups at a given tier.
/// Stored as `operations/{uuid}.opN` on the remote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionFile {
    pub tier: u8,
    pub contents: Vec<CompactionEntry>,
}


pub(crate) fn op_group_to_cbor(group: &OperationGroup) -> Result<Vec<u8>, OperationError> {
    let mut cbor = Vec::new();
    ciborium::ser::into_writer(group, &mut cbor)
        .map_err(|e| OperationError::Serialize(e.to_string()))?;
    Ok(cbor)
}

pub(crate) fn op_group_from_cbor(bytes: &[u8]) -> Result<OperationGroup, OperationError> {
    ciborium::de::from_reader(bytes).map_err(|e| OperationError::Deserialize(e.to_string()))
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
