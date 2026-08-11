use chrono::Utc;
use uuid::Uuid;

use crate::crdt::OperationContent;
use crate::error::LibraryError;
use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::library::Library;
use crate::library::media::MediaEntry;
use crate::state::GroupEntry;

pub type Result<T> = std::result::Result<T, LibraryError>;

impl Library {
    /// # Errors
    ///
    /// Returns an error if the parent album is absent or the group-creation operation cannot be persisted.
    pub async fn group_create(&self, album_id_parent: AlbumUuid) -> Result<GroupUuid> {
        let group_id = GroupUuid::from_uuid(Uuid::new_v4());
        self.record_local_operation(
            Utc::now(),
            OperationContent::GroupCreation {
                group_id,
                parent_id: album_id_parent,
            },
        )?;
        self.load_local_state().await?;
        Ok(group_id)
    }

    /// # Errors
    ///
    /// Returns an error if the group or media is absent, or the membership operation cannot be persisted.
    pub async fn group_add_media(&self, group_id: GroupUuid, media_id: MediaUuid) -> Result<()> {
        self.record_local_operation(
            Utc::now(),
            OperationContent::GroupMediaAdd { group_id, media_id },
        )?;
        self.load_local_state().await?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the group or membership is absent, or the removal operation cannot be persisted.
    pub async fn group_remove_media(&self, group_id: GroupUuid, media_id: MediaUuid) -> Result<()> {
        let observed = self
            .inner
            .crdt_replica_state
            .read()
            .state
            .group_member_dots(group_id, media_id);
        self.record_local_operation(
            Utc::now(),
            OperationContent::GroupMediaRemove {
                group_id,
                media_id,
                observed,
            },
        )?;
        self.load_local_state().await?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the group is absent or the delete operation cannot be persisted.
    pub async fn group_delete(&self, group_id: GroupUuid) -> Result<()> {
        self.record_local_operation(Utc::now(), OperationContent::GroupDeletion { group_id })?;
        self.load_local_state().await?;
        Ok(())
    }

    #[must_use]
    pub fn group_list(&self) -> Vec<GroupEntry> {
        let state = self.inner.operation_state.read();
        state
            .reconstructed
            .groups
            .values()
            .filter(|g| !g.deleted)
            .cloned()
            .collect()
    }

    /// # Errors
    ///
    /// Returns an error if the group is absent.
    pub fn group_list_media(&self, group_id: GroupUuid) -> Result<Vec<MediaEntry>> {
        let state = self.inner.operation_state.read();
        let media_ids = state
            .views
            .by_group
            .get(&group_id)
            .ok_or(LibraryError::GroupNotFound(group_id))?;
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

    /// # Errors
    ///
    /// Returns an error if the album is absent.
    pub fn album_list_groups(&self, album_id: AlbumUuid) -> Result<Vec<GroupEntry>> {
        let state = self.inner.operation_state.read();
        // Return AlbumNotFound only if the album was never created.
        if !state.reconstructed.albums.contains_key(&album_id) {
            return Err(LibraryError::AlbumNotFound(album_id));
        }
        let entries = state
            .views
            .groups_by_album
            .get(&album_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|gid| state.reconstructed.groups.get(gid))
                    .filter(|g| !g.deleted)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::identifiers::LibraryId;
    use crate::library::local_dirs::LocalDirs;
    use crate::library::media::upload::MediaAddSource;
    use crate::library::{Credentials, Library};

    fn make_library(tmp: &TempDir) -> Library {
        use crate::operations::{LibraryPassword, LibraryUsername};
        let library_id = LibraryId(Uuid::new_v4());
        let local_dirs = LocalDirs::new(tmp.path().to_path_buf(), &library_id);
        local_dirs.ensure_state_dirs().unwrap();
        Library::init(
            local_dirs,
            library_id,
            Credentials {
                username: LibraryUsername("alice".into()),
                password: LibraryPassword("pass".into()),
            },
        )
        .unwrap()
        .0
    }

    fn write_file(dir: &std::path::Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[tokio::test]
    // After creating a group, group_list includes it and album_list_groups returns it.
    async fn create_group_appears_in_list_and_album() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let lib = make_library(&tmp);

        let album_id = lib
            .album_create(AlbumName("Album".into()), None)
            .await
            .unwrap();
        let group_id = lib.group_create(album_id).await.unwrap();

        let groups = lib.group_list();
        assert!(groups.iter().any(|g| g.group_id == group_id));

        let album_groups = lib.album_list_groups(album_id).unwrap();
        assert!(album_groups.iter().any(|g| g.group_id == group_id));
    }

    #[tokio::test]
    // After group_add_media, group_list_media shows the file. The file is reachable without being in an album directly.
    async fn add_file_to_group_makes_it_reachable() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let lib = make_library(&tmp);

        let album_id = lib
            .album_create(AlbumName("Album".into()), None)
            .await
            .unwrap();
        let group_id = lib.group_create(album_id).await.unwrap();
        let src = write_file(tmp.path(), "photo.jpg", b"data");
        // file_add requires an album — use a separate album so the file is in album A but NOT in the group's parent
        let album_b = lib.album_create(AlbumName("B".into()), None).await.unwrap();
        let media_id = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_b),
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .id();
        // Remove from album_b so reachability comes only from the group
        lib.album_remove_media(album_b, media_id).await.unwrap();

        lib.group_add_media(group_id, media_id).await.unwrap();

        let media = lib.group_list_media(group_id).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].media_id, media_id);

        // The file is reachable through the group and its parent album chain.
        let state = lib.inner.operation_state.read();
        assert!(state.views.reachable_media_ids.contains(&media_id));
    }

