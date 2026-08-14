use chrono::Utc;

use super::operations::{album, assert_every_delivery_order, dot, group, media, operation};
use crate::crdt::*;
use crate::library::media::MediaHash;
use crate::operations::{MediaName, StorageDate};

#[test]
fn a_photo_added_to_an_album_and_group_converges_for_every_delivery_order() {
    let album_id = album(1);
    let group_id = group(2);
    let media_id = media(3);
    let operations = [
        operation(
            dot(1, 1),
            OperationContent::AlbumCreation {
                album_id,
                name: "Holiday".into(),
                parent_id: None,
            },
        ),
        operation(
            dot(2, 1),
            OperationContent::MediaCreation(MediaCreation {
                media_id,
                filename_original: "source.jpg".into(),
                date: Utc::now(),
                storage_date: StorageDate {
                    year: 2026,
                    month: 8,
                },
                size_bytes: 42,
                content_hash: MediaHash::zeroed(),
                modified_at: None,
                gps: None,
                apple_aae_media_id: None,
                apple_live_photo_media_id: None,
            }),
        ),
        operation(
            dot(3, 2),
            OperationContent::AlbumMediaAdd { album_id, media_id },
        ),
        operation(
            dot(4, 2),
            OperationContent::GroupCreation {
                group_id,
                parent_id: album_id,
            },
        ),
        operation(
            dot(5, 3),
            OperationContent::GroupMediaAdd { group_id, media_id },
        ),
        operation(
            dot(6, 3),
            OperationContent::MediaRename {
                media_id,
                name: Some("Edited photo".into()),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        let item = state.media(media_id).unwrap();
        assert_eq!(item.name, Some(MediaName("Edited photo".into())));
        assert_eq!(item.group_ids, vec![group_id]);
        assert_eq!(state.album(album_id).unwrap().media_ids, vec![media_id]);
        assert_eq!(state.group(group_id).unwrap().media_ids, vec![media_id]);
    });
}

#[test]
fn a_device_uses_a_dot_after_the_latest_remote_operation_it_received() {
    let mut state = CrdtState::new(DeviceId(9));
    state.apply(&operation(
        dot(41, 2),
        OperationContent::AlbumDeletion { album_id: album(1) },
    ));
    assert_eq!(state.next_local_dot(), dot(42, 9));
}
