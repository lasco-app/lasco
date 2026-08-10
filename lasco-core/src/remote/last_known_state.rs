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
#[cfg(test)]
use crate::operations::encrypt_compaction_file;
use crate::operations::remote_ops::{RemoteOpFile, list_remote_op_files_read};
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

        let remote_files = list_remote_op_files_read(storage).await?;
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
                let dots = read_dots_from_bytes(&bytes, master_key, &Self::file_uuid(file))
                    .map_err(SyncError::LocalCacheCorrupt)?;
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
                    let ids = read_dots_from_file(
                        &ops_dir.join(local_name),
                        master_key,
                        &Self::file_uuid(f),
                    )
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
        compaction_file_from_cbor(&plaintext).map_err(|e| {
            SyncError::LocalCacheCorrupt(format!(
                "failed to parse compaction file {}: {e}",
                path.display()
            ))
        })
    }

    /// Encrypts and writes `file` into the on-disk last known state, without contacting the
    /// remote. Used by push after it has itself uploaded or merged a file, so the cache stays
    /// accurate without downloading back what was just written.
    #[cfg(test)]
    pub(crate) fn write_compaction_file(
        &self,
        master_key: &MasterKey,
        uuid: &CompactedOpId,
        tier: u8,
        op_count: u32,
        file: &CompactionFile,
    ) -> Result<(), SyncError> {
        let blob = encrypt_compaction_file(master_key, uuid, file)
            .map_err(|e| SyncError::LocalCacheCorrupt(e.to_string()))?;
        self.write_compaction_bytes(uuid, tier, op_count, &blob.to_bytes())
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
    file_uuid: &CompactedOpId,
) -> Result<Vec<Dot>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };
    read_dots_from_bytes(&bytes, master_key, file_uuid)
        .map_err(|error| format!("{} ({})", error, path.display()))
}

fn read_dots_from_bytes(
    bytes: &[u8],
    master_key: &MasterKey,
    file_uuid: &CompactedOpId,
) -> Result<Vec<Dot>, String> {
    let blob =
        BlobEncrypted::from_bytes(bytes).map_err(|e| format!("failed to parse blob: {e}"))?;
    let file_key = derive_blob_key(master_key, &file_uuid.0);
    let plaintext =
        decrypt_blob(&file_key, &blob).map_err(|e| format!("failed to decrypt: {e}"))?;
    let file = compaction_file_from_cbor(&plaintext)
        .map_err(|e| format!("failed to parse compaction file: {e}"))?;
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
        let Some(uuid) = s.split('.').next().and_then(|s| s.parse::<Uuid>().ok()) else {
            continue;
        };
        let uuid = CompactedOpId::from_uuid(uuid);
        ids.extend(read_dots_from_file(&entry.path(), master_key, &uuid)?);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::encryption::master_key::generate_master_key;
    use crate::identifiers::LibraryId;
    use crate::library::local_dirs::LocalDirs;
    use crate::operations::remote_ops::write_compaction_file;
    use crate::operations::{CompactionEntry, LibraryUsername, OperationGroup};
    use crate::storage::{Storage, StorageMockMemory};

    use super::*;

    fn compaction(op_id: OpUuid) -> CompactionFile {
        CompactionFile {
            tier: 1,
            contents: vec![CompactionEntry {
                op_id,
                group: OperationGroup {
                    op_id,
                    parent_op_id: None,
                    author: LibraryUsername("test".into()),
                    operations: Vec::new(),
                },
            }],
        }
    }

    #[tokio::test]
    async fn rewritten_history_does_not_commit_staged_files() {
        let storage = StorageMockMemory::new();
        let temp = TempDir::new().unwrap();
        let master_key = generate_master_key();
        let local_dirs =
            LocalDirs::new(temp.path().to_path_buf(), &LibraryId(uuid::Uuid::new_v4()));
        let remote_dir = local_dirs.remote_last_known_state_dir("remote-a");

        let old_uuid = CompactedOpId::new();
        let old_key = format!("operations/{old_uuid}.op1_0");
        write_compaction_file(
            &storage,
            &master_key,
            &old_key,
            &old_uuid,
            &compaction(OpUuid::new()),
        )
        .await
        .unwrap();
        LastKnownState::download(&StorageRead::new(&storage), &remote_dir, &master_key)
            .await
            .unwrap();

        storage.delete(&old_key).await.unwrap();
        let new_uuid = CompactedOpId::new();
        let new_key = format!("operations/{new_uuid}.op1_0");
        write_compaction_file(
            &storage,
            &master_key,
            &new_key,
            &new_uuid,
            &compaction(OpUuid::new()),
        )
        .await
        .unwrap();

        let error =
            match LastKnownState::download(&StorageRead::new(&storage), &remote_dir, &master_key)
                .await
            {
                Ok(_) => panic!("rewritten remote history must fail"),
                Err(error) => error,
            };
        assert!(matches!(error, SyncError::RemoteHistoryRewritten(_)));
        assert!(
            !remote_dir
                .operations_dir()
                .join(format!("{new_uuid}.op1_0"))
                .exists(),
            "a failed history validation must not commit staged files"
        );
    }

    #[tokio::test]
    async fn missing_cached_file_is_restored_from_remote() {
        let storage = StorageMockMemory::new();
        let temp = TempDir::new().unwrap();
        let master_key = generate_master_key();
        let local_dirs =
            LocalDirs::new(temp.path().to_path_buf(), &LibraryId(uuid::Uuid::new_v4()));
        let remote_dir = local_dirs.remote_last_known_state_dir("remote-a");
        let uuid = CompactedOpId::new();
        let key = format!("operations/{uuid}.op1_0");
        write_compaction_file(
            &storage,
            &master_key,
            &key,
            &uuid,
            &compaction(OpUuid::new()),
        )
        .await
        .unwrap();

        LastKnownState::download(&StorageRead::new(&storage), &remote_dir, &master_key)
            .await
            .unwrap();
        let cached_path = remote_dir.operations_dir().join(format!("{uuid}.op1_0"));
        std::fs::remove_file(&cached_path).unwrap();

        LastKnownState::download(&StorageRead::new(&storage), &remote_dir, &master_key)
            .await
            .unwrap();
        assert!(cached_path.exists());
    }
}
