use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::LibraryError;
use crate::identifiers::{AlbumUuid, MediaUuid};
use crate::library::Library;
use crate::library::media::MediaEntry;
use crate::operations::{AlbumName, Operation};
use crate::state::GroupEntry;

pub type Result<T> = std::result::Result<T, LibraryError>;

#[derive(Debug, Clone)]
pub struct AlbumSummary {
    pub album_id: AlbumUuid,
    pub album_id_parent: Option<AlbumUuid>,
    pub name: AlbumName,
    pub media_count: usize,
    pub thumbnail_media_id: Option<MediaUuid>,
}

/// A media or group entry as displayed in an album's date-ordered item list.
#[derive(Debug, Clone)]
pub enum AlbumItem {
    Media(MediaEntry),
    Group(GroupEntry),
}

#[derive(Debug, Clone)]
pub struct DatedAlbumItem {
    pub item: AlbumItem,
    pub effective_date: DateTime<Utc>,
}

impl DatedAlbumItem {
    fn tie_breaker(&self) -> (u8, uuid::Uuid) {
        match &self.item {
            AlbumItem::Media(entry) => (0, entry.media_id.0),
            AlbumItem::Group(entry) => (1, entry.group_id.0),
        }
    }
}

impl Library {
    /// Returns direct, non-deleted albums under `parent_album_id`, ordered by
    /// name and then ID. `None` denotes the root album level.
    pub fn album_albums(&self, parent_album_id: Option<AlbumUuid>) -> Vec<AlbumSummary> {
        let state = self.inner.operation_state.read();
        let mut albums: Vec<_> = state
            .reconstructed
            .albums
            .values()
            .filter(|entry| !entry.deleted && entry.album_id_parent == parent_album_id)
            .map(|entry| AlbumSummary {
                album_id: entry.album_id,
                album_id_parent: entry.album_id_parent,
                name: entry.name.clone(),
                media_count: entry.media_ids.len(),
                thumbnail_media_id: entry.thumbnail_media_id,
            })
            .collect();
        albums.sort_by(|a, b| a.name.0.cmp(&b.name.0).then_with(|| a.album_id.0.cmp(&b.album_id.0)));
        albums
    }

    pub fn album_albums_count(&self, parent_album_id: Option<AlbumUuid>) -> usize {
        let state = self.inner.operation_state.read();
        state
            .reconstructed
            .albums
            .values()
            .filter(|entry| !entry.deleted && entry.album_id_parent == parent_album_id)
            .count()
    }

    pub fn album_albums_range(
        &self,
        parent_album_id: Option<AlbumUuid>,
        pos_start_inclusive: usize,
        pos_end_inclusive: usize,
    ) -> Vec<AlbumSummary> {
        if pos_start_inclusive > pos_end_inclusive {
            return Vec::new();
        }
        let take_count = pos_end_inclusive - pos_start_inclusive + 1;
        self.album_albums(parent_album_id)
            .into_iter()
            .skip(pos_start_inclusive)
            .take(take_count)
            .collect()
    }
    pub async fn album_create(
        &self,
        name: AlbumName,
        album_id_parent: Option<AlbumUuid>,
    ) -> Result<AlbumUuid> {
        let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
        self.append_to_pending(Operation::AlbumCreation {
            timestamp: Utc::now(),
            album_id,
            name,
            album_id_parent,
        })?;
        self.load_local_state().await?;
        Ok(album_id)
    }

    pub async fn album_add_media(&self, album_id: AlbumUuid, media_id: MediaUuid) -> Result<()> {
        self.append_to_pending(Operation::AlbumMediaAdd { timestamp: Utc::now(), album_id, media_id })?;
        self.load_local_state().await?;
        Ok(())
    }

    pub async fn album_remove_media(&self, album_id: AlbumUuid, media_id: MediaUuid) -> Result<()> {
        self.append_to_pending(Operation::AlbumMediaRemove { timestamp: Utc::now(), album_id, media_id })?;
        self.load_local_state().await?;
        Ok(())
    }

