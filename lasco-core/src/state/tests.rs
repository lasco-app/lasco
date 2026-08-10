use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid, OpUuid};
use crate::operations::{LibraryUsername, MediaFilename, Operation, StorageDate};

use super::*;

fn op_group(ops: Vec<Operation>) -> OperationGroup {
    OperationGroup {
        op_id: OpUuid::new(),
        parent_op_id: None,
        author: LibraryUsername("test".to_string()),
        operations: ops,
    }
}

fn media_id() -> MediaUuid {
    MediaUuid::from_uuid(Uuid::new_v4())
}

fn album_id() -> AlbumUuid {
    AlbumUuid::from_uuid(Uuid::new_v4())
}

fn group_id() -> GroupUuid {
    GroupUuid::from_uuid(Uuid::new_v4())
}

fn t(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
}

fn media_creation(mid: MediaUuid, date: DateTime<Utc>) -> Operation {
    Operation::MediaCreation {
        timestamp: Utc::now(),
        media_id: mid,
        filename_original: MediaFilename("img.jpg".into()),
        date,
        storage_date: StorageDate {
            year: date.format("%Y").to_string().parse().unwrap(),
            month: date.format("%m").to_string().parse().unwrap(),
        },
        size_bytes: 1024,
        content_hash: crate::library::media::MediaHash::zeroed(),
        modified_at: None,
        gps: None,
        apple_aae_media_id: None,
        apple_live_photo_media_id: None,
    }
}

#[test]
fn empty_ops_empty_state() {
    let state = reconstruct_state(&[]);
    assert!(state.media.is_empty());
    assert!(state.albums.is_empty());
    assert!(state.groups.is_empty());
}

