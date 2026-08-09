use rustc_hash::FxHashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::identifiers::MediaUuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobStatus {
    Cached,
    /// Present at a remote but not downloaded locally
    OnRemote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaListEntry {
    pub full: BlobStatus,
    pub thumb: BlobStatus,
}

/// Positive-only inventory of media blobs confirmed present at a known remote
/// (`remotes/{remote_id}/state/media/media_list.json`). It is intentionally allowed to be
/// incomplete: absence means unconfirmed, not absent from the remote.
///
/// All entries use `BlobStatus::OnRemote` (via `insert_present`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaList {
    pub media: FxHashMap<MediaUuid, MediaListEntry>,
}

impl MediaList {
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
        let data = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::atomic_file::write(path, &data)
    }

    pub fn contains(&self, media_id: &MediaUuid) -> bool {
        self.media.contains_key(media_id)
    }

    /// Inserts a `Cached` entry for `media_id`. Returns `true` if newly inserted.
    pub fn insert_present(&mut self, media_id: MediaUuid) -> bool {
        if self.media.contains_key(&media_id) {
            return false;
        }
        self.media.insert(
            media_id,
            MediaListEntry {
                full: BlobStatus::OnRemote,
                thumb: BlobStatus::OnRemote,
            },
        );
        true
    }
}
