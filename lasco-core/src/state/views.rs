use super::types::{AlbumBrowseItem, ComputedViews};
use crate::crdt::CrdtState;

#[allow(
    clippy::too_many_lines,
    reason = "The related derived indexes are built together from one reconstructed-state snapshot."
)]
/// # Panics
///
/// Panics if an album referenced by the reconstructed browse hierarchy is absent from its album
/// map, violating the consistency invariant established while reconstructing state.
pub fn build_computed_views(state: &CrdtState) -> ComputedViews {
    let mut views = ComputedViews::default();
    let resolved = state.resolve_entries();
    let media_entries = resolved.media;
    let album_entries = resolved.albums;
    let group_entries = resolved.groups;
    let media: rustc_hash::FxHashMap<_, _> = media_entries
        .iter()
        .map(|entry| (entry.media_id, entry))
        .collect();
    let albums: rustc_hash::FxHashMap<_, _> = album_entries
        .iter()
        .map(|entry| (entry.album_id, entry))
        .collect();
    let groups: rustc_hash::FxHashMap<_, _> = group_entries
        .iter()
        .map(|entry| (entry.group_id, entry))
        .collect();

    // album_children maps each parent ID (None = root) to its child album IDs.
    for album in albums.values() {
        views
            .album_children
            .entry(album.album_id_parent)
            .or_default()
            .push(album.album_id);
    }

    // by_album holds current media_ids for non-deleted albums.
    for album in albums.values() {
        views
            .by_album
            .insert(album.album_id, album.media_ids.clone());
    }

    // groups_by_album and by_group contain non-deleted groups only.
    // Also build media_group_membership
    for group in groups.values() {
        views
            .groups_by_album
            .entry(group.album_id_parent)
            .or_default()
            .push(group.group_id);
        views
            .by_group
            .insert(group.group_id, group.media_ids.clone());
        for &media_id in &group.media_ids {
            views
                .media_group_membership
                .entry(media_id)
                .or_default()
                .push(group.group_id);
        }
    }

    // reachable_media_ids
    // (a) media in non-deleted albums
    for album in albums.values() {
        for &media_id in &album.media_ids {
            views.reachable_media_ids.insert(media_id);
        }
    }
    // (b) media in non-deleted groups whose parent album is non-deleted
    for group in groups.values() {
        let parent_alive = albums.contains_key(&group.album_id_parent);
        if parent_alive {
            for &media_id in &group.media_ids {
                views.reachable_media_ids.insert(media_id);
            }
        }
    }

    // by_date buckets reachable media by date.
    for &media_id in &views.reachable_media_ids {
        if let Some(media) = media.get(&media_id) {
            views.by_date.entry(media.date).or_default().push(media_id);
        }
    }

    // Home and orphan browsing operate on primary media only. Store their
    // canonical order directly so a range read only resolves its requested IDs.
    for entry in media.values() {
        if entry.companion_kind.is_some() {
            continue;
        }
        views.home_visible_newest.push(entry.media_id);
        if !views.reachable_media_ids.contains(&entry.media_id) {
            views.home_orphaned_newest.push(entry.media_id);
        }
    }

    // Equal timestamps need a deterministic order so consecutive ranges never
    // overlap or skip an item merely because a hash-map iteration changed.
    for ids in views.by_date.values_mut() {
        ids.sort_by_key(|id| id.0);
    }
    views.home_visible_newest.sort_by(|left, right| {
        let left_date = media.get(left).map(|entry| entry.date);
        let right_date = media.get(right).map(|entry| entry.date);
        right_date
            .cmp(&left_date)
            .then_with(|| right.0.cmp(&left.0))
    });
    views.home_orphaned_newest.sort_by(|left, right| {
        let left_date = media.get(left).map(|entry| entry.date);
        let right_date = media.get(right).map(|entry| entry.date);
        right_date
            .cmp(&left_date)
            .then_with(|| right.0.cmp(&left.0))
    });

    // Direct album browsing includes normal, non-deleted children only.
    for album in albums.values() {
        views
            .album_albums_by_name
            .entry(album.album_id_parent)
            .or_default()
            .push(album.album_id);
    }
    for album_ids in views.album_albums_by_name.values_mut() {
        album_ids.sort_by(|left, right| {
            let left_album = albums.get(left).expect("browse view album exists");
            let right_album = albums.get(right).expect("browse view album exists");
            left_album
                .name
                .0
                .cmp(&right_album.name.0)
                .then_with(|| left.0.cmp(&right.0))
        });
    }

    // Album item views are mixed media/group sequences. A group's effective
    // date is the newest date of its contained media (or the chrono default
    // for an empty/unresolvable group, matching the previous query behavior).
    for album in albums.values() {
        let mut items: Vec<_> = album
            .media_ids
            .iter()
            .copied()
            .map(AlbumBrowseItem::Media)
            .collect();
        if let Some(group_ids) = views.groups_by_album.get(&album.album_id) {
            items.extend(group_ids.iter().copied().map(AlbumBrowseItem::Group));
        }
        items.sort_by(|left, right| {
            let effective_date = |item: &AlbumBrowseItem| match item {
                AlbumBrowseItem::Media(media_id) => media
                    .get(media_id)
                    .map(|media| media.date)
                    .unwrap_or_default(),
                AlbumBrowseItem::Group(group_id) => groups
                    .get(group_id)
                    .into_iter()
                    .flat_map(|group| group.media_ids.iter())
                    .filter_map(|media_id| media.get(media_id).map(|entry| entry.date))
                    .max()
                    .unwrap_or_default(),
            };
            let tie_breaker = |item: &AlbumBrowseItem| match item {
                AlbumBrowseItem::Media(media_id) => (0_u8, media_id.0),
                AlbumBrowseItem::Group(group_id) => (1_u8, group_id.0),
            };
            effective_date(right)
                .cmp(&effective_date(left))
                .then_with(|| tie_breaker(left).cmp(&tie_breaker(right)))
        });
        views.album_items_newest.insert(album.album_id, items);
    }

    // by_content_hash groups all media IDs sharing the same content hash, including unreachable media.
    for entry in media.values() {
        views
            .by_content_hash
            .entry(entry.content_hash)
            .or_default()
            .push(entry.media_id);
    }

    views
}