    #[tokio::test]
    // After group_remove_media, the file is gone from group_list_media. With no other membership it becomes unreachable.
    async fn remove_file_from_group_becomes_unreachable() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let lib = make_library(&tmp);

        let album_id = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let group_id = lib.group_create(album_id).await.unwrap();
        let src = write_file(tmp.path(), "photo.jpg", b"data");
        let media_id = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .id();
        lib.album_remove_media(album_id, media_id).await.unwrap();
        lib.group_add_media(group_id, media_id).await.unwrap();
        lib.group_remove_media(group_id, media_id).await.unwrap();

        let media = lib.group_list_media(group_id).unwrap();
        assert!(media.is_empty());

        let state = lib.inner.operation_state.read();
        assert!(!state.views.reachable_media_ids.contains(&media_id));
    }

    #[tokio::test]
    // After group_delete, the group is absent from group_list and group_list_media returns an error.
    async fn delete_group_absent_from_list_and_files_error() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let lib = make_library(&tmp);

        let album_id = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let group_id = lib.group_create(album_id).await.unwrap();
        lib.group_delete(group_id).await.unwrap();

        let groups = lib.group_list();
        assert!(groups.iter().all(|g| g.group_id != group_id));

        let err = lib.group_list_media(group_id).unwrap_err();
        assert!(
            matches!(err, crate::error::LibraryError::GroupNotFound(_)),
            "expected GroupNotFound, got {err:?}"
        );
    }

    #[tokio::test]
    // When the parent album is deleted, group files become unreachable. The group remains in reconstructed.groups.
    async fn parent_album_deleted_group_files_unreachable() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let lib = make_library(&tmp);

        let album_id = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let group_id = lib.group_create(album_id).await.unwrap();
        let src = write_file(tmp.path(), "photo.jpg", b"data");
        let media_id = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .id();
        lib.album_remove_media(album_id, media_id).await.unwrap();
        lib.group_add_media(group_id, media_id).await.unwrap();

        lib.album_delete(album_id).await.unwrap();

        let state = lib.inner.operation_state.read();
        // Groups beneath a deleted parent are hidden from the CRDT projection.
        assert!(!state.reconstructed.groups.contains_key(&group_id));
        // Its media is also unreachable (transitive).
        assert!(!state.views.reachable_media_ids.contains(&media_id));
    }

    #[tokio::test]
    // Duplicate group_add_media for same pair doesn't duplicate the file.
    async fn duplicate_group_add_media_no_duplicate() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let lib = make_library(&tmp);

        let album_id = lib.album_create(AlbumName("A".into()), None).await.unwrap();
        let group_id = lib.group_create(album_id).await.unwrap();
        let src = write_file(tmp.path(), "photo.jpg", b"data");
        let media_id = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .id();

        lib.group_add_media(group_id, media_id).await.unwrap();
        lib.group_add_media(group_id, media_id).await.unwrap();

        let media = lib.group_list_media(group_id).unwrap();
        assert_eq!(media.len(), 1);
    }

    #[tokio::test]
    // Groups never appear in album_list. Albums never appear in group_list.
    async fn groups_and_albums_are_separate() {
        use crate::operations::AlbumName;
        let tmp = TempDir::new().unwrap();
        let lib = make_library(&tmp);

        let album_id = lib
            .album_create(AlbumName("Album".into()), None)
            .await
            .unwrap();
        lib.group_create(album_id).await.unwrap();

        // album_list only ever contains AlbumSummary entries, so this is a type-level
        // guarantee, but confirm the counts line up as expected.
        let albums = lib.album_list();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].album_id, album_id);

        let groups = lib.group_list();
        assert_eq!(groups.len(), 1);
    }
}
