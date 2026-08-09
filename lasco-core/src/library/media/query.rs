use std::path::Path;

use chrono::Utc;

use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob::decrypt_blob;
use crate::encryption::blob_key::derive_blob_key;
use crate::error::{LibraryError, OperationError};
use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::library::Library;
use crate::operations::{MediaName, Operation};
use crate::remote::MediaList;
use crate::storage::Storage;

use super::MediaEntry;

pub type Result<T> = std::result::Result<T, LibraryError>;

/// Writes `data` to a temp file next to `path` then renames it into place, so a concurrent
/// reader never observes a partially-written file.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp_name = format!(
        "{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// Selects which media entries `Library::media_list` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaListScope {
    /// Media in at least one non-deleted album or group. Used everywhere the media is
    /// meant to be album-visible.
    Reachable,
    /// Every primary media item, whether it belongs to an album or is orphaned.
    /// Companion resources referenced by another media item (AAE and Live Photo video)
    /// are excluded.
    Visible,
    /// Primary media with no live album or group membership. Companion resources
    /// referenced by another media item (AAE and Live Photo video) are excluded.
    Orphaned,
    /// Every known media entry, including AAE sidecars and media orphaned from all albums.
    All,
}

impl Library {
    /// Returns primary visible media ordered by date descending, then ID descending.
    pub fn media_by_date_count(&self, orphaned: bool) -> usize {
        let state = self.inner.operation_state.read();
        if orphaned {
            state.views.home_orphaned_newest.len()
        } else {
            state.views.home_visible_newest.len()
        }
    }

    /// Returns the inclusive position range from the canonical home-media order.
    pub fn media_by_date_range(
        &self,
        orphaned: bool,
        pos_start_inclusive: usize,
        pos_end_inclusive: usize,
    ) -> Vec<MediaEntry> {
        if pos_start_inclusive > pos_end_inclusive {
            return Vec::new();
        }
        let state = self.inner.operation_state.read();
        let ids = if orphaned {
            &state.views.home_orphaned_newest
        } else {
            &state.views.home_visible_newest
        };
        let Some(range) = inclusive_slice(ids, pos_start_inclusive, pos_end_inclusive) else {
            return Vec::new();
        };
        range
            .iter()
            .filter_map(|media_id| {
                let entry = state.reconstructed.media.get(media_id)?;
                let group_ids = state
                    .views
                    .media_group_membership
                    .get(media_id)
                    .cloned()
                    .unwrap_or_default();
                Some(MediaEntry::from_state(entry, group_ids))
            })
            .collect()
    }

    /// Returns media entries matching `scope`.
    pub fn media_list(&self, scope: MediaListScope) -> Vec<MediaEntry> {
        let state = self.inner.operation_state.read();
        match scope {
            MediaListScope::Reachable => state
                .views
                .reachable_media_ids
                .iter()
                .filter_map(|&media_id| {
                    let entry = state.reconstructed.media.get(&media_id)?;
                    let group_ids = state
                        .views
                        .media_group_membership
                        .get(&media_id)
                        .cloned()
                        .unwrap_or_default();
                    Some(MediaEntry::from_state(entry, group_ids))
                })
                .collect(),
            MediaListScope::Visible => {
                let companion_ids: std::collections::HashSet<_> = state
                    .reconstructed
                    .media
                    .values()
                    .flat_map(|entry| [entry.apple_aae_media_id, entry.apple_live_photo_media_id])
                    .flatten()
                    .collect();

                state
                    .reconstructed
                    .media
                    .values()
                    .filter(|entry| !companion_ids.contains(&entry.media_id))
                    .map(|entry| {
                        let group_ids = state
                            .views
                            .media_group_membership
                            .get(&entry.media_id)
                            .cloned()
                            .unwrap_or_default();
                        MediaEntry::from_state(entry, group_ids)
                    })
                    .collect()
            }
            MediaListScope::Orphaned => {
                let companion_ids: std::collections::HashSet<_> = state
                    .reconstructed
                    .media
                    .values()
                    .flat_map(|entry| [entry.apple_aae_media_id, entry.apple_live_photo_media_id])
                    .flatten()
                    .collect();

                state
                    .reconstructed
                    .media
                    .values()
                    .filter(|entry| {
                        !state.views.reachable_media_ids.contains(&entry.media_id)
                            && !companion_ids.contains(&entry.media_id)
                    })
                    .map(|entry| {
                        let group_ids = state
                            .views
                            .media_group_membership
                            .get(&entry.media_id)
                            .cloned()
                            .unwrap_or_default();
                        MediaEntry::from_state(entry, group_ids)
                    })
                    .collect()
            }
            MediaListScope::All => state
                .reconstructed
                .media
                .values()
                .map(|entry| {
                    let group_ids = state
                        .views
                        .media_group_membership
                        .get(&entry.media_id)
                        .cloned()
                        .unwrap_or_default();
                    MediaEntry::from_state(entry, group_ids)
                })
                .collect(),
        }
    }

