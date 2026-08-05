use uuid::Uuid;

use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob::decrypt_blob;
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::identifiers::CompactedOpId;
use crate::library::sync::remote_access::{StorageRead, StorageReadWrite};
use crate::operations::error::OperationError;
use crate::storage::Storage;

#[allow(unused_imports, reason = "MediaFilename is used in tests")]
use super::{CompactionFile, MediaFilename, OperationGroup, compaction_file_from_cbor};

pub type Result<T> = std::result::Result<T, OperationError>;

/// A classified entry under `operations/` on the remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteOpFile {
    /// A compaction file `{uuid}.opN_{op_count}` grouping multiple op groups at `tier` N ≥ 1.
    /// `op_count` is the total number of operations summed across every group in the file.
    Compaction {
        uuid: CompactedOpId,
        tier: u8,
        op_count: u32,
    },
}

/// Lists and classifies all operation files under `operations/` on the remote.
/// Skips any key whose filename starts with `LOCK`.
pub async fn list_remote_op_files(storage: &dyn Storage) -> Result<Vec<RemoteOpFile>> {
    let remote = StorageRead::new(storage);
    list_remote_op_files_read(&remote).await
}

/// Read-only variant used by fetch, which must not receive raw storage access.
pub(crate) async fn list_remote_op_files_read(
    storage: &StorageRead<'_>,
) -> Result<Vec<RemoteOpFile>> {
    let keys = match storage.list("operations/").await {
        Ok(keys) => keys,
        Err(crate::storage::StorageError::NotFound) => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    let mut files = Vec::new();
    for key in &keys {
        let name = key.strip_prefix("operations/").unwrap_or(key.as_str());
        if name.starts_with("LOCK") {
            continue;
        }
        if let Some(dot) = name.rfind('.') {
            let stem = &name[..dot];
            let ext = &name[dot + 1..];
            if let Some(rest) = ext.strip_prefix("op") {
                if let Some((tier_str, count_str)) = rest.split_once('_') {
                    if let (Ok(tier), Ok(op_count), Ok(uuid)) = (
                        tier_str.parse::<u8>(),
                        count_str.parse::<u32>(),
                        stem.parse::<Uuid>(),
                    ) {
                        if tier >= 1 {
                            files.push(RemoteOpFile::Compaction {
                                uuid: CompactedOpId::from_uuid(uuid),
                                tier,
                                op_count,
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(files)
}

/// Reads and decrypts a compaction file at `key`. The `file_uuid` drives key derivation.
pub async fn read_compaction_file(
    storage: &dyn Storage,
    master_key: &MasterKey,
    key: &str,
    file_uuid: &CompactedOpId,
) -> Result<CompactionFile> {
    let remote = StorageRead::new(storage);
    read_compaction_file_read(&remote, master_key, key, file_uuid).await
}

/// Read-only variant used by procedures with restricted remote access.
pub(crate) async fn read_compaction_file_read(
    storage: &StorageRead<'_>,
    master_key: &MasterKey,
    key: &str,
    file_uuid: &CompactedOpId,
) -> Result<CompactionFile> {
    let bytes = storage.get(key).await?;
    let blob = BlobEncrypted::from_bytes(&bytes)?;
    let file_key = derive_blob_key(master_key, &file_uuid.0);
    let plaintext = decrypt_blob(&file_key, &blob)?;
    compaction_file_from_cbor(&plaintext)
}

/// Encrypts and writes a compaction file to `key`. The `file_uuid` drives key derivation.
pub async fn write_compaction_file(
    storage: &dyn Storage,
    master_key: &MasterKey,
    key: &str,
    file_uuid: &CompactedOpId,
    file: &CompactionFile,
) -> Result<()> {
    let remote = StorageReadWrite::new(storage);
    write_compaction_file_write(&remote, master_key, key, file_uuid, file).await
}

/// Read/write variant used by procedures with restricted remote access.
pub(crate) async fn write_compaction_file_write(
    storage: &StorageReadWrite<'_>,
    master_key: &MasterKey,
    key: &str,
    file_uuid: &CompactedOpId,
    file: &CompactionFile,
) -> Result<()> {
    let blob = super::encrypt_compaction_file(master_key, file_uuid, file)?;
    write_compaction_bytes(storage, key, &blob.to_bytes()).await
}

/// Writes already-encrypted compaction file bytes to `key`, without encoding or encrypting
/// again. Used when the same bytes must also be written to the local cache, so both copies
/// share one ciphertext instead of being encrypted twice with two different nonces.
pub(crate) async fn write_compaction_bytes(
    storage: &StorageReadWrite<'_>,
    key: &str,
    bytes: &[u8],
) -> Result<()> {
    storage.put(key, bytes).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use crate::encryption::master_key::generate_master_key;
    use crate::identifiers::MediaUuid;
    use crate::operations::{CompactionEntry, LibraryUsername, Operation, StorageDate};
    use crate::storage::StorageMockMemory;

    use super::*;

    fn make_master_key() -> MasterKey {
        generate_master_key()
    }

    fn sample_group(timestamp: chrono::DateTime<Utc>) -> OperationGroup {
        OperationGroup {
            op_id: super::super::OpUuid::new(),
            parent_op_id: None,
            author: LibraryUsername("test".to_string()),
            operations: vec![Operation::MediaCreation {
                timestamp,
                media_id: MediaUuid::from_uuid(uuid::Uuid::new_v4()),
                filename_original: MediaFilename("photo.jpg".into()),
                date: timestamp,
                storage_date: StorageDate {
                    year: 2024,
                    month: 3,
                },
                size_bytes: 1_048_576,
                content_hash: crate::library::media::MediaHash::zeroed(),
                modified_at: None,
                gps: None,
                apple_aae_media_id: None,
                apple_live_photo_media_id: None,
            }],
        }
    }

    #[tokio::test]
    async fn list_remote_op_files_classifies_correctly() {
        let storage = StorageMockMemory::new();
        let mk = make_master_key();

        let compact_uuid = CompactedOpId::new();
        let group = sample_group(Utc::now());
        let compact_file = CompactionFile {
            tier: 1,
            contents: vec![CompactionEntry {
                op_id: group.op_id,
                group,
            }],
        };
        let compact_key = format!("operations/{compact_uuid}.op1_1");
        write_compaction_file(&storage, &mk, &compact_key, &compact_uuid, &compact_file)
            .await
            .unwrap();

        // Inject a LOCK key that should be skipped.
        storage.put("operations/LOCK.op", b"locked").await.unwrap();

        let files = list_remote_op_files(&storage).await.unwrap();
        assert_eq!(files.len(), 1);
        assert!(matches!(files[0], RemoteOpFile::Compaction { tier: 1, .. }));
    }

    #[tokio::test]
    async fn compaction_file_round_trip() {
        let storage = StorageMockMemory::new();
        let mk = make_master_key();
        let t = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();
        let group = sample_group(t);

        let file_uuid = CompactedOpId::new();
        let key = format!("operations/{file_uuid}.op2_1");
        let original = CompactionFile {
            tier: 2,
            contents: vec![CompactionEntry {
                op_id: group.op_id,
                group: group.clone(),
            }],
        };

        write_compaction_file(&storage, &mk, &key, &file_uuid, &original)
            .await
            .unwrap();
        let recovered = read_compaction_file(&storage, &mk, &key, &file_uuid)
            .await
            .unwrap();

        assert_eq!(recovered.tier, 2);
        assert_eq!(recovered.contents.len(), 1);
        assert_eq!(recovered.contents[0].op_id.0, group.op_id.0);
    }
}
