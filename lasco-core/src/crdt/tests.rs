use std::collections::HashSet;

use chrono::Utc;
use uuid::Uuid;

use crate::crdt::*;
use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::library::media::MediaHash;
use crate::operations::{AlbumName, LibraryUsername, MediaFilename, MediaName, StorageDate};

fn dot(counter: u64, device: u128) -> Dot {
    Dot {
        lamport_counter: counter,
        device_id: DeviceId(device),
    }
}
fn album(n: u128) -> AlbumUuid {
    AlbumUuid::from_uuid(Uuid::from_u128(n))
}
fn media(n: u128) -> MediaUuid {
    MediaUuid::from_uuid(Uuid::from_u128(n))
}
fn op(dot: Dot, content: OperationContent) -> CrdtOperation {
    CrdtOperation {
        dot,
        author: LibraryUsername("test".into()),
        timestamp: Utc::now(),
        content,
    }
}

#[test]
fn lww_is_order_independent_including_option_clear() {
    let id = album(1);
    let create = op(
        dot(1, 1),
        OperationContent::AlbumCreation {
            album_id: id,
            name: "A".into(),
            parent_id: None,
        },
    );
    let rename = op(
        dot(2, 1),
        OperationContent::AlbumRename {
            album_id: id,
            name: Some("B".into()),
        },
    );
    let clear = op(
        dot(3, 1),
        OperationContent::AlbumRename {
            album_id: id,
            name: None,
        },
    );
    let mut left = CanonicalState::new(DeviceId(1));
    left.merge_all([&create, &rename, &clear]);
    let mut right = CanonicalState::new(DeviceId(2));
    right.merge_all([&clear, &create, &rename]);
    assert_eq!(left.albums[&id].name, right.albums[&id].name);
    assert_eq!(left.albums[&id].name.as_ref().unwrap().value, None);
}

#[test]
fn observed_remove_is_add_wins_for_unseen_concurrent_adds() {
    let aid = album(1);
    let mid = media(2);
    let add_a = op(
        dot(1, 1),
        OperationContent::AlbumMediaAdd {
            album_id: aid,
            media_id: mid,
        },
    );
    let add_b = op(
        dot(1, 2),
        OperationContent::AlbumMediaAdd {
            album_id: aid,
            media_id: mid,
        },
    );
    let remove = op(
        dot(2, 1),
        OperationContent::AlbumMediaRemove {
            album_id: aid,
            media_id: mid,
            observed: HashSet::from([add_a.dot]),
        },
    );
    let mut state = CanonicalState::new(DeviceId(3));
    state.merge_all([&remove, &add_b, &add_a, &add_a]);
    assert_eq!(
        state.album_member_dots(aid, mid),
        HashSet::from([add_b.dot])
    );
}

#[test]
fn mutation_before_creation_retains_its_lww_value() {
    let id = album(1);
    let rename = op(
        dot(2, 1),
        OperationContent::AlbumRename {
            album_id: id,
            name: Some("B".into()),
        },
    );
    let create = op(
        dot(1, 1),
        OperationContent::AlbumCreation {
            album_id: id,
            name: "A".into(),
            parent_id: None,
        },
    );
    let mut state = CanonicalState::new(DeviceId(1));
    state.merge_all([&rename, &create]);
    assert_eq!(
        state.albums[&id].name.as_ref().unwrap().value,
        Some(AlbumName("B".into()))
    );
    assert!(state.is_album_created_and_live(id));
}

#[test]
fn tombstones_are_permanent_and_hide_membership() {
    let id = album(1);
    let mid = media(2);
    let create = op(
        dot(1, 1),
        OperationContent::AlbumCreation {
            album_id: id,
            name: "A".into(),
            parent_id: None,
        },
    );
    let delete = op(dot(2, 2), OperationContent::AlbumDeletion { album_id: id });
    let add = op(
        dot(3, 3),
        OperationContent::AlbumMediaAdd {
            album_id: id,
            media_id: mid,
        },
    );
    let mut state = CanonicalState::new(DeviceId(4));
    state.merge_all([&add, &delete, &create]);
    assert!(!state.is_album_created_and_live(id));
    assert!(state.album_member_dots(id, mid).contains(&add.dot));
}

#[test]
fn cycles_sever_the_lowest_winning_parent_dot() {
    let a = album(1);
    let b = album(2);
    let create_a = op(
        dot(10, 1),
        OperationContent::AlbumCreation {
            album_id: a,
            name: "A".into(),
            parent_id: Some(b),
        },
    );
    let create_b = op(
        dot(20, 1),
        OperationContent::AlbumCreation {
            album_id: b,
            name: "B".into(),
            parent_id: Some(a),
        },
    );
    let mut state = CanonicalState::new(DeviceId(3));
    state.merge_all([&create_b, &create_a]);
    let projection = state.album_projection();
    assert_eq!(projection.effective_parents[&a], None);
    assert_eq!(projection.effective_parents[&b], Some(a));
    assert_eq!(projection.visible, HashSet::from([a, b]));
}

