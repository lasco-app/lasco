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
const LOG_MAGIC: [u8; 4] = *b"LOPS";
const LOG_HEADER_LEN: usize = LOG_MAGIC.len() + size_of::<u32>();
const LOCAL_OP_LOG_FORMAT_VERSION: u32 = 1;
const LOCAL_OPS_KEY_ID: uuid::Uuid =
    uuid::Uuid::from_u128(0x6c61_7363_6f5f_6c6f_6361_6c5f_6f70_7302);

pub(crate) fn append_crdt_operation(
    log_path: &std::path::Path,
    master_key: &MasterKey,
    operation: &CrdtOperation,
) -> Result<()> {
    ensure_current_log_format(log_path, master_key)?;
    let frame = encode_frame(master_key, operation)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?
        .write_all(&frame)?;
    Ok(())
}

/// Reads valid operations once, deduplicated by immutable dot identity.
pub(crate) fn read_crdt_operations(
    log_path: &std::path::Path,
    master_key: &MasterKey,
) -> Result<Vec<CrdtOperation>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read(log_path)?;
    let mut cursor = log_frames(&data)?;
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

/// Ensures subsequent appends use the current file-level framing. Legacy headerless logs are
/// read and rewritten as the current format; duplicate dots remain intentionally harmless.
fn ensure_current_log_format(log_path: &std::path::Path, master_key: &MasterKey) -> Result<()> {
    if !log_path.exists() || std::fs::metadata(log_path)?.len() == 0 {
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(log_path, current_log_header())?;
        return Ok(());
    }

    let data = std::fs::read(log_path)?;
    if is_current_log(&data)? {
        return Ok(());
    }

    let operations = read_crdt_operations(log_path, master_key)?;
    let mut rewritten = current_log_header();
    for operation in &operations {
        rewritten.extend(encode_frame(master_key, operation)?);
    }
    crate::atomic_file::write(log_path, &rewritten)?;
    Ok(())
}

fn current_log_header() -> Vec<u8> {
    let mut header = Vec::with_capacity(LOG_HEADER_LEN);
    header.extend_from_slice(&LOG_MAGIC);
    header.extend_from_slice(&LOCAL_OP_LOG_FORMAT_VERSION.to_le_bytes());
    header
}

/// Returns the bytes containing frames. Headerless files are the supported legacy format.
fn log_frames(data: &[u8]) -> Result<&[u8]> {
    if !data.starts_with(&LOG_MAGIC) {
        return Ok(data);
    }
    if data.len() < LOG_HEADER_LEN {
        return Err(OperationError::IncompleteFrame {
            expected: LOG_HEADER_LEN,
            found: data.len(),
        });
    }
    let version = u32::from_le_bytes(data[LOG_MAGIC.len()..LOG_HEADER_LEN].try_into().unwrap());
    if version != LOCAL_OP_LOG_FORMAT_VERSION {
        return Err(OperationError::UnsupportedLocalOperationLogVersion(version));
    }
    Ok(&data[LOG_HEADER_LEN..])
}

fn is_current_log(data: &[u8]) -> Result<bool> {
    if !data.starts_with(&LOG_MAGIC) {
        return Ok(false);
    }
    let _ = log_frames(data)?;
    Ok(true)
}

pub(crate) fn read_known_dots(
    log_path: &std::path::Path,
    master_key: &MasterKey,
) -> Result<HashSet<Dot>> {
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
    let len = u32::try_from(bytes.len()).map_err(|_length_error| OperationError::BlobTooLarge {
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

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::tempdir;

    use super::*;
    use crate::crdt::{CrdtOperation, DeviceId, Dot, OperationContent};
    use crate::operations::LibraryUsername;

    fn operation(counter: u64) -> CrdtOperation {
        CrdtOperation {
            dot: Dot {
                lamport_counter: counter,
                device_id: DeviceId(1),
            },
            author: LibraryUsername("alice".into()),
            timestamp: Utc::now(),
            content: OperationContent::GroupDeletion {
                group_id: crate::identifiers::GroupUuid::from_uuid(uuid::Uuid::new_v4()),
            },
        }
    }

    #[test]
    fn append_creates_a_versioned_log_header() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("operations.log");
        let key = crate::encryption::master_key::generate_master_key();

        append_crdt_operation(&path, &key, &operation(1)).unwrap();

        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..LOG_MAGIC.len()], LOG_MAGIC);
        assert_eq!(
            u32::from_le_bytes(bytes[LOG_MAGIC.len()..LOG_HEADER_LEN].try_into().unwrap()),
            LOCAL_OP_LOG_FORMAT_VERSION
        );
    }

    #[test]
    fn append_migrates_a_legacy_headerless_log() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("operations.log");
        let key = crate::encryption::master_key::generate_master_key();
        let first = operation(1);
        std::fs::write(&path, encode_frame(&key, &first).unwrap()).unwrap();

        append_crdt_operation(&path, &key, &operation(2)).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(&LOG_MAGIC));
        assert_eq!(read_crdt_operations(&path, &key).unwrap().len(), 2);
    }
}
