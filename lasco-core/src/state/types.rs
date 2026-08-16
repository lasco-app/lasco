use std::collections::{BTreeMap, HashSet};

use rustc_hash::FxHashMap;

use chrono::{DateTime, Utc};

use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::library::media::MediaHash;

/// Always derived from the canonical CRDT state, rebuilt on every state change.
#[derive(Clone, Debug, Default, PartialEq)]
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
