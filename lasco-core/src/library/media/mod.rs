pub mod hash;
pub mod query;
pub mod upload;

pub use hash::MediaHash;

use rustc_hash::FxHashMap;

use chrono::{DateTime, Utc};

use crate::crdt::CompanionKind;
use crate::identifiers::{GroupUuid, MediaUuid};
use crate::operations::{GpsCoords, MediaFilename, MediaName, StorageDate};

/// Public-facing media entry returned by `media_list` and `media_show`.
/// `group_ids` is populated from `ComputedViews::media_group_membership`.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaEntry {
    pub media_id: MediaUuid,
    pub filename_original: MediaFilename,
    pub name: Option<MediaName>,
    pub date: DateTime<Utc>,
    pub storage_date: StorageDate,
    pub size_bytes: u64,
    pub properties: FxHashMap<String, String>,
    pub group_ids: Vec<GroupUuid>,
    pub content_hash: MediaHash,
    pub author: String,
    pub modified_at: Option<DateTime<Utc>>,
    pub gps: Option<GpsCoords>,
    pub apple_aae_media_id: Option<MediaUuid>,
    pub apple_live_photo_media_id: Option<MediaUuid>,
    /// Set when another media references this one as its companion resource. A companion is
    /// never browsed on its own and never has a thumbnail.
    pub companion_kind: Option<CompanionKind>,
}

impl MediaEntry {
    pub(crate) fn from_state(entry: &crate::crdt::MediaEntry, group_ids: Vec<GroupUuid>) -> Self {
        Self {
            media_id: entry.media_id,
            filename_original: entry.filename_original.clone(),
            name: entry.name.clone(),
            date: entry.date,
            storage_date: entry.storage_date,
            size_bytes: entry.size_bytes,
            properties: entry.properties.clone(),
            group_ids,
            content_hash: entry.content_hash,
            author: entry.author.0.clone(),
            modified_at: entry.modified_at,
            gps: entry.gps,
            apple_aae_media_id: entry.apple_aae_media_id,
            apple_live_photo_media_id: entry.apple_live_photo_media_id,
            companion_kind: entry.companion_kind,
        }
    }
}
