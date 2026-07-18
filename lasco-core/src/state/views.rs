use super::types::{ComputedViews, ReconstructedState};

pub fn build_computed_views(state: &ReconstructedState) -> ComputedViews {
    let mut views = ComputedViews::default();

    // album_children maps each parent ID (None = root) to its child album IDs.
    for album in state.albums.values() {
        views
            .album_children
            .entry(album.album_id_parent)
            .or_default()
            .push(album.album_id);
    }

    // by_album holds current media_ids for non-deleted albums.
    for album in state.albums.values() {
        if !album.deleted {
            views.by_album.insert(album.album_id, album.media_ids.clone());
        }
    }

    // groups_by_album and by_group contain non-deleted groups only.
    // Also build media_group_membership
    for group in state.groups.values() {
        if !group.deleted {
            views
                .groups_by_album
                .entry(group.album_id_parent)
                .or_default()
                .push(group.group_id);
            views.by_group.insert(group.group_id, group.media_ids.clone());
            for &media_id in &group.media_ids {
                views
                    .media_group_membership
                    .entry(media_id)
                    .or_default()
                    .push(group.group_id);
            }
        }
    }

    // reachable_media_ids
    // (a) media in non-deleted albums
    for album in state.albums.values() {
        if !album.deleted {
            for &media_id in &album.media_ids {
                views.reachable_media_ids.insert(media_id);
            }
        }
    }
    // (b) media in non-deleted groups whose parent album is non-deleted
    for group in state.groups.values() {
        if !group.deleted {
            let parent_alive = state
                .albums
                .get(&group.album_id_parent)
                .is_some_and(|a| !a.deleted);
            if parent_alive {
                for &media_id in &group.media_ids {
                    views.reachable_media_ids.insert(media_id);
                }
            }
        }
    }

    // by_date buckets reachable media by date.
    for &media_id in &views.reachable_media_ids {
        if let Some(media) = state.media.get(&media_id) {
            views.by_date.entry(media.date).or_default().push(media_id);
        }
    }

    // by_content_hash groups all media IDs sharing the same content hash, including unreachable media.
    for media in state.media.values() {
        views
            .by_content_hash
            .entry(media.content_hash)
            .or_default()
            .push(media.media_id);
    }

    views
}