    /// Returns metadata for `media_id`, whether reachable or not.
    pub fn media_show(&self, media_id: MediaUuid) -> Result<MediaEntry> {
        let state = self.inner.operation_state.read();
        let entry = state
            .reconstructed
            .media
            .get(&media_id)
            .ok_or(LibraryError::MediaNotFound(media_id))?;
        let group_ids = state
            .views
            .media_group_membership
            .get(&media_id)
            .cloned()
            .unwrap_or_default();
        Ok(MediaEntry::from_state(entry, group_ids))
    }

    /// Decrypts the media blob and returns the plaintext bytes.
    ///
    /// If the blob is not locally cached, `storage` is used to download and cache it. Pass `None`
    /// to skip remote download (returns `MediaNotFound` when not cached locally).
    pub async fn media_get_bytes(
        &self,
        media_id: MediaUuid,
        storage: Option<&dyn Storage>,
    ) -> Result<Vec<u8>> {
        let (year, month) = self.media_year_month(media_id)?;

        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let data_path = local_state_media_dir.data_path(year, month, &media_id);

        if data_path.exists() {
            let blob_bytes = std::fs::read(&data_path)?;
            return self.decrypt_media_blob(media_id, &blob_bytes);
        }

        let storage = storage.ok_or(LibraryError::MediaNotFound(media_id))?;
        let blob_bytes = self
            .download_media_blob(media_id, year, month, storage)
            .await?;
        let plaintext = self.decrypt_media_blob(media_id, &blob_bytes)?;

        if let Some(parent) = data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic(&data_path, &blob_bytes)?;

        Ok(plaintext)
    }

    /// Downloads a media blob from a known remote and records that positive observation in the
    /// remote's media inventory after the encrypted blob has been validated and cached locally.
    pub async fn media_get_bytes_from_remote(
        &self,
        media_id: MediaUuid,
        remote_id: &str,
        storage: &dyn Storage,
    ) -> Result<Vec<u8>> {
        let (year, month) = self.media_year_month(media_id)?;
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let data_path = local_state_media_dir.data_path(year, month, &media_id);

        if data_path.exists() {
            let blob_bytes = std::fs::read(&data_path)?;
            return self.decrypt_media_blob(media_id, &blob_bytes);
        }

        let blob_bytes = self
            .download_media_blob(media_id, year, month, storage)
            .await?;
        let plaintext = self.decrypt_media_blob(media_id, &blob_bytes)?;

        if let Some(parent) = data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic(&data_path, &blob_bytes)?;
        self.record_remote_media_presence(remote_id, media_id);

        Ok(plaintext)
    }

    /// Decrypts the media blob and writes plaintext to `path_dest`.
    ///
    /// If the blob is not locally cached, `storage` is used to download it. Pass `None`
    /// to skip remote download (returns `MediaNotFound` when not cached locally).
    pub async fn media_get(
        &self,
        media_id: MediaUuid,
        path_dest: &Path,
        storage: Option<&dyn Storage>,
    ) -> Result<()> {
        let plaintext = self.media_get_bytes(media_id, storage).await?;
        write_dest(path_dest, &plaintext)?;
        Ok(())
    }

    /// Sets or clears the user-facing display name for `media_id`.
    pub async fn media_rename(&self, media_id: MediaUuid, name: Option<MediaName>) -> Result<()> {
        {
            let state = self.inner.operation_state.read();
            if !state.reconstructed.media.contains_key(&media_id) {
                return Err(LibraryError::MediaNotFound(media_id));
            }
        }
        self.append_to_pending(Operation::MediaRename {
            timestamp: Utc::now(),
            media_id,
            name,
        })?;
        self.load_local_state().await?;
        Ok(())
    }

