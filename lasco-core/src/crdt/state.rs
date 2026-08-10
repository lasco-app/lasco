//! Canonical, incrementally merged library CRDT state.
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

/// A device-stable random identifier. Generate it once and persist it with the
/// canonical state; creating it per operation would defeat Lamport ordering.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DeviceId(pub u128);

impl DeviceId {
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

/// Counter state owned by a replica. It is persisted alongside canonical CRDT
/// state and advanced for every observed remote dot before the next local dot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaClock {
    pub device_id: DeviceId,
    pub counter: u64,
}

impl ReplicaClock {
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            counter: 0,
        }
    }

    pub fn observe(&mut self, dot: Dot) {
        self.counter = self.counter.max(dot.lamport_counter);
    }

    pub fn next_dot(&mut self) -> Dot {
        self.counter = self
            .counter
            .checked_add(1)
            .expect("Lamport counter exhausted");
        Dot {
            lamport_counter: self.counter,
            device_id: self.device_id,
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
    pub fn contains(&self) -> bool {
        self.adds.iter().any(|dot| !self.removed.contains(dot))
    }

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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CanonicalState {
    /// All seen operation identities, used for idempotence and sync.
    pub causal_context: HashSet<Dot>,
    pub clock: ReplicaClock,
    pub media: HashMap<MediaUuid, MediaCrdt>,
    pub albums: HashMap<AlbumUuid, AlbumCrdt>,
    pub groups: HashMap<GroupUuid, GroupCrdt>,
    pub album_memberships: HashMap<(AlbumUuid, MediaUuid), ObservedRemoveSet>,
    pub group_memberships: HashMap<(GroupUuid, MediaUuid), ObservedRemoveSet>,
}

impl Default for ReplicaClock {
    fn default() -> Self {
        Self::new(DeviceId::random())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaCrdt {
    pub creation: Option<LastWriteWin<MediaCreation>>,
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

impl CanonicalState {
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            clock: ReplicaClock::new(device_id),
            ..Self::default()
        }
    }

    pub fn next_local_dot(&mut self) -> Dot {
        self.clock.next_dot()
    }

    /// Merges one operation. Duplicate dots are intentionally no-ops.
    pub fn apply(&mut self, operation: &CrdtOperation) {
        self.clock.observe(operation.dot);
        if !self.causal_context.insert(operation.dot) {
            return;
        }

        match &operation.content {
            OperationContent::MediaCreation(creation) => {
                let media = self.media.entry(creation.media_id).or_default();
                write_optional(&mut media.creation, operation.dot, creation.clone());
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

    pub fn merge_all<'a>(&mut self, operations: impl IntoIterator<Item = &'a CrdtOperation>) {
        for operation in operations {
            self.apply(operation);
        }
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

    pub fn is_album_created_and_live(&self, id: AlbumUuid) -> bool {
        self.albums
            .get(&id)
            .is_some_and(|album| album.creation.is_some() && album.tombstone.is_none())
    }

    /// Resolves parents, cycles, and visibility without mutating canonical data.
    pub fn album_projection(&self) -> AlbumProjection {
        let mut parents: HashMap<AlbumUuid, Option<AlbumUuid>> = self
            .albums
            .iter()
            .filter(|(id, _)| self.is_album_created_and_live(**id))
            .map(|(id, album)| {
                (
                    *id,
                    album
                        .parent
                        .as_ref()
                        .map(|register| register.value)
                        .unwrap_or(None),
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