#[test]
fn clock_advances_past_remote_observations() {
    let mut clock = ReplicaClock::new(DeviceId(9));
    clock.observe(dot(41, 2));
    assert_eq!(clock.next_dot(), dot(42, 9));
}

#[test]
fn media_registers_preserve_creation_and_merge_each_property_independently() {
    let id = media(1);
    let created_at = Utc::now();
    let create = op(
        dot(1, 1),
        OperationContent::MediaCreation(MediaCreation {
            media_id: id,
            filename_original: "source.jpg".into(),
            date: created_at,
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
    );
    let rename = op(
        dot(4, 2),
        OperationContent::MediaRename {
            media_id: id,
            name: Some("Edited".into()),
        },
    );
    let property_old = op(
        dot(2, 2),
        OperationContent::MediaPropsUpdate {
            media_id: id,
            key: "camera".into(),
            value: "old".into(),
        },
    );
    let property_new = op(
        dot(3, 3),
        OperationContent::MediaPropsUpdate {
            media_id: id,
            key: "camera".into(),
            value: "new".into(),
        },
    );
    let mut state = CanonicalState::new(DeviceId(4));
    state.merge_all([&rename, &property_new, &create, &property_old]);
    let media = &state.media[&id];
    assert_eq!(
        media.creation.as_ref().unwrap().value.filename_original,
        MediaFilename("source.jpg".into())
    );
    assert_eq!(
        media.name.as_ref().unwrap().value,
        Some(MediaName("Edited".into()))
    );
    assert_eq!(media.properties["camera"].value, "new");
}

#[test]
fn group_parent_is_immutable_and_its_membership_is_an_observed_remove_set() {
    let parent = album(1);
    let group = GroupUuid::from_uuid(Uuid::from_u128(2));
    let mid = media(3);
    let create = op(
        dot(1, 1),
        OperationContent::GroupCreation {
            group_id: group,
            parent_id: parent,
        },
    );
    // Same-ID creation conflicts resolve by dot but are not a reparent operation.
    let later_create = op(
        dot(2, 2),
        OperationContent::GroupCreation {
            group_id: group,
            parent_id: album(9),
        },
    );
    let add = op(
        dot(3, 1),
        OperationContent::GroupMediaAdd {
            group_id: group,
            media_id: mid,
        },
    );
    let remove = op(
        dot(4, 1),
        OperationContent::GroupMediaRemove {
            group_id: group,
            media_id: mid,
            observed: HashSet::from([add.dot]),
        },
    );
    let delete = op(
        dot(5, 3),
        OperationContent::GroupDeletion { group_id: group },
    );
    let mut state = CanonicalState::new(DeviceId(4));
    state.merge_all([&delete, &remove, &later_create, &add, &create]);
    assert_eq!(
        state.groups[&group]
            .creation
            .as_ref()
            .unwrap()
            .value
            .parent_id,
        album(9)
    );
    assert!(state.group_member_dots(group, mid).is_empty());
    assert!(state.groups[&group].tombstone.is_some());
}

#[test]
fn reordered_and_duplicated_operations_converge_to_the_same_canonical_state() {
    let id = album(1);
    let mid = media(2);
    let create = op(
        dot(1, 1),
        OperationContent::AlbumCreation {
            album_id: id,
            name: "A".into(),
            parent_id: None,
        },
    );
    let thumbnail = op(
        dot(2, 2),
        OperationContent::AlbumThumbnailSet {
            album_id: id,
            media_id: Some(mid),
        },
    );
    let clear_thumbnail = op(
        dot(3, 1),
        OperationContent::AlbumThumbnailSet {
            album_id: id,
            media_id: None,
        },
    );
    let add = op(
        dot(4, 2),
        OperationContent::AlbumMediaAdd {
            album_id: id,
            media_id: mid,
        },
    );
    let operations = [&create, &thumbnail, &clear_thumbnail, &add];
    let mut left = CanonicalState::new(DeviceId(7));
    left.merge_all(operations);
    let mut right = CanonicalState::new(DeviceId(8));
    right.merge_all([&add, &clear_thumbnail, &create, &thumbnail, &add]);
    assert_eq!(left.causal_context, right.causal_context);
    assert_eq!(left.albums, right.albums);
    assert_eq!(left.album_memberships, right.album_memberships);
    assert_eq!(left.albums[&id].thumbnail.as_ref().unwrap().value, None);
}

#[test]
fn persisted_state_keeps_causal_context_clock_and_outbox() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crdt-state.enc");
    let master_key = crate::encryption::master_key::generate_master_key();
    let operation = op(
        dot(7, 2),
        OperationContent::AlbumDeletion { album_id: album(1) },
    );
    let mut persisted = PersistedReplica {
        state: CanonicalState::new(DeviceId(3)),
        outgoing: vec![operation.clone()],
    };
    persisted.state.apply(&operation);
    save_persisted(&path, &master_key, &persisted).unwrap();
    let mut loaded = load_persisted(&path, &master_key, DeviceId(99)).unwrap();
    assert_eq!(loaded, persisted);
    assert_eq!(loaded.state.next_local_dot(), dot(8, 3));
}
