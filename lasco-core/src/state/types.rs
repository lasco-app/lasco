use std::collections::{BTreeMap, HashSet};

use rustc_hash::FxHashMap;

use chrono::{DateTime, Utc};

use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::library::media::MediaHash;
use crate::operations::{
    AlbumName, GpsCoords, LibraryUsername, MediaFilename, MediaName, StorageDate,
};

#[derive(Clone, Debug, PartialEq)]
pub struct MediaEntry {
    pub media_id: MediaUuid,
    pub filename_original: MediaFilename,
    pub name: Option<MediaName>,
    pub date: DateTime<Utc>,
    pub storage_date: StorageDate,
    pub size_bytes: u64,
    pub properties: FxHashMap<String, String>,
    pub content_hash: MediaHash,
    pub author: LibraryUsername,
    pub modified_at: Option<DateTime<Utc>>,
    pub gps: Option<GpsCoords>,
    pub apple_aae_media_id: Option<MediaUuid>,
    pub apple_live_photo_media_id: Option<MediaUuid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumEntry {
    pub album_id: AlbumUuid,
    pub name: AlbumName,
    pub album_id_parent: Option<AlbumUuid>,
    pub media_ids: Vec<MediaUuid>,
    pub deleted: bool,
    pub thumbnail_media_id: Option<MediaUuid>,
}

/// Groups are always leaves
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupEntry {
    pub group_id: GroupUuid,
    pub album_id_parent: AlbumUuid,
    pub media_ids: Vec<MediaUuid>,
    pub deleted: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ReconstructedState {
    pub media: FxHashMap<MediaUuid, MediaEntry>,
    pub albums: FxHashMap<AlbumUuid, AlbumEntry>,
    pub groups: FxHashMap<GroupUuid, GroupEntry>,
}

/// Always derived from `ReconstructedState`, rebuilt on every state change.
#[derive(Clone, Debug, Default)]
pub struct ComputedViews {
    pub reachable_media_ids: HashSet<MediaUuid>,
    pub by_date: BTreeMap<DateTime<Utc>, Vec<MediaUuid>>,
    /// Primary (non-companion) media in canonical home browse order: newest first.
    pub home_visible_newest: Vec<MediaUuid>,
    /// Primary media with no live album or group membership, newest first.
    pub home_orphaned_newest: Vec<MediaUuid>,
    pub by_album: FxHashMap<AlbumUuid, Vec<MediaUuid>>,
    pub album_children: FxHashMap<Option<AlbumUuid>, Vec<AlbumUuid>>,
    /// Non-deleted direct child albums, ordered by name then ID. `None` is root.
    pub album_albums_by_name: FxHashMap<Option<AlbumUuid>, Vec<AlbumUuid>>,
    /// Media and groups in each non-deleted album, ordered newest first.
    pub album_items_newest: FxHashMap<AlbumUuid, Vec<AlbumBrowseItem>>,
    pub by_group: FxHashMap<GroupUuid, Vec<MediaUuid>>,
    pub groups_by_album: FxHashMap<AlbumUuid, Vec<GroupUuid>>,
    pub media_group_membership: FxHashMap<MediaUuid, Vec<GroupUuid>>,
    /// Maps each content hash to all media IDs with that hash. More than one ID means a concurrent duplicate import.
    pub by_content_hash: FxHashMap<MediaHash, Vec<MediaUuid>>,
}

/// A compact discriminator used by album browse views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlbumBrowseItem {
    Media(MediaUuid),
    Group(GroupUuid),
}
