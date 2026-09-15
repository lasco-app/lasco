use chrono::Utc;

use super::operations::{album, assert_every_delivery_order, group, media, operation};
use crate::crdt::*;
use crate::library::media::MediaHash;
use crate::operations::{ApplePhotosCloudCollectionId, MediaName, StorageDate};

#[test]
fn collection_link_uses_the_earliest_dot_as_its_canonical_album() {
    let first_album = album(1);
    let second_album = album(2);
    let collection_id = ApplePhotosCloudCollectionId("icloud-collection".into());
    let operations = [
        operation(
            Dot { lamport_counter: 2, device_id: DeviceId(1) },
            OperationContent::ApplePhotosCollectionLinkAdded(ApplePhotosCollectionLink {
                album_id: second_album,
                cloud_collection_id: collection_id.clone(),
                kind: ApplePhotosCollectionKind::Album,
            }),
        ),
        operation(
            Dot { lamport_counter: 1, device_id: DeviceId(2) },
            OperationContent::ApplePhotosCollectionLinkAdded(ApplePhotosCollectionLink {
                album_id: first_album,
                cloud_collection_id: collection_id,
                kind: ApplePhotosCollectionKind::Album,
            }),
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        let canonical = state
            .apple_photos_collection_links
            .iter()
            .min_by_key(|entry| entry.dot)
            .unwrap();
        assert_eq!(canonical.link.album_id, first_album);
    });
}

#[test]
fn collection_links_round_trip_with_their_explicit_operation_kind() {
    let collection_id = ApplePhotosCloudCollectionId("icloud-folder".into());
    let operation = operation(
        Dot { lamport_counter: 7, device_id: DeviceId(3) },
        OperationContent::ApplePhotosCollectionLinkAdded(ApplePhotosCollectionLink {
            album_id: album(9),
            cloud_collection_id: collection_id.clone(),
            kind: ApplePhotosCollectionKind::Folder,
        }),
    );

    let json = serde_json::to_string(&operation).unwrap();
    let decoded: CrdtOperation = serde_json::from_str(&json).unwrap();

    assert!(json.contains("ApplePhotosCollectionLinkAdded"));
    assert_eq!(decoded, operation);
    assert_eq!(collection_id.to_string(), "icloud-folder");
}

#[test]
fn an_older_state_without_collection_links_deserializes_to_no_links() {
    let state = CrdtState::new(DeviceId(1));
    let mut encoded = serde_json::to_value(state).unwrap();
    encoded
        .as_object_mut()
        .unwrap()
        .remove("apple_photos_collection_links");

    let decoded: CrdtState = serde_json::from_value(encoded).unwrap();

    assert!(decoded.apple_photos_collection_links.is_empty());
}

#[test]
fn a_photo_added_to_an_album_and_group_converges_for_every_delivery_order() {
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
                lamport_counter: 2,
                device_id: DeviceId(1),
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
                lamport_counter: 3,
                device_id: DeviceId(2),
            },
            OperationContent::AlbumMediaAdd { album_id, media_id },
        ),
        operation(
            Dot {
                lamport_counter: 4,
                device_id: DeviceId(2),
            },
            OperationContent::GroupCreation {
                group_id,
                parent_id: album_id,
            },
        ),
        operation(
            Dot {
                lamport_counter: 5,
                device_id: DeviceId(3),
            },
            OperationContent::GroupMediaAdd { group_id, media_id },
        ),
        operation(
            Dot {
                lamport_counter: 6,
                device_id: DeviceId(3),
            },
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
        Dot {
            lamport_counter: 41,
            device_id: DeviceId(2),
        },
        OperationContent::AlbumDeletion { album_id: album(1) },
    ));
    assert_eq!(
        state.next_local_dot(),
        Dot {
            lamport_counter: 42,
            device_id: DeviceId(9)
        }
    );
}

#[test]
fn delivering_the_same_album_history_twice_keeps_the_exact_same_crdt_state() {
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
        add.clone(),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(1),
            },
            OperationContent::AlbumMediaRemove {
                album_id,
                media_id,
                observed: std::collections::HashSet::from([add.dot]),
            },
        ),
    ];
    let mut delivered_once = CrdtState::new(DeviceId(99));
    delivered_once.merge_all(operations.iter());
    let mut delivered_twice = CrdtState::new(DeviceId(99));
    delivered_twice.merge_all(operations.iter().chain(operations.iter()));

    assert_eq!(delivered_twice, delivered_once);
    assert!(delivered_twice
        .album(album_id)
        .unwrap()
        .media_ids
        .is_empty());
}
