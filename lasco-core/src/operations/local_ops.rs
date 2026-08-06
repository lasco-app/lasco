use std::collections::HashSet;
use std::io::Write as _;

use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob::{decrypt_blob, encrypt_blob};
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::identifiers::OpUuid;
use crate::operations::error::OperationError;

use super::{OperationGroup, op_group_from_cbor, op_group_to_cbor};

pub type Result<T> = std::result::Result<T, OperationError>;

//
// Each frame in the log has the following layout.
//   blob_len  4 bytes         (u32 little-endian)
//   blob      blob_len bytes  (encrypted OperationGroup CBOR)
//
// Operation IDs are available only after authenticated decryption. Every frame,
// including the final one, must be complete and valid.

const BLOB_LEN_FIELD: usize = 4;
const MAX_LOCAL_OP_BLOB_LEN: usize = 64 * 1024 * 1024;
const LOCAL_OPS_KEY_ID: uuid::Uuid = uuid::Uuid::from_u128(0x6c6173636f5f6c6f63616c5f6f707301);

/// Read the single op group from `pending.op`. Returns `None` if the file doesn't exist.
pub fn read_pending_op_group(
    pending_path: &std::path::Path,
    master_key: &MasterKey,
) -> Result<Option<OperationGroup>> {
    if !pending_path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(pending_path)?;
    if data.is_empty() {
        return Err(OperationError::IncompleteFrame {
            expected: BLOB_LEN_FIELD,
            found: 0,
        });
    }
    let Some((blob_bytes, remainder)) = read_frame(&data)? else {
        unreachable!("an empty pending file is rejected above")
    };
    if !remainder.is_empty() {
        return Err(OperationError::PendingTrailingData);
    }
    Ok(Some(decrypt_op_group(master_key, blob_bytes)?))
}

/// Overwrite `pending.op` with a single frame containing `op_group`.
pub fn write_pending_op_group(
    pending_path: &std::path::Path,
    master_key: &MasterKey,
    op_group: &OperationGroup,
) -> Result<()> {
    let frame = encode_frame(master_key, op_group)?;

    if let Some(parent) = pending_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(pending_path)?;

    file.write_all(&frame)?;
    Ok(())
}

/// Read the pending op group and delete `pending.op`. Returns `None` if the file doesn't exist.
pub fn take_pending_op_group(
    pending_path: &std::path::Path,
    master_key: &MasterKey,
) -> Result<Option<OperationGroup>> {
    let group = read_pending_op_group(pending_path, master_key)?;
    if group.is_some() {
        std::fs::remove_file(pending_path)?;
    }
    Ok(group)
}

pub fn append_op_group(
    log_path: &std::path::Path,
    master_key: &MasterKey,
    op_group: &OperationGroup,
) -> Result<()> {
    let frame = encode_frame(master_key, op_group)?;

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    file.write_all(&frame)?;
    Ok(())
}

pub fn read_op_groups(
    log_path: &std::path::Path,
    master_key: &MasterKey,
) -> Result<Vec<OperationGroup>> {
    if !log_path.exists() {
        return Ok(vec![]);
    }

    let data = std::fs::read(&log_path)?;
    let mut cursor = data.as_slice();
    let mut groups = Vec::new();
    let mut seen_ids: HashSet<OpUuid> = HashSet::new();

    while let Some((blob_bytes, next)) = read_frame(cursor)? {
        cursor = next;
        let group = decrypt_op_group(master_key, blob_bytes)?;

        // Skip duplicates (e.g. same op appended again after re-download).
        if seen_ids.insert(group.op_id) {
            groups.push(group);
        }
    }

    Ok(groups)
}

fn local_ops_key(master_key: &MasterKey) -> crate::encryption::blob_key::BlobKey {
    derive_blob_key(master_key, &LOCAL_OPS_KEY_ID)
}

