use std::collections::HashSet;

use chrono::Utc;

use super::operations::{album, assert_every_delivery_order, group, media, operation};
use crate::crdt::*;
use crate::library::media::MediaHash;
use crate::operations::StorageDate;

#[test]
fn a_created_group_with_media_is_shown_under_its_album() {
    let album_id = album(1);
    let group_id = group(2);
    let media_id = media(3);
    let operations = [
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id,
                name: "Holiday".into(),
                parent_id: None,
            },
        ),
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(2),
            },
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
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::GroupCreation {
                group_id,
                parent_id: album_id,
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(1),
            },
            OperationContent::GroupMediaAdd { group_id, media_id },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(state.group(group_id).unwrap().album_id_parent, album_id);
        assert_eq!(state.group(group_id).unwrap().media_ids, vec![media_id]);
        assert_eq!(state.media(media_id).unwrap().group_ids, vec![group_id]);
    });
}

#[test]
fn a_media_item_removed_from_a_group_remains_in_the_library() {
    let album_id = album(1);
    let group_id = group(2);
    let media_id = media(3);
    let add = operation(
        Dot {
            lamport_counter: 3,
            device_id: DeviceId(1),
        },
        OperationContent::GroupMediaAdd { group_id, media_id },
    );
    let operations = [
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id,
                name: "Holiday".into(),
                parent_id: None,
            },
        ),
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(2),
            },
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
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::GroupCreation {
                group_id,
                parent_id: album_id,
            },
        ),
        add.clone(),
        operation(
            Dot {
                lamport_counter: 4,
                device_id: DeviceId(1),
            },
            OperationContent::GroupMediaRemove {
                group_id,
                media_id,
                observed: HashSet::from([add.dot]),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert!(state.media(media_id).is_some());
        assert!(state.group(group_id).unwrap().media_ids.is_empty());
        assert!(state.media(media_id).unwrap().group_ids.is_empty());
    });
}

#[test]
fn a_group_remove_keeps_media_added_concurrently_on_another_device() {
    let group_id = group(1);
    let media_id = media(2);
    let first_add = operation(
        Dot {
            lamport_counter: 1,
            device_id: DeviceId(1),
        },
        OperationContent::GroupMediaAdd { group_id, media_id },
    );
    let operations = [
        first_add.clone(),
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(2),
            },
            OperationContent::GroupMediaAdd { group_id, media_id },
        ),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::GroupMediaRemove {
                group_id,
                media_id,
                observed: HashSet::from([first_add.dot]),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(
            state.group_member_dots(group_id, media_id),
            HashSet::from([Dot {
                lamport_counter: 1,
                device_id: DeviceId(2)
            }])
        );
    });
}

#[test]
fn a_group_is_not_shown_after_its_parent_album_is_deleted() {
    let album_id = album(1);
    let group_id = group(2);
    let operations = [
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id,
                name: "Holiday".into(),
                parent_id: None,
            },
        ),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::GroupCreation {
                group_id,
                parent_id: album_id,
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(2),
            },
            OperationContent::AlbumDeletion { album_id },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert!(state.group(group_id).is_none());
    });
}

#[test]
fn a_deleted_group_stays_absent_when_its_creation_arrives_late() {
    let album_id = album(1);
    let group_id = group(2);
    let operations = [
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id,
                name: "Holiday".into(),
                parent_id: None,
            },
        ),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::GroupCreation {
                group_id,
                parent_id: album_id,
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(2),
            },
            OperationContent::GroupDeletion { group_id },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert!(state.group(group_id).is_none());
    });
}
