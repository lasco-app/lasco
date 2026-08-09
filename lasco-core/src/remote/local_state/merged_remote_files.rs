use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::identifiers::CompactedOpId;

/// Persisted set of immutable remote operation files already merged into the local log.
///
/// Stored at `remotes/{remote_id}/state/merged_remote_files.json`. This is merge progress,
/// not the last-known remote-operation cache: a file may be merged while its cached ciphertext
/// still needs to be restored.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MergedRemoteFiles {
    pub file_uuids: HashSet<CompactedOpId>,
}

impl MergedRemoteFiles {
    pub fn load_or_default(path: &Path) -> std::io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read(path)?;
        serde_json::from_slice(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_name = format!(
            "{}.tmp",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let tmp = path.with_file_name(tmp_name);
        let data = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, path)
    }

    pub fn contains(&self, uuid: &CompactedOpId) -> bool {
        self.file_uuids.contains(uuid)
    }

    pub fn insert(&mut self, uuid: CompactedOpId) {
        self.file_uuids.insert(uuid);
    }
}