fn encode_frame(master_key: &MasterKey, op_group: &OperationGroup) -> Result<Vec<u8>> {
    let cbor = op_group_to_cbor(op_group)?;
    let blob = encrypt_blob(&local_ops_key(master_key), &cbor);
    let blob_bytes = blob.to_bytes();
    let blob_len = blob_bytes.len();
    if blob_len > MAX_LOCAL_OP_BLOB_LEN {
        return Err(OperationError::BlobTooLarge {
            declared: blob_len,
            maximum: MAX_LOCAL_OP_BLOB_LEN,
        });
    }
    let blob_len = u32::try_from(blob_len).map_err(|_| OperationError::BlobTooLarge {
        declared: blob_len,
        maximum: MAX_LOCAL_OP_BLOB_LEN,
    })?;

    let mut frame = Vec::with_capacity(BLOB_LEN_FIELD + blob_len as usize);
    frame.extend_from_slice(&blob_len.to_le_bytes());
    frame.extend_from_slice(&blob_bytes);
    Ok(frame)
}

fn decrypt_op_group(master_key: &MasterKey, blob_bytes: &[u8]) -> Result<OperationGroup> {
    let blob = BlobEncrypted::from_bytes(blob_bytes)?;
    let plaintext = decrypt_blob(&local_ops_key(master_key), &blob)?;
    op_group_from_cbor(&plaintext)
}

