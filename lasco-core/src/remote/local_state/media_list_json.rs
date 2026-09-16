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

/// The two blobs of one media are confirmed independently. `None` means unconfirmed, so an
/// entry written before a media had a thumbnail does not claim that thumbnail is on the remote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaListEntry {
    #[serde(default)]
    pub full: Option<BlobStatus>,
    #[serde(default)]
    pub thumb: Option<BlobStatus>,
}

/// Cache of media blobs observed on a known remote
/// (`remotes/{remote_id}/state/media/media_list.json`). A complete recursive scan replaces this
/// cache atomically; incremental fetch and push observations only add positive entries between
/// scans. Therefore absence means "not observed since the last successful complete scan" unless
/// the caller knows it has just completed one.
///
/// All confirmed blobs use `BlobStatus::OnRemote` (via `record`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Whether the full media file is confirmed on the remote.
    pub fn has_full(&self, media_id: &MediaUuid) -> bool {
        self.media
            .get(media_id)
            .is_some_and(|entry| entry.full.is_some())
    }

    /// Whether the thumbnail is confirmed on the remote.
    pub fn has_thumb(&self, media_id: &MediaUuid) -> bool {
        self.media
            .get(media_id)
            .is_some_and(|entry| entry.thumb.is_some())
    }

    /// Confirms the blobs passed as `true`. A confirmation is never withdrawn, so passing
    /// `false` leaves that blob as it was. Returns `true` if the inventory changed.
    pub fn record(&mut self, media_id: MediaUuid, full: bool, thumb: bool) -> bool {
        let entry = self.media.entry(media_id).or_default();
        let mut changed = false;
        if full && entry.full.is_none() {
            entry.full = Some(BlobStatus::OnRemote);
            changed = true;
        }
        if thumb && entry.thumb.is_none() {
            entry.thumb = Some(BlobStatus::OnRemote);
            changed = true;
        }
        changed
    }

    /// Counts observed blobs that were not present in `previous`, preserving the return contract
    /// of a fresh remote scan even when that scan also removes stale cache entries.
    #[must_use]
    pub fn newly_confirmed_since(&self, previous: &Self) -> usize {
        self.media
            .iter()
            .map(|(media_id, entry)| {
                let old = previous.media.get(media_id);
                usize::from(entry.full.is_some() && old.is_none_or(|old| old.full.is_none()))
                    + usize::from(
                        entry.thumb.is_some() && old.is_none_or(|old| old.thumb.is_none()),
                    )
            })
            .sum()
    }

    /// Removes an observation after this client has reclaimed the corresponding
    /// remote blobs. Unlike `record`, this is used only for a permanent CRDT
    /// tombstone, never to infer absence from a failed probe.
    pub fn forget(&mut self, media_id: &MediaUuid) -> bool {
        self.media.remove(media_id).is_some()
    }
}
