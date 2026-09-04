use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::encryption::master_key::MasterKey;
use crate::error::SyncError;
use crate::identifiers::CompactedOpId;
use crate::operations::remote_ops::{self as op_io, RemoteOpFile};
use crate::remote::LastKnownState;
use crate::storage::AtomicWriteMode;

use super::map_op_err;
use super::remote_access::{StorageRead, StorageReadWrite};

pub(super) type Result<T> = std::result::Result<T, SyncError>;

/// Maximum number of files allowed at any compaction tier before compaction triggers.
pub(super) const TIER_FILE_LIMIT: usize = 10;

/// Returns the lowest tier whose ops limit can hold `op_count` in a single file.
///
/// Used by push to pick where to upload a batch, so it always writes exactly one
/// compaction file that stays within its tier's ops limit.
///
/// Tier N's ops limit is 20*10^(N-1), so the smallest tier holding `op_count`
/// is the smallest N with 10^(N-1) >= `ceil(op_count` / 20). Writing q for that
/// ceiling, N-1 is the number of digits of q-1, which `ilog10` gives directly.
pub(super) fn appropriate_tier(op_count: usize) -> u8 {
    let q = (op_count as u64).div_ceil(20);
    let exponent = if q <= 1 { 0 } else { (q - 1).ilog10() + 1 };
    u8::try_from(exponent + 1).expect("a usize operation count requires fewer than 256 tiers")
}

