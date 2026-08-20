use std::collections::HashSet;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::crdt::Dot;
use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob::decrypt_blob;
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::error::SyncError;
use crate::identifiers::CompactedOpId;
use crate::library::local_dirs::RemoteLastKnownStateDir;
use crate::library::sync::remote_access::StorageRead;
use crate::operations::remote_ops::{RemoteOpFile, list_remote_op_files};
use crate::operations::{CompactionFile, compaction_file_from_cbor};

/// On-disk cache of all remote operation files for a single remote.
///
/// Mirrors the remote `operations/` layout under
/// `remotes/{remote_id}/state/operations/`, keyed by the same filename
/// (`{op_id}.op` or `{uuid}.opN`). Files are written once and never
/// overwritten unless explicitly invalidated.
pub(crate) struct LastKnownState {
    ops_dir: PathBuf,
    files: Vec<RemoteOpFile>,
}

impl LastKnownState {
    /// Phase 1 of fetch. Lists remote operation files, stages every cache-missing file in
    /// memory, and validates remote history before committing staged files to disk.
    ///
    /// The durable cache is unchanged when listing, download, decryption, parsing, or history
    /// validation fails. Merge progress is deliberately not consulted here: it is independent
    /// from whether the ciphertext is present in this cache.
    pub(crate) async fn download(
        storage: &StorageRead<'_>,
        remote_last_known_state_dir: &RemoteLastKnownStateDir,
        master_key: &MasterKey,
    ) -> Result<Self, SyncError> {
        let ops_dir = remote_last_known_state_dir.operations_dir();
        std::fs::create_dir_all(&ops_dir)?;

        let remote_files = list_remote_op_files(storage).await?;
        let cached_dots =
            collect_dots_from_dir(&ops_dir, master_key).map_err(SyncError::LocalCacheCorrupt)?;
        let mut staged_files: Vec<(RemoteOpFile, Vec<u8>, Vec<Dot>)> = Vec::new();

        for file in &remote_files {
            let (remote_key, local_name) = Self::file_paths(file);
            let local_path = ops_dir.join(&local_name);
            if !local_path.exists() {
                let bytes = storage
                    .get(&remote_key)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
                let dots = read_dots_from_bytes(&bytes, master_key, file)
                    .map_err(SyncError::RemoteOperationInvalid)?;
                staged_files.push((file.clone(), bytes, dots));
            }
        }

        // Verify no immutable CRDT operations have been lost from remote history.
        if !cached_dots.is_empty() {
            let mut remote_dots: HashSet<Dot> = HashSet::new();
            for f in &remote_files {
                if let Some((_, _, ids)) = staged_files.iter().find(|(staged, _, _)| staged == f) {
                    remote_dots.extend(ids.iter().copied());
                } else {
                    let (_, local_name) = Self::file_paths(f);
                    let ids = read_dots_from_file(&ops_dir.join(local_name), master_key, f)
                        .map_err(SyncError::LocalCacheCorrupt)?;
                    remote_dots.extend(ids);
                }
            }
            if !remote_dots.is_superset(&cached_dots) {
                let mut missing: Vec<Dot> = cached_dots.difference(&remote_dots).copied().collect();
                missing.sort();
                return Err(SyncError::RemoteHistoryRewritten(format!(
                    "cached_dots={} remote_dots={} missing={missing:?}",
                    cached_dots.len(),
                    remote_dots.len(),
                )));
            }
        }

        for (file, bytes, _) in staged_files {
            let (_, local_name) = Self::file_paths(&file);
            crate::atomic_file::write(&ops_dir.join(local_name), &bytes)?;
        }

        Ok(Self {
            ops_dir,
            files: remote_files,
        })
    }

    /// Disk-only counterpart to `download`. Builds the same instance from whatever is already
    /// recorded in the on-disk last known state for `remote_id`, without contacting the remote.
    ///
    /// Used by push and compaction, which must never list or read arbitrary remote files: they
    /// only ever act on what this client itself already knows about.
    pub(crate) fn open(
        remote_last_known_state_dir: &RemoteLastKnownStateDir,
    ) -> Result<Self, SyncError> {
        let ops_dir = remote_last_known_state_dir.operations_dir();
        let files = Self::list_cached_files(remote_last_known_state_dir)?;
        Ok(Self { ops_dir, files })
    }

