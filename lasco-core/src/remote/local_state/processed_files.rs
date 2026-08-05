use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::identifiers::CompactedOpId;

/// Persisted set of remote operation-file UUIDs that have been fully ingested into the local log.
///
/// Stored at `remotes/{remote_id}/state/processed.json`. Consulted during fetch to skip files that
/// were already processed in a previous run (raw `.op` or compaction `.opN`). For raw files the
/// UUID equals the op_id. For compaction files it is the fresh UUID embedded in the filename.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessedFiles {
    pub file_uuids: HashSet<CompactedOpId>,
}

impl ProcessedFiles {
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