    pub async fn album_delete(&self, album_id: AlbumUuid) -> Result<()> {
        self.append_to_pending(Operation::AlbumDeletion { timestamp: Utc::now(), album_id })?;
        self.load_local_state().await?;
        Ok(())
    }

    pub fn album_list_media(&self, album_id: AlbumUuid) -> Result<Vec<MediaEntry>> {
        let state = self.inner.operation_state.read();
        let media_ids = state
            .views
            .by_album
            .get(&album_id)
            .ok_or(LibraryError::AlbumNotFound(album_id))?;
        let entries = media_ids
            .iter()
            .filter_map(|mid| {
                let media = state.reconstructed.media.get(mid)?;
                let group_ids = state
                    .views
                    .media_group_membership
                    .get(mid)
                    .cloned()
                    .unwrap_or_default();
                Some(MediaEntry::from_state(media, group_ids))
            })
            .collect();
        Ok(entries)
    }

    pub fn album_items_count(&self, album_id: AlbumUuid) -> Result<usize> {
        let state = self.inner.operation_state.read();
        let media_count = state
            .views
            .by_album
            .get(&album_id)
            .ok_or(LibraryError::AlbumNotFound(album_id))?
            .len();
        let group_count = state
            .views
            .groups_by_album
            .get(&album_id)
            .map_or(0, Vec::len);
        Ok(media_count + group_count)
    }

    /// Returns the inclusive range of media and groups in an album, ordered by
    /// effective date with a deterministic ID tie-breaker.
    pub fn album_items_by_date_range(
        &self,
        album_id: AlbumUuid,
        ascending: bool,
        pos_start_inclusive: usize,
        pos_end_inclusive: usize,
    ) -> Result<Vec<DatedAlbumItem>> {
        if pos_start_inclusive > pos_end_inclusive {
            return Ok(Vec::new());
        }
        let state = self.inner.operation_state.read();
        let media_ids = state
            .views
            .by_album
            .get(&album_id)
            .ok_or(LibraryError::AlbumNotFound(album_id))?;
        let mut items = Vec::with_capacity(
            media_ids.len() + state.views.groups_by_album.get(&album_id).map_or(0, Vec::len),
        );
        for media_id in media_ids {
            let Some(media) = state.reconstructed.media.get(media_id) else {
                continue;
            };
            let group_ids = state
                .views
                .media_group_membership
                .get(media_id)
                .cloned()
                .unwrap_or_default();
            let entry = MediaEntry::from_state(media, group_ids);
            items.push(DatedAlbumItem {
                effective_date: entry.date,
                item: AlbumItem::Media(entry),
            });
        }
        if let Some(group_ids) = state.views.groups_by_album.get(&album_id) {
            for group_id in group_ids {
                let Some(group) = state.reconstructed.groups.get(group_id) else {
                    continue;
                };
                let effective_date = group
                    .media_ids
                    .iter()
                    .filter_map(|media_id| state.reconstructed.media.get(media_id).map(|media| media.date))
                    .max()
                    .unwrap_or_default();
                items.push(DatedAlbumItem {
                    item: AlbumItem::Group(group.clone()),
                    effective_date,
                });
            }
        }
        items.sort_by(|a, b| {
            let date_order = a.effective_date.cmp(&b.effective_date);
            let order = if ascending { date_order } else { date_order.reverse() };
            order.then_with(|| a.tie_breaker().cmp(&b.tie_breaker()))
        });
        let take_count = pos_end_inclusive - pos_start_inclusive + 1;
        Ok(items
            .into_iter()
            .skip(pos_start_inclusive)
            .take(take_count)
            .collect())
    }

    pub fn album_list(&self) -> Vec<AlbumSummary> {
        let state = self.inner.operation_state.read();
        state
            .reconstructed
            .albums
            .values()
            .filter(|entry| !entry.deleted)
            .map(|entry| AlbumSummary {
                album_id: entry.album_id,
                album_id_parent: entry.album_id_parent,
                name: entry.name.clone(),
                media_count: entry.media_ids.len(),
                thumbnail_media_id: entry.thumbnail_media_id,
            })
            .collect()
    }

