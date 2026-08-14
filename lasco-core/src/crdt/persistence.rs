use serde::{Deserialize, Serialize};

use super::{CrdtState, DeviceId};

const CRDT_SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Versioned on-disk representation of [`CrdtState`]. The version applies only to the
/// local snapshot encoding; it does not alter CRDT operation semantics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct VersionedCrdtState {
    #[serde(default)]
    format_version: u32,
    state: CrdtState,
}

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CRDT state serialization failed: {0}")]
    Serialize(String),
    #[error("CRDT state deserialization failed: {0}")]
    Deserialize(String),
    #[error("encrypted CRDT state is invalid: {0}")]
    Crypto(#[from] crate::encryption::error::CryptoError),
    #[error("encrypted CRDT state blob is invalid: {0}")]
    Blob(#[from] crate::encryption::error::BlobError),
    #[error("unsupported CRDT snapshot format version {0}")]
    UnsupportedFormatVersion(u32),
}

/// Reads a complete `CrdtState` snapshot.
pub(crate) fn load_persisted(
    path: &std::path::Path,
    master_key: &crate::encryption::master_key::MasterKey,
    device_id: DeviceId,
) -> Result<CrdtState, PersistenceError> {
    if !path.exists() {
        return Ok(CrdtState::new(device_id));
    }
    let encrypted = crate::encryption::blob::BlobEncrypted::from_bytes(&std::fs::read(path)?)?;
    let key = crate::encryption::blob_key::derive_blob_key(master_key, &CRDT_STATE_KEY_ID);
    let bytes = crate::encryption::blob::decrypt_blob(&key, &encrypted)?;
    let persisted: VersionedCrdtState = ciborium::de::from_reader(bytes.as_slice())
        .map_err(|error| PersistenceError::Deserialize(error.to_string()))?;
    match persisted.format_version {
        CRDT_SNAPSHOT_FORMAT_VERSION => Ok(persisted.state),
        version => Err(PersistenceError::UnsupportedFormatVersion(version)),
    }
}

/// Atomically encrypts and replaces the `CrdtState` snapshot.
pub(crate) fn save_persisted(
    path: &std::path::Path,
    master_key: &crate::encryption::master_key::MasterKey,
    state: &CrdtState,
) -> Result<(), PersistenceError> {
    let mut bytes = Vec::new();
    let on_disk = VersionedCrdtState {
        format_version: CRDT_SNAPSHOT_FORMAT_VERSION,
        state: state.clone(),
    };
    ciborium::ser::into_writer(&on_disk, &mut bytes)
        .map_err(|error| PersistenceError::Serialize(error.to_string()))?;
    let key = crate::encryption::blob_key::derive_blob_key(master_key, &CRDT_STATE_KEY_ID);
    let encrypted = crate::encryption::blob::encrypt_blob(&key, &bytes);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::atomic_file::write(path, &encrypted.to_bytes())?;
    Ok(())
}

const CRDT_STATE_KEY_ID: uuid::Uuid =
    uuid::Uuid::from_u128(0x6c61_7363_6f5f_6372_6474_5f73_7461_7465);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_rejects_an_unknown_format_version() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("crdt-state.enc");
        let key = crate::encryption::master_key::generate_master_key();
        let unsupported = VersionedCrdtState {
            format_version: CRDT_SNAPSHOT_FORMAT_VERSION + 1,
            state: CrdtState::new(DeviceId(1)),
        };
        let mut plaintext = Vec::new();
        ciborium::ser::into_writer(&unsupported, &mut plaintext).unwrap();
        let blob_key = crate::encryption::blob_key::derive_blob_key(&key, &CRDT_STATE_KEY_ID);
        let encrypted = crate::encryption::blob::encrypt_blob(&blob_key, &plaintext);
        std::fs::write(&path, encrypted.to_bytes()).unwrap();

        assert!(matches!(
            load_persisted(&path, &key, DeviceId(2)),
            Err(PersistenceError::UnsupportedFormatVersion(version))
                if version == CRDT_SNAPSHOT_FORMAT_VERSION + 1
        ));
    }
}