    /// Decrypts the thumbnail blob and returns the plaintext bytes.
    ///
    /// If the thumbnail is not cached locally and `storage` is provided, it is downloaded
    /// from the remote and cached for subsequent calls.
    pub async fn media_get_thumbnail(
        &self,
        media_id: MediaUuid,
        storage: Option<&dyn Storage>,
    ) -> Result<Vec<u8>> {
        let (year, month) = self.media_year_month(media_id)?;
        let thumb_path = self
            .inner
            .local_dirs
            .local_state_media_dir()
            .thumb_path(year, month, &media_id);
        let blob_bytes = match std::fs::read(&thumb_path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let storage = storage.ok_or(LibraryError::MediaNotFound(media_id))?;
                let key = format!("media/{year}/{month:02}/{media_id}.thumb");
                let bytes = storage
                    .get(&key)
                    .await
                    .map_err(|_| LibraryError::MediaNotFound(media_id))?;
                if let Some(parent) = thumb_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                write_atomic(&thumb_path, &bytes)?;
                bytes
            }
            Err(e) => return Err(LibraryError::Io(e)),
        };
        self.decrypt_media_blob(media_id, &blob_bytes)
    }

    /// Returns the plaintext bytes of the full media file from the local cache only.
    /// Returns `MediaNotFound` if the file is not locally cached.
    pub fn media_get_bytes_local(&self, media_id: MediaUuid) -> Result<Vec<u8>> {
        let (year, month) = self.media_year_month(media_id)?;
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let data_path = local_state_media_dir.data_path(year, month, &media_id);

        if !data_path.exists() {
            return Err(LibraryError::MediaNotFound(media_id));
        }
        let blob_bytes = std::fs::read(&data_path)?;
        self.decrypt_media_blob(media_id, &blob_bytes)
    }

    pub(crate) fn media_year_month(&self, media_id: MediaUuid) -> Result<(u16, u8)> {
        let state = self.inner.operation_state.read();
        let entry = state
            .reconstructed
            .media
            .get(&media_id)
            .ok_or(LibraryError::MediaNotFound(media_id))?;
        Ok((entry.storage_date.year, entry.storage_date.month))
    }

    fn decrypt_media_blob(&self, media_id: MediaUuid, blob_bytes: &[u8]) -> Result<Vec<u8>> {
        let blob = BlobEncrypted::from_bytes(blob_bytes)
            .map_err(|e| LibraryError::Operation(OperationError::Blob(e)))?;
        let file_key = derive_blob_key(&self.inner.master_key, &media_id.0);
        decrypt_blob(&file_key, &blob)
            .map_err(|e| LibraryError::Operation(OperationError::Crypto(e)))
    }

    async fn download_media_blob(
        &self,
        media_id: MediaUuid,
        year: u16,
        month: u8,
        storage: &dyn Storage,
    ) -> Result<Vec<u8>> {
        let key = format!("media/{year}/{month:02}/{media_id}.data");
        storage
            .get(&key)
            .await
            .map_err(|_| LibraryError::MediaNotFound(media_id))
    }

    fn record_remote_media_presence(&self, remote_id: &str, media_id: MediaUuid) {
        let remote_media_list = self.inner.local_dirs.remote_media_list(remote_id);
        self.inner.remote_media_list_lock.with_lock(
            remote_id,
            &remote_media_list,
            |remote_media_list| {
                let path = remote_media_list.media_list_path();
                let Ok(mut media_list) = MediaList::load_or_default(&path) else {
                    return;
                };
                if media_list.insert_present(media_id) {
                    let _ = media_list.save(&path);
                }
            },
        );
    }

    /// Returns IDs of all non-deleted albums that directly contain `media_id`.
    pub fn media_album_ids(&self, media_id: MediaUuid) -> Vec<AlbumUuid> {
        let state = self.inner.operation_state.read();
        state
            .reconstructed
            .albums
            .values()
            .filter(|a| !a.deleted && a.media_ids.contains(&media_id))
            .map(|a| a.album_id)
            .collect()
    }

    /// Returns IDs of albums containing `media_id`, directly or (when `include_via_groups`
    /// is true) through a non-deleted group whose parent album is non-deleted.
    pub fn media_containing_album_ids(
        &self,
        media_id: MediaUuid,
        include_via_groups: bool,
    ) -> Vec<AlbumUuid> {
        let state = self.inner.operation_state.read();
        let mut album_ids: Vec<AlbumUuid> = state
            .reconstructed
            .albums
            .values()
            .filter(|a| !a.deleted && a.media_ids.contains(&media_id))
            .map(|a| a.album_id)
            .collect();

        if include_via_groups {
            let group_ids: Vec<GroupUuid> = state
                .views
                .media_group_membership
                .get(&media_id)
                .cloned()
                .unwrap_or_default();
            for group_id in group_ids {
                if let Some(group) = state.reconstructed.groups.get(&group_id) {
                    if group.deleted {
                        continue;
                    }
                    let parent = group.album_id_parent;
                    let parent_alive = state
                        .reconstructed
                        .albums
                        .get(&parent)
                        .is_some_and(|a| !a.deleted);
                    if parent_alive && !album_ids.contains(&parent) {
                        album_ids.push(parent);
                    }
                }
            }
        }

        album_ids
    }

    /// Returns the ids of local media not confirmed by the positive media inventory of any of
    /// `remote_ids`. If `remote_ids` is empty, returns all local media.
    pub fn media_ids_without_remote_backup(&self, remote_ids: &[String]) -> Vec<MediaUuid> {
        let mut backed_up = std::collections::HashSet::new();
        for remote_id in remote_ids {
            let remote_media_list = self.inner.local_dirs.remote_media_list(remote_id);
            if let Ok(list) = self.inner.remote_media_list_lock.with_lock(
                remote_id,
                &remote_media_list,
                |remote_media_list| {
                    MediaList::load_or_default(&remote_media_list.media_list_path())
                },
            ) {
                backed_up.extend(list.media.into_keys());
            }
        }

        self.media_list(MediaListScope::All)
            .into_iter()
            .map(|entry| entry.media_id)
            .filter(|media_id| !backed_up.contains(media_id))
            .collect()
    }

    /// Removes `media_id` from every non-deleted album that contains it.
    /// After this call the media will no longer appear in `media_list(MediaListScope::Reachable)`.
    pub async fn media_delete(&self, media_id: MediaUuid) -> Result<()> {
        let album_ids: Vec<AlbumUuid> = {
            let state = self.inner.operation_state.read();
            state
                .reconstructed
                .albums
                .values()
                .filter(|a| !a.deleted && a.media_ids.contains(&media_id))
                .map(|a| a.album_id)
                .collect()
        };
        for album_id in album_ids {
            self.append_to_pending(Operation::AlbumMediaRemove {
                timestamp: chrono::Utc::now(),
                album_id,
                media_id,
            })?;
            self.load_local_state().await?;
        }
        Ok(())
    }
}