    /// Reconstruct the path from an album to its root (or as far as available).
    /// Uses a visited set so it is safe even if reparent ops created cycles.
    pub fn album_get_path(&self, album_id: AlbumUuid) -> String {
        let state = self.inner.operation_state.read();
        let mut current = album_id;
        let mut names = Vec::new();
        let mut visited: HashSet<AlbumUuid> = HashSet::new();

        while let Some(entry) = state.reconstructed.albums.get(&current) {
            if !visited.insert(current) {
                names.push("...".to_string());
                break;
            }
            names.push(entry.name.0.clone());

            let Some(parent) = entry.album_id_parent else {
                break;
            };
            current = parent;
        }

        names.reverse();
        names.join(" / ")
    }

    pub async fn album_rename(&self, album_id: AlbumUuid, name: AlbumName) -> Result<()> {
        {
            let state = self.inner.operation_state.read();
            if !state.reconstructed.albums.contains_key(&album_id) {
                return Err(LibraryError::AlbumNotFound(album_id));
            }
        }
        self.append_to_pending(Operation::AlbumRename { timestamp: Utc::now(), album_id, name })?;
        self.load_local_state().await?;
        Ok(())
    }

    pub async fn album_reparent(&self, album_id: AlbumUuid, new_parent_id: Option<AlbumUuid>) -> Result<()> {
        {
            let state = self.inner.operation_state.read();
            if !state.reconstructed.albums.contains_key(&album_id) {
                return Err(LibraryError::AlbumNotFound(album_id));
            }
            if new_parent_id == Some(album_id) {
                return Err(LibraryError::AlbumReparentWouldCycle);
            }
            if let Some(new_parent) = new_parent_id {
                let mut cursor = Some(new_parent);
                let mut visited: HashSet<AlbumUuid> = HashSet::new();
                while let Some(c) = cursor {
                    if c == album_id {
                        return Err(LibraryError::AlbumReparentWouldCycle);
                    }
                    if !visited.insert(c) {
                        break;
                    }
                    cursor = state.reconstructed.albums.get(&c).and_then(|e| e.album_id_parent);
                }
            }
        }
        self.append_to_pending(Operation::AlbumReparent { timestamp: Utc::now(), album_id, new_parent_id })?;
        self.load_local_state().await?;
        Ok(())
    }

    /// Returns all non-deleted album IDs not reachable from root.
    /// These arise from concurrent reparent ops that created cycles.
    pub fn album_disconnected_ids(&self) -> Vec<AlbumUuid> {
        let state = self.inner.operation_state.read();
        let mut reachable: HashSet<AlbumUuid> = HashSet::new();
        let mut queue: VecDeque<AlbumUuid> = VecDeque::new();

        if let Some(root_children) = state.views.album_children.get(&None) {
            for &id in root_children {
                if reachable.insert(id) {
                    queue.push_back(id);
                }
            }
        }

        while let Some(current) = queue.pop_front() {
            if let Some(children) = state.views.album_children.get(&Some(current)) {
                for &child in children {
                    if reachable.insert(child) {
                        queue.push_back(child);
                    }
                }
            }
        }

        state
            .reconstructed
            .albums
            .values()
            .filter(|a| !a.deleted && !reachable.contains(&a.album_id))
            .map(|a| a.album_id)
            .collect()
    }

    /// Return (name, parent_id, media_count, thumbnail_media_id) for a non-deleted album, or None.
    pub fn album_node_by_id(&self, album_id: AlbumUuid) -> Option<(AlbumName, Option<AlbumUuid>, usize, Option<MediaUuid>)> {
        let state = self.inner.operation_state.read();
        let entry = state.reconstructed.albums.get(&album_id)?;
        if entry.deleted {
            return None;
        }
        Some((entry.name.clone(), entry.album_id_parent, entry.media_ids.len(), entry.thumbnail_media_id))
    }

    pub async fn album_set_thumbnail(&self, album_id: AlbumUuid, media_id: Option<MediaUuid>) -> Result<()> {
        {
            let state = self.inner.operation_state.read();
            if !state.reconstructed.albums.contains_key(&album_id) {
                return Err(LibraryError::AlbumNotFound(album_id));
            }
        }
        self.append_to_pending(Operation::AlbumThumbnailSet { timestamp: Utc::now(), album_id, media_id })?;
        self.load_local_state().await?;
        Ok(())
    }

