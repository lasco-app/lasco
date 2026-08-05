use std::collections::HashSet;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob::decrypt_blob;
use crate::encryption::blob_key::derive_blob_key;
use crate::encryption::master_key::MasterKey;
use crate::error::SyncError;
use crate::identifiers::{CompactedOpId, OpUuid};
use crate::library::local_dirs::LocalDirs;
use crate::library::sync::remote_access::StorageRead;
use crate::operations::remote_ops::{RemoteOpFile, list_remote_op_files_read};
use crate::operations::{CompactionFile, compaction_file_from_cbor, encrypt_compaction_file};
use crate::remote::local_state::processed_files::ProcessedFiles;

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
    /// Phase 1 of fetch. Lists remote op files and downloads any not yet cached in the
    /// last known state dir. Files already present on disk or whose UUID appears in `processed` are
    /// skipped, making this idempotent.
    pub(crate) async fn download(
        storage: &StorageRead<'_>,
        local_dirs: &LocalDirs,
        remote_id: &str,
        processed: &ProcessedFiles,
        master_key: &MasterKey,
    ) -> Result<Self, SyncError> {
        let ops_dir = local_dirs.remote_ops_dir(remote_id);
        std::fs::create_dir_all(&ops_dir)?;

        let remote_files = list_remote_op_files_read(storage).await?;

        for file in &remote_files {
            if processed.contains(&Self::file_uuid(file)) {
                continue;
            }
            let (remote_key, local_name) = Self::file_paths(file);
            let local_path = ops_dir.join(&local_name);
            if !local_path.exists() {
                let bytes = storage
                    .get(&remote_key)
                    .await
                    .map_err(SyncError::RemoteUnreachable)?;
                std::fs::write(&local_path, &bytes)?;
            }
        }

        // Verify no op groups have been lost. Collect group IDs from every file on disk
        // (including stale files no longer on the remote) and from every current remote file.
        // If the remote no longer covers a previously cached group, history was rewritten.
        let cached_group_ids: HashSet<OpUuid> = collect_group_ids_from_dir(&ops_dir, master_key)
            .map_err(SyncError::LocalCacheCorrupt)?;
        if !cached_group_ids.is_empty() {
            let mut remote_group_ids: HashSet<OpUuid> = HashSet::new();
            for f in &remote_files {
                let (_, local_name) = Self::file_paths(f);
                let ids = read_op_ids_from_file(
                    &ops_dir.join(local_name),
                    master_key,
                    &Self::file_uuid(f),
                )
                .map_err(SyncError::LocalCacheCorrupt)?;
                remote_group_ids.extend(ids);
            }
            if !remote_group_ids.is_superset(&cached_group_ids) {
                let mut missing: Vec<OpUuid> = cached_group_ids
                    .difference(&remote_group_ids)
                    .copied()
                    .collect();
                missing.sort();
                return Err(SyncError::RemoteHistoryRewritten(format!(
                    "cached_groups={} remote_groups={} missing={missing:?}",
                    cached_group_ids.len(),
                    remote_group_ids.len(),
                )));
            }
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
    pub(crate) fn open(local_dirs: &LocalDirs, remote_id: &str) -> Result<Self, SyncError> {
        let ops_dir = local_dirs.remote_ops_dir(remote_id);
        let files = Self::list_cached_files(local_dirs, remote_id)?;
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
        std::fs::write(
            self.ops_dir.join(format!("{uuid}.op{tier}_{op_count}")),
            bytes,
        )?;
        Ok(())
    }

    /// Lists op files recorded in the on-disk last known state for `remote_id`, without
    /// contacting the remote. A missing cache directory is treated as empty.
    pub(crate) fn list_cached_files(
        local_dirs: &LocalDirs,
        remote_id: &str,
    ) -> Result<Vec<RemoteOpFile>, SyncError> {
        let ops_dir = local_dirs.remote_ops_dir(remote_id);
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
fn read_op_ids_from_file(
    path: &Path,
    master_key: &MasterKey,
    file_uuid: &CompactedOpId,
) -> Result<Vec<OpUuid>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };
    let blob = BlobEncrypted::from_bytes(&bytes)
        .map_err(|e| format!("failed to parse blob {}: {e}", path.display()))?;
    let file_key = derive_blob_key(master_key, &file_uuid.0);
    let plaintext = decrypt_blob(&file_key, &blob)
        .map_err(|e| format!("failed to decrypt {}: {e}", path.display()))?;
    let file = compaction_file_from_cbor(&plaintext)
        .map_err(|e| format!("failed to parse compaction file {}: {e}", path.display()))?;
    Ok(file.contents.into_iter().map(|e| e.op_id).collect())
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

pub(crate) fn collect_group_ids_from_dir(
    dir: &Path,
    master_key: &MasterKey,
) -> Result<HashSet<OpUuid>, String> {
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
        ids.extend(read_op_ids_from_file(&entry.path(), master_key, &uuid)?);
    }
    Ok(ids)
}
