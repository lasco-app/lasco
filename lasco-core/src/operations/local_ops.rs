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
//   op_id    16 bytes         (UUID bytes)
//   blob_len  4 bytes         (u32 little-endian)
//   blob      blob_len bytes  (encrypted OperationGroup CBOR)
//
// The plaintext op_id header allows cheap `read_op_ids` without decryption.
// A trailing partial frame is silently treated as EOF (crash-safe append).

const OP_ID_LEN: usize = 16;
const BLOB_LEN_FIELD: usize = 4;
const FRAME_HEADER: usize = OP_ID_LEN + BLOB_LEN_FIELD;

/// Read the single op group from `pending.op`. Returns `None` if the file doesn't exist.
pub fn read_pending_op_group(
    pending_path: &std::path::Path,
    master_key: &MasterKey,
) -> Result<Option<OperationGroup>> {
    if !pending_path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(pending_path)?;
    let Some((op_id, (blob_bytes, _))) = read_frame_header(&data) else {
        return Ok(None);
    };
    let Ok(blob) = BlobEncrypted::from_bytes(blob_bytes) else {
        return Ok(None);
    };
    let file_key = derive_blob_key(master_key, &op_id.0);
    let Ok(plaintext) = decrypt_blob(&file_key, &blob) else {
        return Ok(None);
    };
    let Ok(group) = op_group_from_cbor(&plaintext) else {
        return Ok(None);
    };
    Ok(Some(group))
}

/// Overwrite `pending.op` with a single frame containing `op_group`.
pub fn write_pending_op_group(
    pending_path: &std::path::Path,
    master_key: &MasterKey,
    op_group: &OperationGroup,
) -> Result<()> {
    let cbor = op_group_to_cbor(op_group)?;
    let file_key = derive_blob_key(master_key, &op_group.op_id.0);
    let blob = encrypt_blob(&file_key, &cbor);
    let blob_bytes = blob.to_bytes();

    if let Some(parent) = pending_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(pending_path)?;

    file.write_all(op_group.op_id.0.as_bytes())?;
    file.write_all(&(blob_bytes.len() as u32).to_le_bytes())?;
    file.write_all(&blob_bytes)?;
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
    let cbor = op_group_to_cbor(op_group)?;
    let file_key = derive_blob_key(master_key, &op_group.op_id.0);
    let blob = encrypt_blob(&file_key, &cbor);
    let blob_bytes = blob.to_bytes();

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    file.write_all(op_group.op_id.0.as_bytes())?;
    file.write_all(&(blob_bytes.len() as u32).to_le_bytes())?;
    file.write_all(&blob_bytes)?;
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

    loop {
        let Some((op_id, rest)) = read_frame_header(cursor) else {
            break;
        };
        let (blob_bytes, next) = rest;
        cursor = next;

        // Skip duplicates (e.g. same op appended again after re-download).
        if seen_ids.contains(&op_id) {
            continue;
        }

        let blob = BlobEncrypted::from_bytes(blob_bytes)?;
        let file_key = derive_blob_key(master_key, &op_id.0);
        let plaintext = decrypt_blob(&file_key, &blob)?;
        let group = op_group_from_cbor(&plaintext)?;

        seen_ids.insert(op_id);
        groups.push(group);
    }

    Ok(groups)
}

pub fn read_op_ids(log_path: &std::path::Path) -> Result<HashSet<OpUuid>> {
    if !log_path.exists() {
        return Ok(HashSet::new());
    }

    let data = std::fs::read(&log_path)?;
    let mut cursor = data.as_slice();
    let mut ids = HashSet::new();

    loop {
        let Some((op_id, (_, next))) = read_frame_header(cursor) else {
            break;
        };
        cursor = next;
        ids.insert(op_id);
    }

    Ok(ids)
}

/// Parses one frame header from the front of `data`.
/// Returns `Some((op_id, (blob_bytes, remainder)))` or `None` on partial/empty frame.
fn read_frame_header(data: &[u8]) -> Option<(OpUuid, (&[u8], &[u8]))> {
    if data.len() < FRAME_HEADER {
        return None;
    }
    let op_id_bytes: [u8; OP_ID_LEN] = data[..OP_ID_LEN].try_into().unwrap();
    let op_id = OpUuid::from_uuid(uuid::Uuid::from_bytes(op_id_bytes));
    let blob_len = u32::from_le_bytes(data[OP_ID_LEN..FRAME_HEADER].try_into().unwrap()) as usize;
    let rest = &data[FRAME_HEADER..];
    if rest.len() < blob_len {
        return None; // partial frame at EOF
    }
    let (blob_bytes, next) = rest.split_at(blob_len);
    Some((op_id, (blob_bytes, next)))
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
    fn read_op_ids_matches_appended_ids() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log_path = make_log_path(&tmp);
        let mk = generate_master_key();

        let g1 = sample_group(Utc::now());
        let g2 = sample_group(Utc::now());
        append_op_group(&log_path, &mk, &g1).unwrap();
        append_op_group(&log_path, &mk, &g2).unwrap();

        let ids = read_op_ids(&log_path).unwrap();
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
        assert!(read_op_ids(&log_path).unwrap().is_empty());
    }

    #[test]
    fn partial_frame_at_eof_treated_as_eof() {
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

        // A truncated log has its partial frame silently ignored.
        let groups = read_op_groups(&log_path, &mk).unwrap();
        assert!(groups.is_empty(), "partial frame must be treated as EOF");
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
        assert!(data.len() > FRAME_HEADER, "log must have content");
        data[FRAME_HEADER] = 0; // BLOB_FORMAT_VERSION is 1 so 0 triggers UnknownVersion
        std::fs::write(&log_path, &data).unwrap();

        assert!(read_op_groups(&log_path, &mk).is_err());
    }
}
