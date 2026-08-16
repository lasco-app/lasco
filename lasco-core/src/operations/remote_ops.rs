use uuid::Uuid;

use crate::identifiers::CompactedOpId;
use crate::library::sync::remote_access::{StorageRead, StorageReadWrite};
use crate::operations::error::OperationError;
use crate::storage::AtomicWriteMode;

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
///
/// Fetch callers receive this read-only view rather than raw storage access.
pub(crate) async fn list_remote_op_files(storage: &StorageRead<'_>) -> Result<Vec<RemoteOpFile>> {
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

/// Writes already-encrypted compaction file bytes to `key`, without encoding or encrypting
/// again. Used when the same bytes must also be written to the local cache, so both copies
/// share one ciphertext instead of being encrypted twice with two different nonces.
pub(crate) async fn write_compaction_bytes(
    storage: &StorageReadWrite<'_>,
    key: &str,
    bytes: &[u8],
) -> Result<()> {
    storage
        .put_atomic(key, bytes, AtomicWriteMode::Replace)
        .await?;
    Ok(())
}
