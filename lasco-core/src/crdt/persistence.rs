use super::{CrdtState, DeviceId};

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
}

impl PersistenceError {
    /// Only structural snapshot failures are safe to rebuild from the operation log.
    #[must_use]
    pub fn is_recoverable_snapshot_failure(&self) -> bool {
        matches!(self, Self::Deserialize(_))
    }
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
    let mut state: CrdtState = ciborium::de::from_reader(bytes.as_slice())
        .map_err(|error| PersistenceError::Deserialize(error.to_string()))?;
    state.set_device_id(device_id);
    state.rebuild_views();
    Ok(state)
}

/// Atomically encrypts and replaces the `CrdtState` snapshot.
pub(crate) fn save_persisted(
    path: &std::path::Path,
    master_key: &crate::encryption::master_key::MasterKey,
    state: &CrdtState,
) -> Result<(), PersistenceError> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(state, &mut bytes)
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
    use crate::crdt::{CrdtOperation, Dot, OperationContent};
    use crate::identifiers::AlbumUuid;
    use crate::operations::LibraryUsername;
    use chrono::Utc;

    #[test]
    fn snapshot_that_cannot_be_parsed_is_a_recoverable_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("crdt-state.enc");
        let key = crate::encryption::master_key::generate_master_key();
        let blob_key = crate::encryption::blob_key::derive_blob_key(&key, &CRDT_STATE_KEY_ID);
        let encrypted = crate::encryption::blob::encrypt_blob(&blob_key, b"not a crdt snapshot");
        std::fs::write(&path, encrypted.to_bytes()).unwrap();

        let error = load_persisted(&path, &key, DeviceId(2)).unwrap_err();
        assert!(matches!(error, PersistenceError::Deserialize(_)));
        assert!(error.is_recoverable_snapshot_failure());
    }

    #[test]
    fn snapshot_keeps_crdt_metadata_and_clock() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("crdt-state.enc");
        let master_key = crate::encryption::master_key::generate_master_key();
        let operation = CrdtOperation {
            dot: Dot {
                lamport_counter: 7,
                device_id: DeviceId(2),
            },
            author: LibraryUsername("test".into()),
            timestamp: Utc::now(),
            content: OperationContent::AlbumDeletion {
                album_id: AlbumUuid::from_uuid(uuid::Uuid::from_u128(1)),
            },
        };
        let mut persisted = CrdtState::new(DeviceId(u128::MAX));
        persisted.apply(&operation);

        save_persisted(&path, &master_key, &persisted).unwrap();
        let mut loaded = load_persisted(&path, &master_key, DeviceId(99)).unwrap();

        assert_ne!(loaded.device_id(), persisted.device_id());
        assert_eq!(
            loaded.next_local_dot(),
            Dot {
                lamport_counter: 8,
                device_id: DeviceId(99),
            }
        );
    }
}