    pub(crate) fn file_uuid(file: &RemoteOpFile) -> CompactedOpId {
        match file {
            RemoteOpFile::Compaction { uuid, .. } => *uuid,
        }
    }

    /// All remote op files known so far, either from `download` or from `open`.
    pub(crate) fn files(&self) -> &[RemoteOpFile] {
        &self.files
    }

    /// Mutable access to the known files, for callers that add or remove entries after
    /// uploading or compacting, without re-reading the on-disk cache.
    pub(crate) fn files_mut(&mut self) -> &mut Vec<RemoteOpFile> {
        &mut self.files
    }

    /// Reads and decrypts a compaction file from the last known state dir.
    pub(crate) fn read_compaction_file(
        &self,
        master_key: &MasterKey,
        uuid: &CompactedOpId,
        tier: u8,
        op_count: u32,
    ) -> Result<CompactionFile, SyncError> {
        let path = self.ops_dir.join(format!("{uuid}.op{tier}_{op_count}"));
        let bytes = std::fs::read(&path)?;
        let blob = BlobEncrypted::from_bytes(&bytes).map_err(|e| {
            SyncError::LocalCacheCorrupt(format!("failed to parse blob {}: {e}", path.display()))
        })?;
        let file_key = derive_blob_key(master_key, &uuid.0);
        let plaintext = decrypt_blob(&file_key, &blob).map_err(|e| {
            SyncError::LocalCacheCorrupt(format!("failed to decrypt {}: {e}", path.display()))
        })?;
        let file = compaction_file_from_cbor(&plaintext).map_err(|e| {
            SyncError::LocalCacheCorrupt(format!(
                "failed to parse compaction file {}: {e}",
                path.display()
            ))
        })?;
        validate_compaction_file(&file, tier, op_count).map_err(SyncError::LocalCacheCorrupt)?;
        Ok(file)
    }

    /// Writes already-encrypted compaction file bytes into the on-disk last known state, without
    /// encoding or encrypting again. Used when the same bytes were already written to the remote,
    /// so both copies share one ciphertext instead of being encrypted twice with two different
    /// nonces.
    pub(crate) fn write_compaction_bytes(
        &self,
        uuid: &CompactedOpId,
        tier: u8,
        op_count: u32,
        bytes: &[u8],
    ) -> Result<(), SyncError> {
        std::fs::create_dir_all(&self.ops_dir)?;
        crate::atomic_file::write(
            &self.ops_dir.join(format!("{uuid}.op{tier}_{op_count}")),
            bytes,
        )?;
        Ok(())
    }

    /// Lists op files recorded in the on-disk last known state for `remote_id`, without
    /// contacting the remote. A missing cache directory is treated as empty.
    pub(crate) fn list_cached_files(
        remote_last_known_state_dir: &RemoteLastKnownStateDir,
    ) -> Result<Vec<RemoteOpFile>, SyncError> {
        let ops_dir = remote_last_known_state_dir.operations_dir();
        let entries = match std::fs::read_dir(&ops_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry?;
            if let Some(file) = parse_cached_filename(&entry.file_name().to_string_lossy()) {
                files.push(file);
            }
        }
        Ok(files)
    }

    /// Removes a file from the on-disk last known state. A missing file is not an error.
    pub(crate) fn remove_compaction_file(
        &self,
        uuid: &CompactedOpId,
        tier: u8,
        op_count: u32,
    ) -> Result<(), SyncError> {
        match std::fs::remove_file(self.ops_dir.join(format!("{uuid}.op{tier}_{op_count}"))) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn file_paths(file: &RemoteOpFile) -> (String, String) {
        match file {
            RemoteOpFile::Compaction {
                uuid,
                tier,
                op_count,
            } => (
                format!("operations/{uuid}.op{tier}_{op_count}"),
                format!("{uuid}.op{tier}_{op_count}"),
            ),
        }
    }
}

/// Reads and decrypts the op ids stored in a cached compaction file.
///
/// A missing file is treated as legitimately empty (nothing cached yet).
/// A file that exists but fails to read, decrypt, or parse is a real error
/// and must not be treated the same as "no ops here".
fn read_dots_from_file(
    path: &Path,
    master_key: &MasterKey,
    file: &RemoteOpFile,
) -> Result<Vec<Dot>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };
    read_dots_from_bytes(&bytes, master_key, file)
        .map_err(|error| format!("{} ({})", error, path.display()))
}