#[test]
fn media_creation_no_album_not_reachable() {
    let mid = media_id();
    let date = t(2024, 1, 1);
    let groups = vec![op_group(vec![media_creation(mid, date)])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert!(state.media.contains_key(&mid));
    assert!(!views.reachable_media_ids.contains(&mid));
    assert!(views.by_date.is_empty());
}

#[test]
fn album_creation_and_media_add_appears_in_by_album() {
    let mid = media_id();
    let aid = album_id();
    let date = t(2024, 6, 15);
    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "Vacation".into(),
            album_id_parent: None,
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert_eq!(views.by_album[&aid], vec![mid]);
    assert!(views.reachable_media_ids.contains(&mid));
}

#[test]
fn album_media_remove_makes_media_unreachable_but_stays_in_media() {
    let mid = media_id();
    let aid = album_id();
    let date = t(2024, 3, 10);
    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
        Operation::AlbumMediaRemove {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert!(state.media.contains_key(&mid));
    assert!(!views.reachable_media_ids.contains(&mid));
    assert!(views.by_album[&aid].is_empty());
}

#[test]
fn group_creation_with_album_parent() {
    let aid = album_id();
    let gid = group_id();
    let groups = vec![op_group(vec![
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert_eq!(state.groups[&gid].album_id_parent, aid);
    assert!(views.groups_by_album[&aid].contains(&gid));
}

#[test]
fn multiple_interleaved_ops_produce_correct_state() {
    let mid1 = media_id();
    let mid2 = media_id();
    let aid1 = album_id();
    let aid2 = album_id();
    let date = t(2024, 1, 1);

    let groups = vec![op_group(vec![
        media_creation(mid1, date),
        media_creation(mid2, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid1,
            name: "A1".into(),
            album_id_parent: None,
        },
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid2,
            name: "A2".into(),
            album_id_parent: Some(aid1),
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid1,
            media_id: mid1,
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid2,
            media_id: mid2,
        },
        Operation::AlbumMediaRemove {
            timestamp: Utc::now(),
            album_id: aid1,
            media_id: mid1,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert!(state.albums[&aid1].media_ids.is_empty());
    assert_eq!(state.albums[&aid2].media_ids, vec![mid2]);
    assert!(!views.reachable_media_ids.contains(&mid1));
    assert!(views.reachable_media_ids.contains(&mid2));
}

#[test]
fn duplicate_album_media_add_does_not_duplicate() {
    let mid = media_id();
    let aid = album_id();
    let date = t(2024, 1, 1);
    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);

    assert_eq!(state.albums[&aid].media_ids.len(), 1);
}

#[test]
fn by_date_correct_and_album_children_nested() {
    let mid = media_id();
    let aid_parent = album_id();
    let aid_child = album_id();
    let date = t(2024, 5, 20);

    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid_parent,
            name: "P".into(),
            album_id_parent: None,
        },
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid_child,
            name: "C".into(),
            album_id_parent: Some(aid_parent),
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid_child,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert!(views.by_date[&date].contains(&mid));
    assert!(views.album_children[&Some(aid_parent)].contains(&aid_child));
    assert!(views.album_children[&None].contains(&aid_parent));
}

#[test]
fn group_creation_media_add_reachable_and_in_by_group() {
    let mid = media_id();
    let aid = album_id();
    let gid = group_id();
    let date = t(2024, 7, 4);

    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert!(views.by_group[&gid].contains(&mid));
    assert!(views.reachable_media_ids.contains(&mid));
}

#[test]
fn group_media_remove_makes_media_unreachable() {
    let mid = media_id();
    let aid = album_id();
    let gid = group_id();
    let date = t(2024, 8, 1);

    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
        Operation::GroupMediaRemove {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert!(state.media.contains_key(&mid));
    assert!(!views.reachable_media_ids.contains(&mid));
    assert!(views.by_group[&gid].is_empty());
}

#[test]
fn group_deletion_media_become_unreachable() {
    let mid = media_id();
    let aid = album_id();
    let gid = group_id();
    let date = t(2024, 9, 1);

    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
        Operation::GroupDeletion {
            timestamp: Utc::now(),
            group_id: gid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    assert!(!views.by_group.contains_key(&gid));
    assert!(!views.reachable_media_ids.contains(&mid));
}

#[test]
fn parent_album_deleted_group_media_unreachable() {
    let mid = media_id();
    let aid = album_id();
    let gid = group_id();
    let date = t(2024, 10, 1);

    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
        Operation::AlbumDeletion {
            timestamp: Utc::now(),
            album_id: aid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    // group and media_ids remain in reconstructed state
    assert!(!state.groups[&gid].deleted);
    assert!(state.groups[&gid].media_ids.contains(&mid));
    // but transitive reachability is lost
    assert!(!views.reachable_media_ids.contains(&mid));
}

#[test]
fn media_group_membership_reverse_index() {
    let mid = media_id();
    let aid = album_id();
    let gid1 = group_id();
    let gid2 = group_id();
    let date = t(2024, 11, 1);

    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid1,
            album_id_parent: aid,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid2,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid1,
            media_id: mid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid2,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    let membership = &views.media_group_membership[&mid];
    assert!(membership.contains(&gid1));
    assert!(membership.contains(&gid2));

    // After removing from gid1
    let groups2 = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid1,
            album_id_parent: aid,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid2,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid1,
            media_id: mid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid2,
            media_id: mid,
        },
        Operation::GroupMediaRemove {
            timestamp: Utc::now(),
            group_id: gid1,
            media_id: mid,
        },
    ])];
    let state2 = reconstruct_state(&groups2);
    let views2 = build_computed_views(&state2);

    let m2 = &views2.media_group_membership[&mid];
    assert!(!m2.contains(&gid1));
    assert!(m2.contains(&gid2));

    // After group deletion
    let groups3 = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid1,
            album_id_parent: aid,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid2,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid1,
            media_id: mid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid2,
            media_id: mid,
        },
        Operation::GroupDeletion {
            timestamp: Utc::now(),
            group_id: gid1,
        },
    ])];
    let state3 = reconstruct_state(&groups3);
    let views3 = build_computed_views(&state3);

    let m3 = &views3.media_group_membership[&mid];
    assert!(!m3.contains(&gid1));
    assert!(m3.contains(&gid2));
}

#[test]
fn duplicate_group_media_add_does_not_duplicate() {
    let mid = media_id();
    let aid = album_id();
    let gid = group_id();
    let date = t(2024, 1, 1);

    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);

    assert_eq!(state.groups[&gid].media_ids.len(), 1);
}

#[test]
fn media_in_album_and_group_remains_reachable_after_removal_from_one() {
    let mid = media_id();
    let aid = album_id();
    let gid = group_id();
    let date = t(2024, 2, 1);

    // Media in both album and group. Remove from album only.
    let groups = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
        Operation::AlbumMediaRemove {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
    ])];
    let state = reconstruct_state(&groups);
    let views = build_computed_views(&state);

    // Still reachable via group
    assert!(views.reachable_media_ids.contains(&mid));

    // Remove from group too
    let groups2 = vec![op_group(vec![
        media_creation(mid, date),
        Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id: aid,
            name: "A".into(),
            album_id_parent: None,
        },
        Operation::AlbumMediaAdd {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
        Operation::GroupCreation {
            timestamp: Utc::now(),
            group_id: gid,
            album_id_parent: aid,
        },
        Operation::GroupMediaAdd {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
        Operation::AlbumMediaRemove {
            timestamp: Utc::now(),
            album_id: aid,
            media_id: mid,
        },
        Operation::GroupMediaRemove {
            timestamp: Utc::now(),
            group_id: gid,
            media_id: mid,
        },
    ])];
    let state2 = reconstruct_state(&groups2);
    let views2 = build_computed_views(&state2);

    assert!(!views2.reachable_media_ids.contains(&mid));
}
