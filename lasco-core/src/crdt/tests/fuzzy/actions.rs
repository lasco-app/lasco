use rand::{
    seq::{IteratorRandom as _, SliceRandom as _},
    Rng as _,
};

use crate::crdt::{CrdtOperation, MediaCreation, OperationContent};
use crate::library::media::MediaHash;
use crate::operations::{LibraryUsername, StorageDate};

use super::{simulator::Replica, values::Values};

const DEFAULT_WEIGHT: usize = 1;

pub(super) struct OperationWeights {
    media_creation: usize,
    album_creation: usize,
    album_deletion: usize,
    group_creation: usize,
    group_deletion: usize,
    album_media_add: usize,
    album_media_remove: usize,
    group_media_add: usize,
    group_media_remove: usize,
}

impl OperationWeights {
    pub(super) fn from_rng(rng: &mut rand::rngs::StdRng) -> Self {
        let (album_creation, album_deletion) = growth_pair(rng);
        let (group_creation, group_deletion) = growth_pair(rng);
        let (album_media_add, album_media_remove) = growth_pair(rng);
        let (group_media_add, group_media_remove) = growth_pair(rng);
        Self {
            media_creation: rng.gen_range(1..=3),
            album_creation,
            album_deletion,
            group_creation,
            group_deletion,
            album_media_add,
            album_media_remove,
            group_media_add,
            group_media_remove,
        }
    }
}

fn growth_pair(rng: &mut rand::rngs::StdRng) -> (usize, usize) {
    let removal = rng.gen_range(1..=2);
    let addition = removal + rng.gen_range(1..=2);
    (addition, removal)
}

pub(super) fn draw_operation(
    device: &mut Replica,
    generator: &mut Values,
    weights: &OperationWeights,
    rng: &mut rand::rngs::StdRng,
) -> CrdtOperation {
    let dot = device.state.next_local_dot();
    let media = device.state.media_entries();
    let albums = device.state.album_entries();
    let groups = device.state.group_entries();
    let device_id = device.device_id.0;
    let mut candidates = Vec::new();

    macro_rules! candidate {
        ($weight:expr, $content:expr) => {{
            let operation = CrdtOperation {
                dot,
                author: LibraryUsername(format!("device-{device_id}")),
                timestamp: generator.timestamp(),
                content: $content,
            };
            for _ in 0..$weight {
                candidates.push(operation.clone());
            }
        }};
    }

    candidate!(
        weights.media_creation,
        OperationContent::MediaCreation(MediaCreation {
            media_id: generator.media(),
            filename_original: format!("{}.jpg", generator.value("media-original", device_id))
                .into(),
            date: generator.timestamp(),
            storage_date: StorageDate {
                year: 2026,
                month: 8,
            },
            size_bytes: 1 + rng.gen_range(0..10_000),
            content_hash: MediaHash::zeroed(),
            modified_at: None,
            gps: None,
            apple_aae_media_id: None,
            apple_live_photo_media_id: None,
        })
    );
    candidate!(
        weights.album_creation,
        OperationContent::AlbumCreation {
            album_id: generator.album(),
            name: generator.value("album-name", device_id).into(),
            parent_id: None,
        }
    );

    if !media.is_empty() {
        let item = media.choose(rng).unwrap();
        candidate!(
            DEFAULT_WEIGHT,
            OperationContent::MediaRename {
                media_id: item.media_id,
                name: (!rng.gen_bool(0.20))
                    .then(|| generator.value("media-name", device_id).into()),
            }
        );

        let item = media.choose(rng).unwrap();
        candidate!(
            DEFAULT_WEIGHT,
            OperationContent::MediaPropsUpdate {
                media_id: item.media_id,
                key: format!("property-{}", rng.gen_range(0..3)),
                value: generator.value("property-value", device_id),
            }
        );
    }

    if !albums.is_empty() {
        let album = albums.choose(rng).unwrap();
        candidate!(
            DEFAULT_WEIGHT,
            OperationContent::AlbumRename {
                album_id: album.album_id,
                name: (!rng.gen_bool(0.20))
                    .then(|| generator.value("album-name", device_id).into()),
            }
        );

        let album = albums.choose(rng).unwrap();
        let root_parent = albums
            .iter()
            .filter(|candidate| {
                candidate.album_id != album.album_id && candidate.album_id_parent.is_none()
            })
            .choose(rng)
            .map(|candidate| candidate.album_id);
        candidate!(
            DEFAULT_WEIGHT,
            OperationContent::AlbumReparent {
                album_id: album.album_id,
                parent_id: (!rng.gen_bool(0.25)).then_some(root_parent).flatten(),
            }
        );

        let album = albums.choose(rng).unwrap();
        candidate!(
            DEFAULT_WEIGHT,
            OperationContent::AlbumThumbnailSet {
                album_id: album.album_id,
                media_id: (!media.is_empty() && !rng.gen_bool(0.25))
                    .then(|| media.choose(rng).unwrap().media_id),
            }
        );

        candidate!(
            weights.album_deletion,
            OperationContent::AlbumDeletion {
                album_id: albums.choose(rng).unwrap().album_id,
            }
        );
        candidate!(
            weights.group_creation,
            OperationContent::GroupCreation {
                group_id: generator.group(),
                parent_id: albums.choose(rng).unwrap().album_id,
            }
        );

        if !media.is_empty() {
            let album = albums.choose(rng).unwrap();
            let item = media.choose(rng).unwrap();
            candidate!(
                weights.album_media_add,
                OperationContent::AlbumMediaAdd {
                    album_id: album.album_id,
                    media_id: item.media_id,
                }
            );
        }
        if let Some(album) = albums.iter().find(|album| !album.media_ids.is_empty()) {
            let media_id = *album.media_ids.choose(rng).unwrap();
            candidate!(
                weights.album_media_remove,
                OperationContent::AlbumMediaRemove {
                    album_id: album.album_id,
                    media_id,
                    observed: device.state.album_member_dots(album.album_id, media_id),
                }
            );
        }
    }

    if !groups.is_empty() {
        candidate!(
            weights.group_deletion,
            OperationContent::GroupDeletion {
                group_id: groups.choose(rng).unwrap().group_id,
            }
        );
        if !media.is_empty() {
            let group = groups.choose(rng).unwrap();
            let item = media.choose(rng).unwrap();
            candidate!(
                weights.group_media_add,
                OperationContent::GroupMediaAdd {
                    group_id: group.group_id,
                    media_id: item.media_id,
                }
            );
        }
        if let Some(group) = groups.iter().find(|group| !group.media_ids.is_empty()) {
            let media_id = *group.media_ids.choose(rng).unwrap();
            candidate!(
                weights.group_media_remove,
                OperationContent::GroupMediaRemove {
                    group_id: group.group_id,
                    media_id,
                    observed: device.state.group_member_dots(group.group_id, media_id),
                }
            );
        }
    }

    candidates.choose(rng).unwrap().clone()
}