    /// Resolve an album name to its UUID.
    /// Returns an error if the name is not found or if multiple albums match (ambiguous).
    pub fn album_resolve_name(&self, name: &AlbumName) -> Result<AlbumUuid> {
        let state = self.inner.operation_state.read();
        let matches: Vec<(AlbumUuid, String)> = state
            .reconstructed
            .albums
            .iter()
            .filter(|(_, entry)| !entry.deleted && &entry.name == name)
            .map(|(id, _)| (*id, self.album_get_path(*id)))
            .collect();

        match matches.len() {
            0 => Err(LibraryError::AlbumNotFoundByName(name.clone())),
            1 => Ok(matches[0].0),
            _ => Err(LibraryError::AlbumNameAmbiguous(
                name.clone(),
                matches.into_iter().collect(),
            )),
        }
    }

}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::error::LibraryError;
    use crate::identifiers::LibraryId;
    use crate::library::{Credentials, Library};
    use crate::library::media::upload::MediaAddSource;
    use crate::library::local_dirs::LocalDirs;

    async fn make_library(tmp: &TempDir) -> (Library, LocalDirs) {
        use crate::operations::{LibraryPassword, LibraryUsername};
        let library_id = LibraryId(Uuid::new_v4());
        let local_dirs = LocalDirs::new(tmp.path().to_path_buf(), &library_id);
        local_dirs.ensure_state_dirs().unwrap();
        let (lib, _password_uuid) = Library::init(
            local_dirs.clone(),
            library_id,
            Credentials {
                username: LibraryUsername("alice".into()),
                password: LibraryPassword("pass".into()),
            },
        )
        .await
        .unwrap();
        (lib, local_dirs)
    }

    fn write_file(dir: &std::path::Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[tokio::test]
    // When a parent and child album exist, album_list reports the child's parent id.
    async fn album_list_reports_parent_child_relationship() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let parent_id = lib.album_create(AlbumName("Parent".into()), None).await.unwrap();
        let child_id = lib.album_create(AlbumName("Child".into()), Some(parent_id)).await.unwrap();

        let albums = lib.album_list();
        assert_eq!(albums.len(), 2);
        let parent = albums.iter().find(|a| a.album_id == parent_id).unwrap();
        assert_eq!(parent.album_id_parent, None);
        let child = albums.iter().find(|a| a.album_id == child_id).unwrap();
        assert_eq!(child.album_id_parent, Some(parent_id));
    }

    #[tokio::test]
    async fn album_albums_range_returns_only_direct_albums_in_name_order() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let zulu = lib.album_create(AlbumName("Zulu".into()), None).await.unwrap();
        let alpha = lib.album_create(AlbumName("Alpha".into()), None).await.unwrap();
        let _nested = lib
            .album_create(AlbumName("Nested".into()), Some(alpha))
            .await
            .unwrap();

        assert_eq!(lib.album_albums_count(None), 2);
        let first = lib.album_albums_range(None, 0, 0);
        let second = lib.album_albums_range(None, 1, 1);
        assert_eq!(first[0].album_id, alpha);
        assert_eq!(second[0].album_id, zulu);
        assert_eq!(lib.album_albums_count(Some(alpha)), 1);
    }

    #[tokio::test]
    // After album_add_media, album_list_media shows the file.
    async fn add_file_then_list_shows_file() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_id = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let src = write_file(tmp.path(), "photo.jpg", b"data");
        let media_id = lib.media_add(MediaAddSource::CopyFrom(src), Some(album_id), None, None, None).await.unwrap().id();

        let media = lib.album_list_media(album_id).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].media_id, media_id);
    }

    #[tokio::test]
    // After album_remove_media, the file is no longer in the list.
    async fn remove_file_then_list_is_empty() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_id = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let src = write_file(tmp.path(), "photo.jpg", b"data");
        let media_id = lib.media_add(MediaAddSource::CopyFrom(src), Some(album_id), None, None, None).await.unwrap().id();
        lib.album_remove_media(album_id, media_id).await.unwrap();

        let media = lib.album_list_media(album_id).unwrap();
        assert!(media.is_empty());
    }

    #[tokio::test]
    // After album_delete, the album no longer appears in album_list.
    async fn deleted_album_absent_from_list() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_id = lib.album_create(AlbumName("Deletable".into()), None).await.unwrap();
        lib.album_delete(album_id).await.unwrap();

        let albums = lib.album_list();
        assert!(albums.iter().all(|a| a.album_id != album_id));
    }

    #[tokio::test]
    // file_add auto-inserts file into specified album via AlbumMediaAdd op.
    async fn file_add_auto_inserts_into_album() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_id = lib.album_create(AlbumName("Upload".into()), None).await.unwrap();
        let src = write_file(tmp.path(), "img.jpg", b"pixels");
        let media_id = lib.media_add(MediaAddSource::CopyFrom(src), Some(album_id), None, None, None).await.unwrap().id();

        let state = lib.inner.operation_state.read();
        let by_album = &state.views.by_album;
        assert!(by_album.get(&album_id).map_or(false, |ids| ids.contains(&media_id)));
    }

    #[tokio::test]
    // album_add_media on an explicit separate call also works.
    async fn manual_album_add_media_works() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_a = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let album_b = lib.album_create(AlbumName("B".into()), None).await.unwrap();
        let src = write_file(tmp.path(), "img.jpg", b"pixels");
        let media_id = lib.media_add(MediaAddSource::CopyFrom(src), Some(album_a), None, None, None).await.unwrap().id();

        lib.album_add_media(album_b, media_id).await.unwrap();
        let media_b = lib.album_list_media(album_b).unwrap();
        assert_eq!(media_b.len(), 1);
        assert_eq!(media_b[0].media_id, media_id);
    }

    #[tokio::test]
    // album_get_path for root album returns just its name
    async fn album_get_path_root_album() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_id = lib
            .album_create(AlbumName("Root Album".into()), None)
            .await
            .unwrap();
        lib.load_local_state().await.unwrap();

        let path = lib.album_get_path(album_id);
        assert_eq!(path, "Root Album");
    }

    #[tokio::test]
    // album_get_path for nested album returns full path
    async fn album_get_path_nested_album() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let root = lib
            .album_create(AlbumName("Root".into()), None)
            .await
            .unwrap();
        let child = lib
            .album_create(AlbumName("Child".into()), Some(root))
            .await
            .unwrap();
        let grandchild = lib
            .album_create(AlbumName("Grandchild".into()), Some(child))
            .await
            .unwrap();
        lib.load_local_state().await.unwrap();

        let path = lib.album_get_path(grandchild);
        assert_eq!(path, "Root / Child / Grandchild");
    }

    #[tokio::test]
    // album_resolve_name returns correct UUID for unique name
    async fn album_resolve_name_unique() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_name = AlbumName("Unique Album".into());
        let album_id = lib.album_create(album_name.clone(), None).await.unwrap();
        lib.load_local_state().await.unwrap();

        let resolved = lib.album_resolve_name(&album_name).unwrap();
        assert_eq!(resolved, album_id);
    }

    #[tokio::test]
    // album_resolve_name returns NotFound for non-existent album
    async fn album_resolve_name_not_found() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_name = AlbumName("Nonexistent".into());
        let result = lib.album_resolve_name(&album_name);

        assert!(matches!(result, Err(LibraryError::AlbumNotFoundByName(_))));
    }

    #[tokio::test]
    // album_resolve_name returns Ambiguous for duplicate names
    async fn album_resolve_name_ambiguous() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_name = AlbumName("Duplicate".into());
        let _album1 = lib.album_create(album_name.clone(), None).await.unwrap();
        let _album2 = lib.album_create(album_name.clone(), None).await.unwrap();
        lib.load_local_state().await.unwrap();

        let result = lib.album_resolve_name(&album_name);
        assert!(matches!(result, Err(LibraryError::AlbumNameAmbiguous(_, _))));

        if let Err(LibraryError::AlbumNameAmbiguous(_, matches)) = result {
            assert_eq!(matches.len(), 2);
            // Both should have path "Duplicate" since they're root-level
            assert!(matches.iter().all(|(_, path)| path == "Duplicate"));
        }
    }

    #[tokio::test]
    // album_resolve_name excludes deleted albums
    async fn album_resolve_name_excludes_deleted() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_name = AlbumName("Deletable".into());
        let album_id = lib.album_create(album_name.clone(), None).await.unwrap();
        lib.load_local_state().await.unwrap();

        // Delete the album
        lib.album_delete(album_id).await.unwrap();
        lib.load_local_state().await.unwrap();

        // Resolution should now fail
        let result = lib.album_resolve_name(&album_name);
        assert!(matches!(result, Err(LibraryError::AlbumNotFoundByName(_))));
    }

    #[tokio::test]
    // album_get_path with deleted album still works (deleted flag doesn't affect path lookup)
    async fn album_get_path_deleted_album() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_name = AlbumName("Deletable".into());
        let album_id = lib.album_create(album_name.clone(), None).await.unwrap();
        lib.load_local_state().await.unwrap();

        // Delete the album
        lib.album_delete(album_id).await.unwrap();
        lib.load_local_state().await.unwrap();

        // Path should still be resolvable
        let path = lib.album_get_path(album_id);
        assert_eq!(path, "Deletable");
    }

    #[tokio::test]
    // album_rename changes the album's name
    async fn album_rename_changes_name() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_id = lib.album_create(AlbumName("Before".into()), None).await.unwrap();
        lib.album_rename(album_id, AlbumName("After".into())).await.unwrap();

        let albums = lib.album_list();
        assert_eq!(albums.iter().find(|a| a.album_id == album_id).unwrap().name.0, "After");
    }

    #[tokio::test]
    // album_rename on non-existent album returns AlbumNotFound
    async fn album_rename_not_found() {
        use crate::identifiers::AlbumUuid;
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let fake_id = AlbumUuid::from_uuid(uuid::Uuid::new_v4());
        let err = lib.album_rename(fake_id, AlbumName("X".into())).await.unwrap_err();
        assert!(matches!(err, LibraryError::AlbumNotFound(_)));
    }

    #[tokio::test]
    // album_reparent moves a child under a new parent
    async fn album_reparent_moves_album() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let root_a = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let root_b = lib.album_create(AlbumName("B".into()), None).await.unwrap();
        let child = lib.album_create(AlbumName("C".into()), Some(root_a)).await.unwrap();

        lib.album_reparent(child, Some(root_b)).await.unwrap();

        let albums = lib.album_list();
        let child_entry = albums.iter().find(|a| a.album_id == child).unwrap();
        assert_eq!(child_entry.album_id_parent, Some(root_b));
        assert!(albums.iter().all(|a| a.album_id_parent != Some(root_a)));
    }

    #[tokio::test]
    // album_reparent rejects self-parenting
    async fn album_reparent_self_is_cycle() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let album_id = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let err = lib.album_reparent(album_id, Some(album_id)).await.unwrap_err();
        assert!(matches!(err, LibraryError::AlbumReparentWouldCycle));
    }

    #[tokio::test]
    // album_reparent rejects parent that is a descendant
    async fn album_reparent_descendant_is_cycle() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        let parent = lib.album_create(AlbumName("P".into()), None).await.unwrap();
        let child = lib.album_create(AlbumName("C".into()), Some(parent)).await.unwrap();

        let err = lib.album_reparent(parent, Some(child)).await.unwrap_err();
        assert!(matches!(err, LibraryError::AlbumReparentWouldCycle));
    }

    #[tokio::test]
    // album_disconnected_ids returns empty when no cycles exist
    async fn album_disconnected_ids_empty_without_cycles() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;

        lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let parent = lib.album_create(AlbumName("B".into()), None).await.unwrap();
        lib.album_create(AlbumName("C".into()), Some(parent)).await.unwrap();

        assert!(lib.album_disconnected_ids().is_empty());
    }
}