fn inclusive_slice<T>(items: &[T], start: usize, end: usize) -> Option<&[T]> {
    if start > end || start >= items.len() {
        return None;
    }
    items.get(start..=end.min(items.len() - 1))
}

fn write_dest(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use crate::identifiers::LibraryId;
    use crate::library::Credentials;
    use crate::library::local_dirs::LocalDirs;
    use crate::operations::MediaFilename;
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::super::upload::MediaAddSource;
    use super::*;

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

    async fn add_media_to_album(
        lib: &Library,
        tmp: &TempDir,
        name: &str,
        content: &[u8],
    ) -> (MediaUuid, AlbumUuid) {
        use crate::operations::AlbumName;
        let album_id = lib
            .album_create(AlbumName("Test Album".into()), None)
            .await
            .unwrap();
        let src = tmp.path().join(name);
        std::fs::write(&src, content).unwrap();
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
        (media_id, album_id)
    }

    #[tokio::test]
    async fn media_list_includes_added_media() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let (media_id, _) = add_media_to_album(&lib, &tmp, "photo.jpg", b"data").await;

        let list = lib.media_list(MediaListScope::Reachable);
        assert!(list.iter().any(|e| e.media_id == media_id));
    }

    #[tokio::test]
    async fn media_by_date_range_is_counted_and_non_overlapping() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let (first_id, _) = add_media_to_album(&lib, &tmp, "first.jpg", b"first").await;
        let (second_id, _) = add_media_to_album(&lib, &tmp, "second.jpg", b"second").await;

        assert_eq!(lib.media_by_date_count(false), 2);
        let first = lib.media_by_date_range(false, 0, 0);
        let second = lib.media_by_date_range(false, 1, 1);
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].media_id, second[0].media_id);
        assert!(matches!(first[0].media_id, id if id == first_id || id == second_id));
        assert!(matches!(second[0].media_id, id if id == first_id || id == second_id));
        assert!(lib.media_by_date_range(false, 2, 3).is_empty());
    }

    #[tokio::test]
    async fn media_list_excludes_removed_media_but_media_show_succeeds() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let (media_id, album_id) = add_media_to_album(&lib, &tmp, "photo.jpg", b"data").await;

        lib.album_remove_media(album_id, media_id).await.unwrap();

        let list = lib.media_list(MediaListScope::Reachable);
        assert!(
            !list.iter().any(|e| e.media_id == media_id),
            "must not be in list"
        );

        let shown = lib.media_show(media_id).unwrap();
        assert_eq!(
            shown.media_id, media_id,
            "media_show must still return metadata"
        );
    }

    #[tokio::test]
    async fn orphaned_media_list_includes_only_primary_unreachable_media() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let (reachable_id, album_id) =
            add_media_to_album(&lib, &tmp, "reachable.jpg", b"reachable").await;
        let orphan_source = tmp.path().join("orphan.jpg");
        std::fs::write(&orphan_source, b"orphan").unwrap();
        let orphan_id = lib
            .media_add(
                MediaAddSource::CopyFrom(orphan_source),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .id();

        let companion_source = tmp.path().join("companion.mov");
        std::fs::write(&companion_source, b"companion").unwrap();
        let companion_id = lib
            .media_add(
                MediaAddSource::CopyFrom(companion_source),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .id();
        let primary_source = tmp.path().join("primary.jpg");
        std::fs::write(&primary_source, b"primary").unwrap();
        let primary_id = lib
            .media_add(
                MediaAddSource::CopyFrom(primary_source),
                Some(album_id),
                None,
                None,
                Some(companion_id),
            )
            .await
            .unwrap()
            .id();

        let orphaned = lib.media_list(MediaListScope::Orphaned);
        assert!(orphaned.iter().any(|entry| entry.media_id == orphan_id));
        assert!(!orphaned.iter().any(|entry| entry.media_id == reachable_id));
        assert!(!orphaned.iter().any(|entry| entry.media_id == primary_id));
        assert!(!orphaned.iter().any(|entry| entry.media_id == companion_id));

        let visible = lib.media_list(MediaListScope::Visible);
        assert!(visible.iter().any(|entry| entry.media_id == reachable_id));
        assert!(visible.iter().any(|entry| entry.media_id == orphan_id));
        assert!(visible.iter().any(|entry| entry.media_id == primary_id));
        assert!(!visible.iter().any(|entry| entry.media_id == companion_id));

        lib.album_add_media(album_id, orphan_id).await.unwrap();
        assert!(
            !lib.media_list(MediaListScope::Orphaned)
                .iter()
                .any(|entry| entry.media_id == orphan_id)
        );

        lib.album_remove_media(album_id, orphan_id).await.unwrap();
        assert!(
            lib.media_list(MediaListScope::Orphaned)
                .iter()
                .any(|entry| entry.media_id == orphan_id)
        );
    }

    #[tokio::test]
    async fn media_get_cache_hit_decrypts_correctly() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let content = b"original photo content";
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", content).await;

        let dest = tmp.path().join("out.jpg");
        lib.media_get(media_id, &dest, None).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), content);
    }

    #[tokio::test]
    async fn media_get_thumbnail_decrypts_correctly() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"photo data").await;

        let thumb_data = vec![0u8; 64];
        lib.media_set_thumbnail(media_id, &thumb_data).unwrap();

        let thumb_bytes = lib.media_get_thumbnail(media_id, None).await.unwrap();
        assert_eq!(thumb_bytes, thumb_data);
    }

    #[tokio::test]
    async fn media_show_works_on_unreachable_media() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let (media_id, album_id) = add_media_to_album(&lib, &tmp, "img.jpg", b"data").await;

        lib.album_remove_media(album_id, media_id).await.unwrap();

        assert!(
            !lib.media_list(MediaListScope::Reachable)
                .iter()
                .any(|e| e.media_id == media_id)
        );
        let entry = lib.media_show(media_id).unwrap();
        assert_eq!(entry.media_id, media_id);
        assert_eq!(entry.filename_original, MediaFilename("img.jpg".into()));
    }

    #[tokio::test]
    async fn media_rename_sets_and_clears_name() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"data").await;

        let before = lib.media_show(media_id).unwrap();
        assert_eq!(before.name, None);

        lib.media_rename(media_id, Some(MediaName("Holiday".into())))
            .await
            .unwrap();
        let renamed = lib.media_show(media_id).unwrap();
        assert_eq!(renamed.name, Some(MediaName("Holiday".into())));

        lib.media_rename(media_id, None).await.unwrap();
        let cleared = lib.media_show(media_id).unwrap();
        assert_eq!(cleared.name, None);
    }

    #[tokio::test]
    async fn media_get_works_on_unreachable_media() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let content = b"secret bytes";
        let (media_id, album_id) = add_media_to_album(&lib, &tmp, "img.jpg", content).await;

        lib.album_remove_media(album_id, media_id).await.unwrap();

        let dest = tmp.path().join("out.jpg");
        lib.media_get(media_id, &dest, None).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), content);
    }
}
