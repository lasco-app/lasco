use std::collections::HashSet;

use super::operations::{album, assert_every_delivery_order, dot, media, operation};
use crate::crdt::*;

#[test]
fn an_album_thumbnail_can_be_set_and_then_cleared() {
    let album_id = album(1);
    let media_id = media(2);
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
            dot(2, 2),
            OperationContent::AlbumThumbnailSet {
                album_id,
                media_id: Some(media_id),
            },
        ),
        operation(
            dot(3, 1),
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
        dot(1, 1),
        OperationContent::AlbumMediaAdd { album_id, media_id },
    );
    let operations = [
        first_add.clone(),
        operation(
            dot(1, 2),
            OperationContent::AlbumMediaAdd { album_id, media_id },
        ),
        operation(
            dot(2, 1),
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
            HashSet::from([dot(1, 2)])
        );
    });
}

#[test]
fn a_deleted_album_stays_absent_when_earlier_operations_arrive_late() {
    let album_id = album(1);
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
            OperationContent::AlbumRename {
                album_id,
                name: Some("Summer holiday".into()),
            },
        ),
        operation(dot(3, 2), OperationContent::AlbumDeletion { album_id }),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert!(state.album(album_id).is_none());
    });
}

#[test]
fn two_albums_that_name_each_other_as_parent_become_a_visible_tree() {
    let first = album(1);
    let second = album(2);
    let operations = [
        operation(
            dot(10, 1),
            OperationContent::AlbumCreation {
                album_id: first,
                name: "First".into(),
                parent_id: Some(second),
            },
        ),
        operation(
            dot(20, 1),
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
        dot(1, 1),
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
