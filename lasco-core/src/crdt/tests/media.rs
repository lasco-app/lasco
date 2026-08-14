use chrono::Utc;

use super::operations::{assert_every_delivery_order, media, operation};
use crate::crdt::*;
use crate::library::media::MediaHash;
use crate::operations::{MediaName, StorageDate};

fn create_media(media_id: crate::identifiers::MediaUuid) -> CrdtOperation {
    operation(
        Dot {
            lamport_counter: 1,
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
    )
}

#[test]
fn a_media_item_renamed_on_another_device_keeps_the_latest_name() {
    let media_id = media(1);
    let operations = [
        create_media(media_id),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::MediaRename {
                media_id,
                name: Some("First edit".into()),
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(2),
            },
            OperationContent::MediaRename {
                media_id,
                name: Some("Second edit".into()),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(
            state.media(media_id).unwrap().name,
            Some(MediaName("Second edit".into()))
        );
    });
}

#[test]
fn simultaneous_media_renames_use_the_device_id_to_break_the_tie() {
    let media_id = media(1);
    let operations = [
        create_media(media_id),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::MediaRename {
                media_id,
                name: Some("First device".into()),
            },
        ),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(2),
            },
            OperationContent::MediaRename {
                media_id,
                name: Some("Second device".into()),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(
            state.media(media_id).unwrap().name,
            Some(MediaName("Second device".into()))
        );
    });
}

#[test]
fn a_media_item_renamed_before_its_creation_arrives_is_still_renamed() {
    let media_id = media(1);
    let operations = [
        create_media(media_id),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(2),
            },
            OperationContent::MediaRename {
                media_id,
                name: Some("Edited".into()),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        assert_eq!(
            state.media(media_id).unwrap().name,
            Some(MediaName("Edited".into()))
        );
    });
}

#[test]
fn a_media_item_keeps_the_latest_value_for_each_updated_property() {
    let media_id = media(1);
    let operations = [
        create_media(media_id),
        operation(
            Dot {
                lamport_counter: 2,
                device_id: DeviceId(1),
            },
            OperationContent::MediaPropsUpdate {
                media_id,
                key: "camera".into(),
                value: "old camera".into(),
            },
        ),
        operation(
            Dot {
                lamport_counter: 3,
                device_id: DeviceId(2),
            },
            OperationContent::MediaPropsUpdate {
                media_id,
                key: "camera".into(),
                value: "new camera".into(),
            },
        ),
        operation(
            Dot {
                lamport_counter: 4,
                device_id: DeviceId(3),
            },
            OperationContent::MediaPropsUpdate {
                media_id,
                key: "lens".into(),
                value: "50 mm".into(),
            },
        ),
    ];

    assert_every_delivery_order(&operations, |state| {
        let item = state.media(media_id).unwrap();
        assert_eq!(item.properties["camera"], "new camera");
        assert_eq!(item.properties["lens"], "50 mm");
    });
}
