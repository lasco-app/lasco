use std::collections::HashSet;

use chrono::Utc;

use super::operations::{album, assert_every_delivery_order, media, operation};
use crate::crdt::*;
use crate::library::media::MediaHash;
use crate::operations::StorageDate;

#[test]
fn an_album_thumbnail_can_be_set_and_then_cleared() {
    let album_id = album(1);
    let media_id = media(2);
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
                device_id: DeviceId(2),
            },
            OperationContent::AlbumThumbnailSet {
                album_id,
                media_id: Some(media_id),
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumThumbnailSet {
                album_id,
                media_id: None,
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(state.album(album_id).unwrap().thumbnail_media_id, None);
    });
}

#[test]
fn an_album_remove_keeps_media_added_concurrently_on_another_device() {
    let album_id = album(1);
    let media_id = media(2);
    let first_add = operation(
        Dot {
            lamport_counter: 1,
            device_id: DeviceId(1),
        },
        OperationContent::AlbumMediaAdd { album_id, media_id },
    );
    let operations = [
        first_add.clone(),
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(2),
            },
            OperationContent::AlbumMediaAdd { album_id, media_id },
        ),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumMediaRemove {
                album_id,
                media_id,
                observed: HashSet::from([first_add.dot]),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(
            state.album_member_dots(album_id, media_id),
            HashSet::from([Dot {
                lamport_counter: 1,
                device_id: DeviceId(2)
            }])
        );
    });
}

#[test]
fn a_media_item_removed_from_an_album_remains_in_the_library() {
    let album_id = album(1);
    let media_id = media(2);
    let add = operation(
        Dot {
            lamport_counter: 2,
            device_id: DeviceId(1),
        },
        OperationContent::AlbumMediaAdd { album_id, media_id },
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
        add.clone(),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumMediaRemove {
                album_id,
                media_id,
                observed: HashSet::from([add.dot]),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert!(state.media(media_id).is_some());
        assert!(state.album_member_dots(album_id, media_id).is_empty());
        assert!(state.album(album_id).unwrap().media_ids.is_empty());
    });
}

#[test]
fn a_deleted_album_stays_absent_when_earlier_operations_arrive_late() {
    let album_id = album(1);
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
            OperationContent::AlbumRename {
                album_id,
                name: Some("Summer holiday".into()),
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
        assert!(state.album(album_id).is_none());
    });
}

#[test]
fn an_album_renamed_and_given_a_thumbnail_before_creation_arrives_keeps_both_changes() {
    let album_id = album(1);
    let media_id = media(2);
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
                device_id: DeviceId(2),
            },
            OperationContent::AlbumRename {
                album_id,
                name: Some("Summer holiday".into()),
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(2),
            },
            OperationContent::AlbumThumbnailSet {
                album_id,
                media_id: Some(media_id),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        let album = state.album(album_id).unwrap();
        assert_eq!(album.name.0, "Summer holiday");
        assert_eq!(album.thumbnail_media_id, Some(media_id));
    });
}

#[test]
fn an_album_moved_between_parents_can_be_made_a_root_again() {
    let first_parent = album(1);
    let second_parent = album(2);
    let child = album(3);
    let operations = [
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id: first_parent,
                name: "First parent".into(),
                parent_id: None,
            },
        ),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id: second_parent,
                name: "Second parent".into(),
                parent_id: None,
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id: child,
                name: "Child".into(),
                parent_id: Some(first_parent),
            },
        ),
        operation(
            Dot {
                lamport_counter: 4,
                device_id: DeviceId(2),
            },
            OperationContent::AlbumReparent {
                album_id: child,
                parent_id: Some(second_parent),
            },
        ),
        operation(
            Dot {
                lamport_counter: 5,
                device_id: DeviceId(2),
            },
            OperationContent::AlbumReparent {
                album_id: child,
                parent_id: None,
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(state.album(child).unwrap().album_id_parent, None);
    });
}

#[test]
fn a_live_child_is_hidden_when_its_parent_is_deleted() {
    let parent = album(1);
    let child = album(2);
    let operations = [
        operation(
            Dot {
                lamport_counter: 1,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id: parent,
                name: "Parent".into(),
                parent_id: None,
            },
        ),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id: child,
                name: "Child".into(),
                parent_id: Some(parent),
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(2),
            },
            OperationContent::AlbumDeletion { album_id: parent },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert!(state.album(parent).is_none());
        assert!(state.album(child).is_none());
    });
}

#[test]
fn two_albums_that_name_each_other_as_parent_become_a_visible_tree() {
    let first = album(1);
    let second = album(2);
    let operations = [
        operation(
            Dot {
                lamport_counter: 10,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id: first,
                name: "First".into(),
                parent_id: Some(second),
            },
        ),
        operation(
            Dot {
                lamport_counter: 20,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumCreation {
                album_id: second,
                name: "Second".into(),
                parent_id: Some(first),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(state.album(first).unwrap().album_id_parent, None);
        assert_eq!(state.album(second).unwrap().album_id_parent, Some(first));
    });
}

#[test]
fn an_album_with_a_missing_parent_is_not_shown() {
    let album_id = album(1);
    let operations = [operation(
        Dot {
            lamport_counter: 1,
            device_id: DeviceId(1),
        },
        OperationContent::AlbumCreation {
            album_id,
            name: "Orphan".into(),
            parent_id: Some(album(99)),
        },
    )];

    assert_every_delivery_order(&operations, |state| {
        assert!(state.album(album_id).is_none());
    });
}