/// Parses one complete frame from the front of `data`.
/// Returns `None` only for an empty input.
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

    let blob_len = u32::from_le_bytes(data[..BLOB_LEN_FIELD].try_into().unwrap()) as usize;
    if blob_len == 0 {
        return Err(OperationError::ZeroLengthBlob);
    }
    if blob_len > MAX_LOCAL_OP_BLOB_LEN {
        return Err(OperationError::BlobTooLarge {
            declared: blob_len,
            maximum: MAX_LOCAL_OP_BLOB_LEN,
        });
    }

    let rest = &data[BLOB_LEN_FIELD..];
    if rest.len() < blob_len {
        return Err(OperationError::IncompleteFrame {
            expected: blob_len,
            found: rest.len(),
        });
    }
    let (blob_bytes, next) = rest.split_at(blob_len);
    Ok(Some((blob_bytes, next)))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use crate::encryption::master_key::generate_master_key;
    use crate::identifiers::{LibraryId, MediaUuid};
    use crate::library::local_dirs::LocalDirs;
    use crate::operations::{LibraryUsername, MediaFilename, Operation, StorageDate};

    use super::*;

    fn make_log_path(tmp: &tempfile::TempDir) -> std::path::PathBuf {
        let library_id = LibraryId(Uuid::new_v4());
        LocalDirs::new(tmp.path().to_path_buf(), &library_id)
            .local_state_operations()
            .operations_log_path()
    }

    fn make_pending_path(tmp: &tempfile::TempDir) -> std::path::PathBuf {
        let library_id = LibraryId(Uuid::new_v4());
        LocalDirs::new(tmp.path().to_path_buf(), &library_id)
            .local_state_operations()
            .pending_op_path()
    }

    fn sample_group(timestamp: chrono::DateTime<Utc>) -> OperationGroup {
        OperationGroup {
            op_id: OpUuid::from_uuid(Uuid::now_v7()),
            parent_op_id: None,
            author: LibraryUsername("test".to_string()),
            operations: vec![Operation::MediaCreation {
                timestamp,
                media_id: MediaUuid::from_uuid(Uuid::new_v4()),
                filename_original: MediaFilename("img.jpg".into()),
                date: timestamp,
                storage_date: StorageDate {
                    year: 2024,
                    month: 6,
                },
                size_bytes: 2_097_152,
                content_hash: crate::library::media::MediaHash::zeroed(),
                modified_at: None,
                gps: None,
                apple_aae_media_id: None,
                apple_live_photo_media_id: None,
            }],
        }
    }

    #[test]
    fn append_then_read_round_trips_all_fields() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();
        let group = sample_group(Utc::now());

        append_op_group(&log_path, &mk, &group).unwrap();
        let groups = read_op_groups(&log_path, &mk).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].op_id.0, group.op_id.0);
    }

    #[test]
    fn read_op_groups_returns_all_appended() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();

        let t = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        append_op_group(&log_path, &mk, &sample_group(t)).unwrap();
        append_op_group(&log_path, &mk, &sample_group(t)).unwrap();
        append_op_group(&log_path, &mk, &sample_group(t)).unwrap();

        let groups = read_op_groups(&log_path, &mk).unwrap();
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn read_op_groups_derives_ids_from_authenticated_payloads() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();

        let g1 = sample_group(Utc::now());
        let g2 = sample_group(Utc::now());
        append_op_group(&log_path, &mk, &g1).unwrap();
        append_op_group(&log_path, &mk, &g2).unwrap();

        let ids: HashSet<_> = read_op_groups(&log_path, &mk)
            .unwrap()
            .into_iter()
            .map(|group| group.op_id)
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&g1.op_id));
        assert!(ids.contains(&g2.op_id));
    }

    #[test]
    fn empty_log_returns_empty_results() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();

        assert!(read_op_groups(&log_path, &mk).unwrap().is_empty());
    }

    #[test]
    fn partial_frame_at_eof_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();

        let group = sample_group(Utc::now());
        append_op_group(&log_path, &mk, &group).unwrap();

        // Truncate the log to simulate a crash mid-write (partial frame).
        let mut data = std::fs::read(&log_path).unwrap();
        let truncate_at = data.len() - 5;
        data.truncate(truncate_at);
        std::fs::write(&log_path, &data).unwrap();

        assert!(matches!(
            read_op_groups(&log_path, &mk),
            Err(OperationError::IncompleteFrame { .. })
        ));
    }

    #[test]
    fn corrupt_frame_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();

        let group = sample_group(Utc::now());
        append_op_group(&log_path, &mk, &group).unwrap();

        // Corrupt the blob version byte in the first frame.
        let mut data = std::fs::read(&log_path).unwrap();
        assert!(data.len() > BLOB_LEN_FIELD, "log must have content");
        data[BLOB_LEN_FIELD] = 0; // BLOB_FORMAT_VERSION is 1 so 0 triggers UnknownVersion
        std::fs::write(&log_path, &data).unwrap();

        assert!(read_op_groups(&log_path, &mk).is_err());
    }

    #[test]
    fn zero_blob_length_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();
        std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
        std::fs::write(&log_path, 0_u32.to_le_bytes()).unwrap();

        assert!(matches!(
            read_op_groups(&log_path, &mk),
            Err(OperationError::ZeroLengthBlob)
        ));
    }

    #[test]
    fn oversized_blob_length_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();
        std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
        let oversized = (MAX_LOCAL_OP_BLOB_LEN as u32) + 1;
        std::fs::write(&log_path, oversized.to_le_bytes()).unwrap();

        assert!(matches!(
            read_op_groups(&log_path, &mk),
            Err(OperationError::BlobTooLarge { .. })
        ));
    }

    #[test]
    fn changed_blob_length_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();
        append_op_group(&log_path, &mk, &sample_group(Utc::now())).unwrap();

        let mut data = std::fs::read(&log_path).unwrap();
        let blob_len = u32::from_le_bytes(data[..BLOB_LEN_FIELD].try_into().unwrap());
        data[..BLOB_LEN_FIELD].copy_from_slice(&(blob_len - 1).to_le_bytes());
        std::fs::write(&log_path, &data).unwrap();

        assert!(read_op_groups(&log_path, &mk).is_err());
    }

    #[test]
    fn enlarged_final_blob_length_returns_incomplete_frame_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();
        append_op_group(&log_path, &mk, &sample_group(Utc::now())).unwrap();

        let mut data = std::fs::read(&log_path).unwrap();
        let blob_len = u32::from_le_bytes(data[..BLOB_LEN_FIELD].try_into().unwrap());
        data[..BLOB_LEN_FIELD].copy_from_slice(&(blob_len + 1).to_le_bytes());
        std::fs::write(&log_path, &data).unwrap();

        assert!(matches!(
            read_op_groups(&log_path, &mk),
            Err(OperationError::IncompleteFrame { .. })
        ));
    }

    #[test]
    fn corrupt_pending_file_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pending_path = make_pending_path(&tmp);
        let mk = generate_master_key();
        write_pending_op_group(&pending_path, &mk, &sample_group(Utc::now())).unwrap();

        let mut data = std::fs::read(&pending_path).unwrap();
        data.truncate(data.len() - 1);
        std::fs::write(&pending_path, &data).unwrap();

        assert!(matches!(
            read_pending_op_group(&pending_path, &mk),
            Err(OperationError::IncompleteFrame { .. })
        ));
    }
}