/// Returns the number of files at each tier.
pub(super) fn count_tier_files(remote_files: &[RemoteOpFile]) -> HashMap<u8, usize> {
    let mut counts: HashMap<u8, usize> = HashMap::new();
    for file in remote_files {
        match file {
            RemoteOpFile::Compaction { tier, .. } => {
                *counts.entry(*tier).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// Returns true if the given tier has reached the file limit and must be compacted.
pub(super) fn tier_needs_compaction(file_count: usize) -> bool {
    file_count >= TIER_FILE_LIMIT
}

/// Contents written to `operations/LOCK.opN` while compaction is in progress.
/// Stores a client ID and creation date for diagnostic purposes, so a person
/// inspecting the file remotely can tell who holds it and since when.
/// There is no automatic takeover of a held lock. If a lock is left behind by a
/// crashed or killed client, it must be deleted manually before compaction can proceed.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CompactionLockRecord {
    client_id: String,
    #[serde(default)]
    lease_id: Option<uuid::Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Remote key for the compaction lock (shared across all tiers).
const LOCK_KEY: &str = "operations/LOCK.op";

/// Metadata shown to a user before they explicitly remove a stale lock.
#[derive(Debug, Clone)]
pub struct CompactionLockInfo {
    pub owner_device_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn read_lock_record(storage: &StorageRead<'_>) -> Result<Option<CompactionLockRecord>> {
    let bytes = match storage.get(LOCK_KEY).await {
        Ok(bytes) => bytes,
        Err(crate::storage::StorageError::NotFound) => return Ok(None),
        Err(error) => return Err(SyncError::RemoteUnreachable(error)),
    };
    let lock: CompactionLockRecord = serde_json::from_slice(&bytes).map_err(|error| {
        SyncError::RemoteOperationInvalid(format!("invalid compaction lock: {error}"))
    })?;
    Ok(Some(lock))
}

async fn read_lock(storage: &StorageRead<'_>) -> Result<Option<CompactionLockInfo>> {
    let Some(lock) = read_lock_record(storage).await? else {
        return Ok(None);
    };
    Ok(Some(CompactionLockInfo {
        owner_device_id: lock.client_id,
        created_at: lock.created_at,
    }))
}

/// Reads the current global compaction lock without changing it.
pub async fn inspect_lock(storage: &StorageRead<'_>) -> Result<Option<CompactionLockInfo>> {
    read_lock(storage).await
}

/// Deletes a lock only if it still belongs to the expected local device.
pub async fn remove_lock_owned_by(
    storage: &StorageReadWrite<'_>,
    expected_device_id: &str,
) -> Result<bool> {
    let Some(lock) = read_lock(&storage.as_read()).await? else {
        return Ok(false);
    };
    if lock.owner_device_id != expected_device_id {
        return Ok(false);
    }
    storage
        .delete(LOCK_KEY)
        .await
        .map_err(SyncError::RemoteUnreachable)?;
    Ok(true)
}

/// Proof that the compaction lock is held. Obtained from [`try_acquire_lock`] and consumed by
/// [`release_lock`]. [`compact_tier`] requires a reference to one, so the type system rules out
/// calling it without the lock held.
pub(super) struct CompactionLockToken {
    lock: CompactionLockRecord,
}

async fn lock_is_held(storage: &StorageRead<'_>, token: &CompactionLockToken) -> Result<bool> {
    Ok(read_lock_record(storage)
        .await?
        .as_ref()
        .is_some_and(|lock| lock == &token.lock))
}

/// Tries to acquire the compaction lock.
///
/// The lock is global across all tiers, not scoped per tier. Compacting two different
/// tiers at the same time will contend on this same lock even though the merges are
/// independent.
///
/// There is no automatic takeover of a lock left behind by a crashed or killed client.
/// A stuck lock must be deleted manually from remote storage before compaction can proceed.
///
/// This is a best-effort lock for stores without an atomic conditional write. The write is
/// confirmed with a unique lease ID, which rejects a contender that was overwritten before it
/// observed ownership; it cannot prevent a contender that writes after that confirmation.
///
/// Returns `Some` token if the lock was acquired and confirmed.
/// Returns `None` if the lock is already held or ownership could not be confirmed.
pub(super) async fn try_acquire_lock(
    storage: &StorageReadWrite<'_>,
    device_id: crate::crdt::DeviceId,
) -> Result<Option<CompactionLockToken>> {
    if storage
        .exists(LOCK_KEY)
        .await
        .map_err(SyncError::RemoteUnreachable)?
    {
        return Ok(None);
    }

    let lock = CompactionLockRecord {
        client_id: device_id.to_string(),
        lease_id: Some(uuid::Uuid::new_v4()),
        created_at: chrono::Utc::now(),
    };
    let payload = serde_json::to_vec(&lock).expect("CompactionLockRecord is always serializable");

    storage
        .put_atomic(LOCK_KEY, &payload, AtomicWriteMode::Replace)
        .await
        .map_err(SyncError::RemoteUnreachable)?;

    Ok(lock_is_held(
        &storage.as_read(),
        &CompactionLockToken { lock: lock.clone() },
    )
    .await?
    .then_some(CompactionLockToken { lock }))
}

/// Releases the compaction lock.
///
/// This lock is global across all tiers, see [`try_acquire_lock`]. Takes the token by value so
/// a client can't release a lock it never acquired, and can't accidentally use the token again
/// afterwards.
pub(super) async fn release_lock(
    storage: &StorageReadWrite<'_>,
    token: CompactionLockToken,
) -> Result<()> {
    if !lock_is_held(&storage.as_read(), &token).await? {
        return Ok(());
    }
    storage
        .delete(LOCK_KEY)
        .await
        .map_err(SyncError::RemoteUnreachable)
}

/// Metadata produced by a successful [`compact_tier`] call, so the caller can update its
/// in-memory view of known files. The on-disk last known state is already up to date by the
/// time this is returned, `compact_tier` writes it incrementally as each remote op succeeds.
pub(super) struct CompactionResult {
    pub(super) sources: Vec<RemoteOpFile>,
    pub(super) new_file: RemoteOpFile,
}

/// Merges all tier files into one new file at `tier+1`, then deletes the sources.
///
/// `lock` must be held across the whole cascade if compacting multiple tiers. This
/// function never acquires or releases it, only requires proof that the caller holds it.
///
/// There is no retry: a failed remote operation stops the cascade immediately,
/// leaving the remaining sources at this tier in place.
///
/// Sources are read from `last_known_state`, not by downloading a possibly
/// out of date version from the remote.
///
/// Whether this function succeeds or fails partway through, `last_known_state`
/// is kept consistent with whatever modifications were actually made on the
/// remote.
pub(super) async fn compact_tier(
    storage: &StorageReadWrite<'_>,
    master_key: &MasterKey,
    tier: u8,
    last_known_state: &LastKnownState,
    lock: &CompactionLockToken,
) -> Result<CompactionResult> {
    // Collect all tier-N source files.
    let sources: Vec<RemoteOpFile> = last_known_state
        .files()
        .iter()
        .filter(|file| matches!(file, RemoteOpFile::Compaction { tier: file_tier, .. } if *file_tier == tier))
        .cloned()
        .collect();

    // Read all individual operations from every source file, deduplicated by dot.
    let mut operations = Vec::new();
    let mut known_dots = HashSet::new();
    for source in &sources {
        let RemoteOpFile::Compaction {
            uuid,
            tier: file_tier,
            op_count,
        } = source;
        let file =
            last_known_state.read_compaction_file(master_key, uuid, *file_tier, *op_count)?;
        operations.extend(
            file.operations
                .into_iter()
                .filter(|operation| known_dots.insert(operation.dot)),
        );
    }

    // Write the new compaction file at tier+1. Encrypted once, so the same ciphertext is
    // written to both the remote and the local cache below.
    let new_uuid = CompactedOpId::new();
    let new_tier = tier + 1;
    let new_op_count = u32::try_from(operations.len())
        .expect("an in-memory compaction cannot contain more than u32::MAX operations");
    let new_key = format!("operations/{new_uuid}.op{new_tier}_{new_op_count}");
    let new_file = crate::operations::CompactionFile {
        tier: new_tier,
        operations,
    };
    let blob = crate::operations::encrypt_compaction_file(master_key, &new_uuid, &new_file)
        .map_err(map_op_err)?;
    let bytes = blob.to_bytes();
    if !lock_is_held(&storage.as_read(), lock).await? {
        return Err(SyncError::RemoteOperationInvalid(
            "compaction lock ownership was lost".to_string(),
        ));
    }
    op_io::write_compaction_bytes(storage, &new_key, &bytes)
        .await
        .map_err(map_op_err)?;

    // Update the local cache for the new file as soon as it exists remotely, before
    // attempting any of the deletes below.
    last_known_state.write_compaction_bytes(&new_uuid, new_tier, new_op_count, &bytes)?;

    // Delete source files. The caller holds the compaction lock for the whole cascade,
    // so no other client can be reading or compacting them at the same time.
    for source in &sources {
        let RemoteOpFile::Compaction {
            uuid,
            tier: file_tier,
            op_count,
        } = source;
        let key = format!("operations/{uuid}.op{file_tier}_{op_count}");
        if !lock_is_held(&storage.as_read(), lock).await? {
            return Err(SyncError::RemoteOperationInvalid(
                "compaction lock ownership was lost".to_string(),
            ));
        }
        storage
            .delete(&key)
            .await
            .map_err(SyncError::RemoteUnreachable)?;

        // Update the local cache for this source right away, so if a later delete in this
        // loop fails, everything up to here is already accounted for on disk.
        last_known_state.remove_compaction_file(uuid, *file_tier, *op_count)?;
    }

    Ok(CompactionResult {
        sources,
        new_file: RemoteOpFile::Compaction {
            uuid: new_uuid,
            tier: new_tier,
            op_count: new_op_count,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{appropriate_tier, release_lock, try_acquire_lock};
    use crate::crdt::DeviceId;
    use crate::library::sync::remote_access::StorageReadWrite;
    use crate::storage::StorageMockMemory;

    #[test]
    fn appropriate_tier_picks_lowest_tier_that_fits() {
        assert_eq!(appropriate_tier(0), 1);
        assert_eq!(appropriate_tier(1), 1);
        assert_eq!(appropriate_tier(20), 1);
        assert_eq!(appropriate_tier(21), 2);
        assert_eq!(appropriate_tier(200), 2);
        assert_eq!(appropriate_tier(201), 3);
        assert_eq!(appropriate_tier(2000), 3);
        assert_eq!(appropriate_tier(2001), 4);
    }

    #[tokio::test]
    async fn lock_check_write_and_confirmation_allow_one_holder() {
        let storage = StorageMockMemory::new();
        let access = StorageReadWrite::new(&storage);

        let first = try_acquire_lock(&access, DeviceId(1)).await.unwrap();
        assert!(first.is_some());
        assert!(
            try_acquire_lock(&access, DeviceId(2))
                .await
                .unwrap()
                .is_none()
        );

        release_lock(&access, first.unwrap()).await.unwrap();
        assert!(
            try_acquire_lock(&access, DeviceId(2))
                .await
                .unwrap()
                .is_some()
        );
    }
}
