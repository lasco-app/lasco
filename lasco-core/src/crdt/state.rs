//! Materialized, incrementally merged library CRDT state.
//!
//! This module deliberately has no storage or transport policy.  It is the
//! representation persisted by the next storage format and the single merge
//! implementation used by local and remote operations.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use rand::Rng as _;
use serde::{Deserialize, Serialize};

use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::library::media::MediaHash;
use crate::operations::{
    AlbumName, GpsCoords, LibraryUsername, MediaFilename, MediaName, StorageDate,
};
use crate::state::{ComputedViews, build_computed_views};

#[derive(Clone, Debug, PartialEq)]
pub struct MediaEntry {
    pub media_id: MediaUuid,
    pub filename_original: MediaFilename,
    pub name: Option<MediaName>,
    pub date: DateTime<Utc>,
    pub storage_date: StorageDate,
    pub size_bytes: u64,
    pub properties: rustc_hash::FxHashMap<String, String>,
    pub content_hash: MediaHash,
    pub author: LibraryUsername,
    pub modified_at: Option<DateTime<Utc>>,
    pub gps: Option<GpsCoords>,
    pub apple_aae_media_id: Option<MediaUuid>,
    pub apple_live_photo_media_id: Option<MediaUuid>,
    pub group_ids: Vec<GroupUuid>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumEntry {
    pub album_id: AlbumUuid,
    pub name: AlbumName,
    pub album_id_parent: Option<AlbumUuid>,
    pub media_ids: Vec<MediaUuid>,
    pub thumbnail_media_id: Option<MediaUuid>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupEntry {
    pub group_id: GroupUuid,
    pub album_id_parent: AlbumUuid,
    pub media_ids: Vec<MediaUuid>,
}

/// A device-stable random identifier. Generate it once and persist it with the
/// `CrdtState`; creating it per operation would defeat Lamport ordering.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DeviceId(pub u128);

impl DeviceId {
    #[must_use]
    pub fn random() -> Self {
        Self(rand::thread_rng().r#gen())
    }
}

/// The globally ordered identity of one immutable operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Dot {
    pub lamport_counter: u64,
    pub device_id: DeviceId,
}

/// Persisted Lamport timestamp. It advances for every observed remote dot before
/// the next locally-authored dot is allocated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LamportClock(u64);

impl LamportClock {
    pub fn observe(&mut self, dot: Dot) {
        self.0 = self.0.max(dot.lamport_counter);
    }

    /// # Panics
    ///
    /// Panics if it cannot advance past `u64::MAX`; this should never happen in practice.
    pub fn next_dot(&mut self, device_id: DeviceId) -> Dot {
        self.0 = self.0.checked_add(1).expect("Lamport clock exhausted");
        Dot {
            lamport_counter: self.0,
            device_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrdtOperation {
    pub dot: Dot,
    /// Declarative audit metadata. It has no role in conflict resolution.
    pub author: LibraryUsername,
    pub timestamp: DateTime<Utc>,
    pub content: OperationContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OperationContent {
    MediaCreation(MediaCreation),
    MediaRename {
        media_id: MediaUuid,
        name: Option<MediaName>,
    },
    MediaPropsUpdate {
        media_id: MediaUuid,
        key: String,
        value: String,
    },
    AlbumCreation {
        album_id: AlbumUuid,
        name: AlbumName,
        parent_id: Option<AlbumUuid>,
    },
    AlbumMediaAdd {
        album_id: AlbumUuid,
        media_id: MediaUuid,
    },
    AlbumMediaRemove {
        album_id: AlbumUuid,
        media_id: MediaUuid,
        observed: HashSet<Dot>,
    },
    AlbumDeletion {
        album_id: AlbumUuid,
    },
    AlbumRename {
        album_id: AlbumUuid,
        name: Option<AlbumName>,
    },
    AlbumReparent {
        album_id: AlbumUuid,
        parent_id: Option<AlbumUuid>,
    },
    AlbumThumbnailSet {
        album_id: AlbumUuid,
        media_id: Option<MediaUuid>,
    },
    GroupCreation {
        group_id: GroupUuid,
        parent_id: AlbumUuid,
    },
    GroupMediaAdd {
        group_id: GroupUuid,
        media_id: MediaUuid,
    },
    GroupMediaRemove {
        group_id: GroupUuid,
        media_id: MediaUuid,
        observed: HashSet<Dot>,
    },
    GroupDeletion {
        group_id: GroupUuid,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaCreation {
    pub media_id: MediaUuid,
    pub filename_original: MediaFilename,
    pub date: DateTime<Utc>,
    pub storage_date: StorageDate,
    pub size_bytes: u64,
    pub content_hash: MediaHash,
    pub modified_at: Option<DateTime<Utc>>,
    pub gps: Option<GpsCoords>,
    pub apple_aae_media_id: Option<MediaUuid>,
    pub apple_live_photo_media_id: Option<MediaUuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastWriteWin<T> {
    pub dot: Dot,
    pub value: T,
}

impl<T> LastWriteWin<T> {
    fn write(&mut self, dot: Dot, value: T) {
        if dot > self.dot {
            *self = Self { dot, value };
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedRemoveSet {
    pub adds: HashSet<Dot>,
    pub removed: HashSet<Dot>,
}

impl ObservedRemoveSet {
    #[must_use]
    pub fn contains(&self) -> bool {
        self.adds.iter().any(|dot| !self.removed.contains(dot))
    }

    #[must_use]
    pub fn live_dots(&self) -> HashSet<Dot> {
        self.adds.difference(&self.removed).copied().collect()
    }

    fn add(&mut self, dot: Dot) {
        self.adds.insert(dot);
    }

    fn remove(&mut self, observed: &HashSet<Dot>) {
        self.removed.extend(observed.iter().copied());
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrdtState {
    pub(super) device_id: DeviceId,
    pub(super) lamport_clock: LamportClock,
    pub(super) media: HashMap<MediaUuid, MediaCrdt>,
    pub(super) albums: HashMap<AlbumUuid, AlbumCrdt>,
    pub(super) groups: HashMap<GroupUuid, GroupCrdt>,
    pub(super) album_memberships: HashMap<(AlbumUuid, MediaUuid), ObservedRemoveSet>,
    pub(super) group_memberships: HashMap<(GroupUuid, MediaUuid), ObservedRemoveSet>,
    /// Derived, in-memory query indexes. This cache is never serialized.
    #[serde(skip)]
    pub(crate) views: ComputedViews,
}

pub(crate) struct ResolvedEntries {
    pub(crate) media: Vec<MediaEntry>,
    pub(crate) albums: Vec<AlbumEntry>,
    pub(crate) groups: Vec<GroupEntry>,
}

impl Default for CrdtState {
    fn default() -> Self {
        Self {
            device_id: DeviceId::random(),
            lamport_clock: LamportClock::default(),
            media: HashMap::new(),
            albums: HashMap::new(),
            groups: HashMap::new(),
            album_memberships: HashMap::new(),
            group_memberships: HashMap::new(),
            views: ComputedViews::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaCrdt {
    pub creation: Option<LastWriteWin<MediaCreation>>,
    pub author: Option<LastWriteWin<LibraryUsername>>,
    pub name: Option<LastWriteWin<Option<MediaName>>>,
    pub properties: HashMap<String, LastWriteWin<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumCrdt {
    pub creation: Option<LastWriteWin<AlbumCreation>>,
    pub name: Option<LastWriteWin<Option<AlbumName>>>,
    pub parent: Option<LastWriteWin<Option<AlbumUuid>>>,
    pub thumbnail: Option<LastWriteWin<Option<MediaUuid>>>,
    /// A deletion cannot be undone. Retaining its greatest dot makes the
    /// canonical representation deterministic without weakening permanence.
    pub tombstone: Option<Dot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumCreation {
    pub album_id: AlbumUuid,
    pub name: AlbumName,
    pub parent_id: Option<AlbumUuid>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCrdt {
    pub creation: Option<LastWriteWin<GroupCreation>>,
    pub tombstone: Option<Dot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCreation {
    pub group_id: GroupUuid,
    pub parent_id: AlbumUuid,
}

impl CrdtState {
    #[must_use]
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            ..Self::default()
        }
    }

    pub fn next_local_dot(&mut self) -> Dot {
        self.lamport_clock.next_dot(self.device_id)
    }

    /// Merges one operation. Each CRDT substate is idempotent: LWW registers retain
    /// their winning dot, observed-remove sets store add/tombstone dots, and entity
    /// tombstones retain their greatest dot. No global set of applied operations is kept.
    #[allow(
        clippy::too_many_lines,
        reason = "Keeping all CRDT operation mutations in one exhaustive match makes state transitions auditable."
    )]
    fn apply_raw(&mut self, operation: &CrdtOperation) {
        self.lamport_clock.observe(operation.dot);
        match &operation.content {
            OperationContent::MediaCreation(creation) => {
                let media = self.media.entry(creation.media_id).or_default();
                write_optional(&mut media.creation, operation.dot, creation.clone());
                write_optional(&mut media.author, operation.dot, operation.author.clone());
            }
            OperationContent::MediaRename { media_id, name } => {
                let media = self.media.entry(*media_id).or_default();
                write_optional(&mut media.name, operation.dot, name.clone());
            }
            OperationContent::MediaPropsUpdate {
                media_id,
                key,
                value,
            } => {
                let media = self.media.entry(*media_id).or_default();
                let register =
                    media
                        .properties
                        .entry(key.clone())
                        .or_insert_with(|| LastWriteWin {
                            dot: operation.dot,
                            value: value.clone(),
                        });
                register.write(operation.dot, value.clone());
            }
            OperationContent::AlbumCreation {
                album_id,
                name,
                parent_id,
            } => {
                let album = self.albums.entry(*album_id).or_default();
                let creation = AlbumCreation {
                    album_id: *album_id,
                    name: name.clone(),
                    parent_id: *parent_id,
                };
                write_optional(&mut album.creation, operation.dot, creation);
                write_optional(&mut album.name, operation.dot, Some(name.clone()));
                write_optional(&mut album.parent, operation.dot, *parent_id);
            }
            OperationContent::AlbumMediaAdd { album_id, media_id } => {
                self.album_memberships
                    .entry((*album_id, *media_id))
                    .or_default()
                    .add(operation.dot);
            }
            OperationContent::AlbumMediaRemove {
                album_id,
                media_id,
                observed,
            } => {
                self.album_memberships
                    .entry((*album_id, *media_id))
                    .or_default()
                    .remove(observed);
            }
            OperationContent::AlbumDeletion { album_id } => {
                let album = self.albums.entry(*album_id).or_default();
                album.tombstone = Some(
                    album
                        .tombstone
                        .map_or(operation.dot, |old| old.max(operation.dot)),
                );
            }
            OperationContent::AlbumRename { album_id, name } => {
                write_optional(
                    &mut self.albums.entry(*album_id).or_default().name,
                    operation.dot,
                    name.clone(),
                );
            }
            OperationContent::AlbumReparent {
                album_id,
                parent_id,
            } => {
                write_optional(
                    &mut self.albums.entry(*album_id).or_default().parent,
                    operation.dot,
                    *parent_id,
                );
            }
            OperationContent::AlbumThumbnailSet { album_id, media_id } => {
                write_optional(
                    &mut self.albums.entry(*album_id).or_default().thumbnail,
                    operation.dot,
                    *media_id,
                );
            }
            OperationContent::GroupCreation {
                group_id,
                parent_id,
            } => {
                let creation = GroupCreation {
                    group_id: *group_id,
                    parent_id: *parent_id,
                };
                write_optional(
                    &mut self.groups.entry(*group_id).or_default().creation,
                    operation.dot,
                    creation,
                );
            }
            OperationContent::GroupMediaAdd { group_id, media_id } => {
                self.group_memberships
                    .entry((*group_id, *media_id))
                    .or_default()
                    .add(operation.dot);
            }
            OperationContent::GroupMediaRemove {
                group_id,
                media_id,
                observed,
            } => {
                self.group_memberships
                    .entry((*group_id, *media_id))
                    .or_default()
                    .remove(observed);
            }
            OperationContent::GroupDeletion { group_id } => {
                let group = self.groups.entry(*group_id).or_default();
                group.tombstone = Some(
                    group
                        .tombstone
                        .map_or(operation.dot, |old| old.max(operation.dot)),
                );
            }
        }
    }

    pub fn apply(&mut self, operation: &CrdtOperation) {
        self.apply_raw(operation);
        self.rebuild_views();
    }

    pub fn rebuild_views(&mut self) {
        self.views = build_computed_views(self);
    }

    pub fn apply_batch<'a>(&mut self, operations: impl IntoIterator<Item = &'a CrdtOperation>) {
        for operation in operations {
            self.apply_raw(operation);
        }
        self.rebuild_views();
    }

    pub fn merge_all<'a>(&mut self, operations: impl IntoIterator<Item = &'a CrdtOperation>) {
        self.apply_batch(operations);
    }

    pub fn album_member_dots(&self, album_id: AlbumUuid, media_id: MediaUuid) -> HashSet<Dot> {
        self.album_memberships
            .get(&(album_id, media_id))
            .map_or_else(HashSet::new, ObservedRemoveSet::live_dots)
    }

    pub fn group_member_dots(&self, group_id: GroupUuid, media_id: MediaUuid) -> HashSet<Dot> {
        self.group_memberships
            .get(&(group_id, media_id))
            .map_or_else(HashSet::new, ObservedRemoveSet::live_dots)
    }

    #[must_use]
    pub fn is_album_created_and_live(&self, id: AlbumUuid) -> bool {
        self.albums
            .get(&id)
            .is_some_and(|album| album.creation.is_some() && album.tombstone.is_none())
    }

    /// Resolves parents, cycles, and visibility without mutating canonical data.
    ///
    /// # Panics
    ///
    /// Panics if the internally constructed cycle path is empty while resolving an album-parent cycle.
    #[must_use]
    pub fn album_projection(&self) -> AlbumProjection {
        let mut parents: HashMap<AlbumUuid, Option<AlbumUuid>> = self
            .albums
            .iter()
            .filter(|(id, _)| self.is_album_created_and_live(**id))
            .map(|(id, album)| {
                (
                    *id,
                    album.parent.as_ref().and_then(|register| register.value),
                )
            })
            .collect();

        // Invalid non-root parents hide a child. Keep the edge so visibility can
        // distinguish it from a genuine root.
        let candidate_ids: HashSet<_> = parents.keys().copied().collect();
        let cycle_candidates: Vec<_> = candidate_ids.iter().copied().collect();
        for start in cycle_candidates {
            let mut path = Vec::new();
            let mut seen = HashMap::new();
            let mut current = start;
            while let Some(Some(parent)) = parents.get(&current) {
                if !candidate_ids.contains(parent) {
                    break;
                }
                if let Some(index) = seen.get(parent) {
                    let sever = path[*index..]
                        .iter()
                        .chain(std::iter::once(&current))
                        .min_by_key(|id| {
                            self.albums[id]
                                .parent
                                .as_ref()
                                .expect("cycle has a parent register")
                                .dot
                        })
                        .copied()
                        .expect("cycle is nonempty");
                    parents.insert(sever, None);
                    break;
                }
                seen.insert(current, path.len());
                path.push(current);
                current = *parent;
            }
        }

        let mut visible = HashSet::new();
        for id in parents.keys().copied() {
            if is_visible(id, &parents, &mut visible, &mut HashSet::new()) {
                visible.insert(id);
            }
        }
        AlbumProjection {
            effective_parents: parents,
            visible,
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Resolving all query entries together guarantees one shared album projection."
    )]
    pub(crate) fn resolve_entries(&self) -> ResolvedEntries {
        let projection = self.album_projection();
        let mut media_entries = Vec::new();
        for media in self.media.values() {
            let Some(creation) = &media.creation else {
                continue;
            };
            let value = &creation.value;
            media_entries.push(MediaEntry {
                media_id: value.media_id,
                filename_original: value.filename_original.clone(),
                name: media
                    .name
                    .as_ref()
                    .and_then(|register| register.value.clone()),
                date: value.date,
                storage_date: value.storage_date,
                size_bytes: value.size_bytes,
                properties: media
                    .properties
                    .iter()
                    .map(|(key, register)| (key.clone(), register.value.clone()))
                    .collect(),
                content_hash: value.content_hash,
                author: media.author.as_ref().map_or_else(
                    || LibraryUsername("unknown".into()),
                    |register| register.value.clone(),
                ),
                modified_at: value.modified_at,
                gps: value.gps,
                apple_aae_media_id: value.apple_aae_media_id,
                apple_live_photo_media_id: value.apple_live_photo_media_id,
                group_ids: Vec::new(),
            });
        }
        let mut album_entries = Vec::new();
        for (album_id, album) in &self.albums {
            if !projection.visible.contains(album_id) {
                continue;
            }
            let Some(creation) = &album.creation else {
                continue;
            };
            let media_ids = self
                .album_memberships
                .iter()
                .filter_map(|((id, media_id), set)| {
                    (*id == *album_id && set.contains()).then_some(*media_id)
                })
                .collect();
            album_entries.push(AlbumEntry {
                album_id: *album_id,
                name: album
                    .name
                    .as_ref()
                    .and_then(|register| register.value.clone())
                    .unwrap_or_else(|| creation.value.name.clone()),
                album_id_parent: projection.effective_parents[album_id],
                media_ids,
                thumbnail_media_id: album.thumbnail.as_ref().and_then(|register| register.value),
            });
        }
        let mut group_entries = Vec::new();
        for (group_id, group) in &self.groups {
            let Some(creation) = &group.creation else {
                continue;
            };
            if group.tombstone.is_some() || !projection.visible.contains(&creation.value.parent_id)
            {
                continue;
            }
            let media_ids = self
                .group_memberships
                .iter()
                .filter_map(|((id, media_id), set)| {
                    (*id == *group_id && set.contains()).then_some(*media_id)
                })
                .collect();
            group_entries.push(GroupEntry {
                group_id: *group_id,
                album_id_parent: creation.value.parent_id,
                media_ids,
            });
        }
        let visible_groups: HashMap<_, _> = group_entries.iter().map(|g| (g.group_id, g)).collect();
        for media in &mut media_entries {
            media.group_ids = visible_groups
                .values()
                .filter(|group| group.media_ids.contains(&media.media_id))
                .map(|group| group.group_id)
                .collect();
            media.group_ids.sort_by_key(|id| id.0);
        }
        media_entries.sort_by_key(|entry| entry.media_id.0);
        album_entries.sort_by_key(|entry| entry.album_id.0);
        group_entries.sort_by_key(|entry| entry.group_id.0);
        for entry in &mut album_entries {
            entry.media_ids.sort_by_key(|id| id.0);
        }
        for entry in &mut group_entries {
            entry.media_ids.sort_by_key(|id| id.0);
        }
        ResolvedEntries {
            media: media_entries,
            albums: album_entries,
            groups: group_entries,
        }
    }

    #[must_use]
    pub fn media(&self, id: MediaUuid) -> Option<MediaEntry> {
        self.resolve_entries()
            .media
            .into_iter()
            .find(|e| e.media_id == id)
    }
    #[must_use]
    pub fn media_entries(&self) -> Vec<MediaEntry> {
        self.resolve_entries().media
    }
    #[must_use]
    pub fn album(&self, id: AlbumUuid) -> Option<AlbumEntry> {
        self.resolve_entries()
            .albums
            .into_iter()
            .find(|e| e.album_id == id)
    }
    #[must_use]
    pub fn album_entries(&self) -> Vec<AlbumEntry> {
        self.resolve_entries().albums
    }
    #[must_use]
    pub fn group(&self, id: GroupUuid) -> Option<GroupEntry> {
        self.resolve_entries()
            .groups
            .into_iter()
            .find(|e| e.group_id == id)
    }
    #[must_use]
    pub fn group_entries(&self) -> Vec<GroupEntry> {
        self.resolve_entries().groups
    }
}

fn write_optional<T>(register: &mut Option<LastWriteWin<T>>, dot: Dot, value: T) {
    match register {
        Some(existing) => existing.write(dot, value),
        None => *register = Some(LastWriteWin { dot, value }),
    }
}

fn is_visible(
    id: AlbumUuid,
    parents: &HashMap<AlbumUuid, Option<AlbumUuid>>,
    resolved: &mut HashSet<AlbumUuid>,
    visiting: &mut HashSet<AlbumUuid>,
) -> bool {
    if resolved.contains(&id) {
        return true;
    }
    if !visiting.insert(id) {
        return false;
    }
    let visible = match parents.get(&id) {
        Some(None) => true,
        Some(Some(parent)) => {
            parents.contains_key(parent) && is_visible(*parent, parents, resolved, visiting)
        }
        None => false,
    };
    visiting.remove(&id);
    if visible {
        resolved.insert(id);
    }
    visible
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AlbumProjection {
    pub effective_parents: HashMap<AlbumUuid, Option<AlbumUuid>>,
    pub visible: HashSet<AlbumUuid>,
}
