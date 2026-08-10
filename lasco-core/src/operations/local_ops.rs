//! Append-only encrypted frames containing one `CrdtOperation` each.

use std::collections::HashSet;
use std::io::Write as _;

use crate::crdt::{CrdtOperation, Dot};
use crate::encryption::blob::{BlobEncrypted, decrypt_blob, encrypt_blob};
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::operations::error::OperationError;

pub type Result<T> = std::result::Result<T, OperationError>;

const BLOB_LEN_FIELD: usize = 4;
const MAX_LOCAL_OP_BLOB_LEN: usize = 64 * 1024 * 1024;
const LOCAL_OPS_KEY_ID: uuid::Uuid = uuid::Uuid::from_u128(0x6c6173636f5f6c6f63616c5f6f707302);

pub fn append_crdt_operation(
    log_path: &std::path::Path,
    master_key: &MasterKey,
    operation: &CrdtOperation,
) -> Result<()> {
    let frame = encode_frame(master_key, operation)?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?
        .write_all(&frame)?;
    Ok(())
}

/// Reads valid operations once, deduplicated by immutable dot identity.
pub fn read_crdt_operations(
    log_path: &std::path::Path,
    master_key: &MasterKey,
) -> Result<Vec<CrdtOperation>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read(log_path)?;
    let mut cursor = data.as_slice();
    let mut operations = Vec::new();
    let mut known = HashSet::new();
    while let Some((blob, remainder)) = read_frame(cursor)? {
        cursor = remainder;
        let operation = decrypt_operation(master_key, blob)?;
        if known.insert(operation.dot) {
            operations.push(operation);
        }
    }
    Ok(operations)
}

pub fn read_known_dots(log_path: &std::path::Path, master_key: &MasterKey) -> Result<HashSet<Dot>> {
    Ok(read_crdt_operations(log_path, master_key)?
        .into_iter()
        .map(|operation| operation.dot)
        .collect())
}

pub(crate) fn crdt_operation_to_cbor(operation: &CrdtOperation) -> Result<Vec<u8>> {
    let mut cbor = Vec::new();
    ciborium::ser::into_writer(operation, &mut cbor)
        .map_err(|error| OperationError::Serialize(error.to_string()))?;
    Ok(cbor)
}

pub(crate) fn crdt_operation_from_cbor(bytes: &[u8]) -> Result<CrdtOperation> {
    ciborium::de::from_reader(bytes).map_err(|error| OperationError::Deserialize(error.to_string()))
}

fn local_ops_key(master_key: &MasterKey) -> crate::encryption::blob_key::BlobKey {
    derive_blob_key(master_key, &LOCAL_OPS_KEY_ID)
}

fn encode_frame(master_key: &MasterKey, operation: &CrdtOperation) -> Result<Vec<u8>> {
    let plaintext = crdt_operation_to_cbor(operation)?;
    let blob = encrypt_blob(&local_ops_key(master_key), &plaintext);
    let bytes = blob.to_bytes();
    if bytes.len() > MAX_LOCAL_OP_BLOB_LEN {
        return Err(OperationError::BlobTooLarge {
            declared: bytes.len(),
            maximum: MAX_LOCAL_OP_BLOB_LEN,
        });
    }
    let len = u32::try_from(bytes.len()).map_err(|_| OperationError::BlobTooLarge {
        declared: bytes.len(),
        maximum: MAX_LOCAL_OP_BLOB_LEN,
    })?;
    let mut frame = Vec::with_capacity(BLOB_LEN_FIELD + bytes.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&bytes);
    Ok(frame)
}

fn decrypt_operation(master_key: &MasterKey, bytes: &[u8]) -> Result<CrdtOperation> {
    let blob = BlobEncrypted::from_bytes(bytes)?;
    let plaintext = decrypt_blob(&local_ops_key(master_key), &blob)?;
    crdt_operation_from_cbor(&plaintext)
}

fn read_frame(data: &[u8]) -> Result<Option<(&[u8], &[u8])>> {
    if data.is_empty() {
        return Ok(None);
    }
    if data.len() < BLOB_LEN_FIELD {
        return Err(OperationError::IncompleteFrame {
            expected: BLOB_LEN_FIELD,
            found: data.len(),
        });
    }
    let len = u32::from_le_bytes(data[..BLOB_LEN_FIELD].try_into().unwrap()) as usize;
    if len == 0 {
        return Err(OperationError::ZeroLengthBlob);
    }
    if len > MAX_LOCAL_OP_BLOB_LEN {
        return Err(OperationError::BlobTooLarge {
            declared: len,
            maximum: MAX_LOCAL_OP_BLOB_LEN,
        });
    }
    let rest = &data[BLOB_LEN_FIELD..];
    if rest.len() < len {
        return Err(OperationError::IncompleteFrame {
            expected: len,
            found: rest.len(),
        });
    }
    Ok(Some(rest.split_at(len)))
}