fn read_dots_from_bytes(
    bytes: &[u8],
    master_key: &MasterKey,
    remote_file: &RemoteOpFile,
) -> Result<Vec<Dot>, String> {
    let blob =
        BlobEncrypted::from_bytes(bytes).map_err(|e| format!("failed to parse blob: {e}"))?;
    let file_uuid = LastKnownState::file_uuid(remote_file);
    let file_key = derive_blob_key(master_key, &file_uuid.0);
    let plaintext =
        decrypt_blob(&file_key, &blob).map_err(|e| format!("failed to decrypt: {e}"))?;
    let file = compaction_file_from_cbor(&plaintext)
        .map_err(|e| format!("failed to parse compaction file: {e}"))?;
    let RemoteOpFile::Compaction { tier, op_count, .. } = remote_file;
    validate_compaction_file(&file, *tier, *op_count)?;
    Ok(file
        .operations
        .into_iter()
        .map(|operation| operation.dot)
        .collect())
}

/// Parses a cached filename like `{uuid}.opN_M` into its `RemoteOpFile` metadata.
fn parse_cached_filename(name: &str) -> Option<RemoteOpFile> {
    let (stem, ext) = name.split_once('.')?;
    let uuid = CompactedOpId::from_uuid(stem.parse::<Uuid>().ok()?);
    let (tier_str, count_str) = ext.strip_prefix("op")?.split_once('_')?;
    let tier = tier_str.parse::<u8>().ok()?;
    let op_count = count_str.parse::<u32>().ok()?;
    Some(RemoteOpFile::Compaction {
        uuid,
        tier,
        op_count,
    })
}

pub(crate) fn collect_dots_from_dir(
    dir: &Path,
    master_key: &MasterKey,
) -> Result<HashSet<Dot>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(e) => return Err(format!("failed to read dir {}: {e}", dir.display())),
    };
    let mut ids = HashSet::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to read entry in {}: {e}", dir.display()))?;
        let name = entry.file_name();
        let s = name.to_string_lossy();
        let Some(file) = parse_cached_filename(&s) else {
            continue;
        };
        ids.extend(read_dots_from_file(&entry.path(), master_key, &file)?);
    }
    Ok(ids)
}

fn validate_compaction_file(file: &CompactionFile, tier: u8, op_count: u32) -> Result<(), String> {
    if tier == 0 {
        return Err("filename tier must be at least 1".to_string());
    }
    if file.tier != tier {
        return Err(format!(
            "filename tier {tier} does not match payload tier {}",
            file.tier
        ));
    }
    if u32::try_from(file.operations.len()).ok() != Some(op_count) {
        return Err(format!(
            "filename operation count {op_count} does not match payload count {}",
            file.operations.len()
        ));
    }
    let capacity = 20_u64.checked_mul(10_u64.checked_pow(u32::from(tier - 1)).unwrap_or(u64::MAX));
    if capacity.is_some_and(|limit| u64::from(op_count) > limit) {
        return Err(format!(
            "operation count {op_count} exceeds tier {tier} capacity"
        ));
    }
    let mut dots = HashSet::new();
    if file
        .operations
        .iter()
        .any(|operation| !dots.insert(operation.dot))
    {
        return Err("payload contains duplicate operation dots".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CompactionFile, validate_compaction_file};

    #[test]
    fn compaction_metadata_must_match_payload() {
        let file = CompactionFile {
            tier: 1,
            operations: Vec::new(),
        };

        assert!(validate_compaction_file(&file, 1, 0).is_ok());
        assert!(
            validate_compaction_file(&file, 2, 0)
                .unwrap_err()
                .contains("tier")
        );
        assert!(
            validate_compaction_file(&file, 1, 1)
                .unwrap_err()
                .contains("count")
        );
    }
}
