use std::collections::{BTreeSet, HashMap};

use rustc_hash::FxHashMap;

use crate::identifiers::OpUuid;
use crate::operations::{Operation, OperationGroup};

use super::types::{AlbumEntry, GroupEntry, MediaEntry, ReconstructedState};

/// Topological sort of op groups following `parent_op_id` causal links.
/// When multiple groups are ready (same parent), tie-breaks by `op_id` ascending.
/// UUIDv7 embeds a timestamp so this gives "earliest clock wins" on concurrent forks.
fn causal_sort(groups: &[OperationGroup]) -> Vec<&OperationGroup> {
    let by_id: HashMap<OpUuid, &OperationGroup> = groups.iter().map(|g| (g.op_id, g)).collect();

    // children_of[parent] = sorted set of child op_ids
    let mut children_of: HashMap<Option<OpUuid>, BTreeSet<OpUuid>> = HashMap::new();
    for g in groups {
        children_of
            .entry(g.parent_op_id)
            .or_default()
            .insert(g.op_id);
    }

    let mut result = Vec::with_capacity(groups.len());
    // Start from roots (parent_op_id = None), ordered by op_id for determinism.
    let mut ready: BTreeSet<OpUuid> = children_of.remove(&None).unwrap_or_default();

    while let Some(id) = ready.pop_first() {
        let Some(group) = by_id.get(&id) else {
            continue;
        };
        result.push(*group);
        if let Some(children) = children_of.remove(&Some(id)) {
            ready.extend(children);
        }
    }

    result
}

pub fn reconstruct_state(op_groups: &[OperationGroup]) -> ReconstructedState {
    let mut state = ReconstructedState::default();

    for group in causal_sort(op_groups) {
        for op in &group.operations {
            match op {
                Operation::MediaCreation {
                    media_id,
                    filename_original,
                    date,
                    storage_date,
                    size_bytes,
                    content_hash,
                    modified_at,
                    gps,
                    apple_aae_media_id,
                    apple_live_photo_media_id,
                    ..
                } => {
                    state.media.insert(
                        *media_id,
                        MediaEntry {
                            media_id: *media_id,
                            filename_original: filename_original.clone(),
                            name: None,
                            date: *date,
                            storage_date: *storage_date,
                            size_bytes: *size_bytes,
                            properties: FxHashMap::default(),
                            content_hash: *content_hash,
                            author: group.author.clone(),
                            modified_at: *modified_at,
                            gps: *gps,
                            apple_aae_media_id: *apple_aae_media_id,
                            apple_live_photo_media_id: *apple_live_photo_media_id,
                        },
                    );
                }
                Operation::MediaRename { media_id, name, .. } => {
                    if let Some(media) = state.media.get_mut(media_id) {
                        media.name = name.clone();
                    }
                }
                Operation::MediaPropsUpdate {
                    media_id,
                    key,
                    value,
                    ..
                } => {
                    if let Some(media) = state.media.get_mut(media_id) {
                        media.properties.insert(key.clone(), value.clone());
                    }
                }
                Operation::AlbumCreation {
                    album_id,
                    name,
                    album_id_parent,
                    ..
                } => {
                    state.albums.insert(
                        *album_id,
                        AlbumEntry {
                            album_id: *album_id,
                            name: name.clone(),
                            album_id_parent: *album_id_parent,
                            media_ids: Vec::new(),
                            deleted: false,
                            thumbnail_media_id: None,
                        },
                    );
                }
                Operation::AlbumMediaAdd {
                    album_id, media_id, ..
                } => {
                    if let Some(album) = state.albums.get_mut(album_id) {
                        if !album.media_ids.contains(media_id) {
                            album.media_ids.push(*media_id);
                        }
                    }
                }
                Operation::AlbumMediaRemove {
                    album_id, media_id, ..
                } => {
                    if let Some(album) = state.albums.get_mut(album_id) {
                        album.media_ids.retain(|id| id != media_id);
                    }
                }
                Operation::AlbumDeletion { album_id, .. } => {
                    if let Some(album) = state.albums.get_mut(album_id) {
                        album.deleted = true;
                    }
                }
                Operation::AlbumRename { album_id, name, .. } => {
                    if let Some(album) = state.albums.get_mut(album_id) {
                        album.name = name.clone();
                    }
                }
                Operation::AlbumReparent {
                    album_id,
                    new_parent_id,
                    ..
                } => {
                    if let Some(album) = state.albums.get_mut(album_id) {
                        album.album_id_parent = *new_parent_id;
                    }
                }
                Operation::AlbumThumbnailSet {
                    album_id, media_id, ..
                } => {
                    if let Some(album) = state.albums.get_mut(album_id) {
                        album.thumbnail_media_id = *media_id;
                    }
                }
                Operation::GroupCreation {
                    group_id,
                    album_id_parent,
                    ..
                } => {
                    state.groups.insert(
                        *group_id,
                        GroupEntry {
                            group_id: *group_id,
                            album_id_parent: *album_id_parent,
                            media_ids: Vec::new(),
                            deleted: false,
                        },
                    );
                }
                Operation::GroupMediaAdd {
                    group_id, media_id, ..
                } => {
                    if let Some(group) = state.groups.get_mut(group_id) {
                        if !group.media_ids.contains(media_id) {
                            group.media_ids.push(*media_id);
                        }
                    }
                }
                Operation::GroupMediaRemove {
                    group_id, media_id, ..
                } => {
                    if let Some(group) = state.groups.get_mut(group_id) {
                        group.media_ids.retain(|id| id != media_id);
                    }
                }
                Operation::GroupDeletion { group_id, .. } => {
                    if let Some(group) = state.groups.get_mut(group_id) {
                        group.deleted = true;
                    }
                }
            }
        }
    }

    state
}
