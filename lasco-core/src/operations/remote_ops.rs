use uuid::Uuid;

use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob::decrypt_blob;
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::identifiers::CompactedOpId;
use crate::library::sync::remote_access::{StorageRead, StorageReadWrite};
use crate::operations::error::OperationError;
use crate::storage::Storage;

use super::{CompactionFile, compaction_file_from_cbor};

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
pub(crate) async fn list_remote_op_files(storage: &dyn Storage) -> Result<Vec<RemoteOpFile>> {
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
            if let Some(rest) = ext.strip_prefix("op")
                && let Some((tier_str, count_str)) = rest.split_once('_')
                && let (Ok(tier), Ok(op_count), Ok(uuid)) = (
                    tier_str.parse::<u8>(),
                    count_str.parse::<u32>(),
                    stem.parse::<Uuid>(),
                )
                && tier >= 1
            {
                files.push(RemoteOpFile::Compaction {
                    uuid: CompactedOpId::from_uuid(uuid),
                    tier,
                    op_count,
                });
            }
        }
    }
    Ok(files)
}

/// Reads and decrypts a compaction file at `key`. The `file_uuid` drives key derivation.
pub(crate) async fn read_compaction_file(
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
pub(crate) async fn write_compaction_file(
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
    storage.put_atomic(key, bytes).await?;
    Ok(())
}
