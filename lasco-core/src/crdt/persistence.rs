use serde::{Deserialize, Serialize};

use super::{CanonicalState, CrdtOperation, DeviceId};

/// The encrypted local snapshot written atomically after a merge. The outbox is
/// deliberately separate from causal context: it records delivery obligations,
/// whereas causal context records operations already incorporated in state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CrdtStateReplica {
    pub state: CanonicalState,
    pub outgoing: Vec<CrdtOperation>,
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
}

/// Reads a complete canonical-state snapshot. There is intentionally no reader
/// for the pre-CRDT operation log; this format is an explicit local-data reset.
pub fn load_persisted(
    path: &std::path::Path,
    master_key: &crate::encryption::master_key::MasterKey,
    device_id: DeviceId,
) -> Result<CrdtStateReplica, PersistenceError> {
    if !path.exists() {
        return Ok(CrdtStateReplica {
            state: CanonicalState::new(device_id),
            outgoing: Vec::new(),
        });
    }
    let encrypted = crate::encryption::blob::BlobEncrypted::from_bytes(&std::fs::read(path)?)?;
    let key = crate::encryption::blob_key::derive_blob_key(master_key, &CRDT_STATE_KEY_ID);
    let bytes = crate::encryption::blob::decrypt_blob(&key, &encrypted)?;
    ciborium::de::from_reader(bytes.as_slice())
        .map_err(|error| PersistenceError::Deserialize(error.to_string()))
}

/// Atomically encrypts and replaces the canonical state and durable outbox.
pub fn save_persisted(
    path: &std::path::Path,
    master_key: &crate::encryption::master_key::MasterKey,
    persisted: &CrdtStateReplica,
) -> Result<(), PersistenceError> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(persisted, &mut bytes)
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
