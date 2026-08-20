use std::path::Path;

use chrono::Utc;

use crate::crdt::OperationContent;
use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob::decrypt_blob;
use crate::encryption::blob_key::derive_blob_key;
use crate::error::{LibraryError, OperationError};
use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::library::Library;
use crate::library::range::inclusive_slice;
use crate::operations::MediaName;
use crate::remote::MediaList;
use crate::storage::Storage;

use super::MediaEntry;

pub type Result<T> = std::result::Result<T, LibraryError>;

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

/// Selects what counts as a home for a media blob in `Library::media_ids_without_backup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupScope {
    /// Only a remote counts. Used before clearing local media, where the local copy is
    /// exactly what is about to be deleted.
    RemotesOnly,
    /// A remote or this device's local state counts. Used before removing a remote, where
    /// the local copy survives the operation.
    RemotesOrLocal,
}

impl Library {
    /// Returns primary visible media ordered by date descending, then ID descending.
    #[must_use]
    pub fn media_by_date_count(&self, orphaned: bool) -> usize {
        let state = self.inner.state.read();
        if orphaned {
            state.views.home_orphaned_newest.len()
        } else {
            state.views.home_visible_newest.len()
        }
    }

    /// Returns the inclusive position range from the canonical home-media order.
    #[must_use]
    pub fn media_by_date_range(
        &self,
        orphaned: bool,
        pos_start_inclusive: usize,
        pos_end_inclusive: usize,
    ) -> Vec<MediaEntry> {
        if pos_start_inclusive > pos_end_inclusive {
            return Vec::new();
        }
        let state = self.inner.state.read();
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
                state
                    .media(*media_id)
                    .map(|entry| MediaEntry::from_state(&entry, entry.group_ids.clone()))
            })
            .collect()
    }

    /// Returns media entries matching `scope`.
    #[must_use]
    pub fn media_list(&self, scope: MediaListScope) -> Vec<MediaEntry> {
        let state = self.inner.state.read();
        match scope {
            MediaListScope::Reachable => state
                .views
                .reachable_media_ids
                .iter()
                .filter_map(|&media_id| {
                    state
                        .media(media_id)
                        .map(|entry| MediaEntry::from_state(&entry, entry.group_ids.clone()))
                })
                .collect(),
            MediaListScope::Visible => state
                .media_entries()
                .iter()
                .filter(|entry| entry.companion_kind.is_none())
                .map(|entry| MediaEntry::from_state(entry, entry.group_ids.clone()))
                .collect(),
            MediaListScope::Orphaned => state
                .media_entries()
                .iter()
                .filter(|entry| {
                    entry.companion_kind.is_none()
                        && !state.views.reachable_media_ids.contains(&entry.media_id)
                })
                .map(|entry| MediaEntry::from_state(entry, entry.group_ids.clone()))
                .collect(),
            MediaListScope::All => state
                .media_entries()
                .iter()
                .map(|entry| MediaEntry::from_state(entry, entry.group_ids.clone()))
                .collect(),
        }
    }

    /// Returns metadata for `media_id`, whether reachable or not.
    /// # Errors
    ///
    /// Returns an error if the media does not exist in the reconstructed state.
    pub fn media_show(&self, media_id: MediaUuid) -> Result<MediaEntry> {
        let state = self.inner.state.read();
        let entry = state
            .media(media_id)
            .ok_or(LibraryError::MediaNotFound(media_id))?;
        Ok(MediaEntry::from_state(&entry, entry.group_ids.clone()))
    }

    /// Decrypts the media blob and returns the plaintext bytes.
    ///
    /// If the blob is not locally cached, `storage` is used to download and cache it. Pass `None`
    /// to skip remote download (returns `MediaNotFound` when not cached locally).
    /// # Errors
    ///
    /// Returns an error if media is absent locally and from the optional remote, or if blob I/O, decryption, or cache writes fail.
    pub async fn media_get_bytes(
        &self,
        media_id: MediaUuid,
        storage: Option<&dyn Storage>,
    ) -> Result<Vec<u8>> {
        self.media_get_bytes_from_storage(media_id, storage).await
    }

    async fn media_get_bytes_from_storage(
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
        crate::atomic_file::write(&data_path, &blob_bytes)?;

        Ok(plaintext)
    }

    /// Decrypts the media blob and writes plaintext to `path_dest`.
    ///
    /// If the blob is not locally cached, `storage` is used to download it. Pass `None`
    /// to skip remote download (returns `MediaNotFound` when not cached locally).
    #[allow(
        dead_code,
        reason = "Retained for filesystem-destination media exports and unit tests."
    )]
    async fn media_get(
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
    /// # Errors
    ///
    /// Returns an error if media is absent or the rename operation cannot be persisted.
    pub async fn media_rename(&self, media_id: MediaUuid, name: Option<MediaName>) -> Result<()> {
        {
            let state = self.inner.state.read();
            if state.media(media_id).is_none() {
                return Err(LibraryError::MediaNotFound(media_id));
            }
        }
        self.record_local_operation(Utc::now(), OperationContent::MediaRename { media_id, name })?;
        Ok(())
    }

    /// Decrypts the thumbnail blob and returns the plaintext bytes.
    ///
    /// If the thumbnail is not cached locally and `storage` is provided, it is downloaded
    /// from the remote and cached for subsequent calls.
    /// # Errors
    ///
    /// Returns an error if the thumbnail is unavailable locally and from the optional remote, or if blob I/O, decryption, or cache writes fail.
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
                // A companion resource is never given a thumbnail, so there is nothing to
                // fetch and the remote is left alone.
                if self.media_is_companion(media_id) {
                    return Err(LibraryError::MediaNotFound(media_id));
                }
                let storage = storage.ok_or(LibraryError::MediaNotFound(media_id))?;
                let key = format!("media/{year}/{month:02}/{media_id}.thumb");
                let bytes = storage
                    .get(&key)
                    .await
                    .map_err(|_remote_media_error| LibraryError::MediaNotFound(media_id))?;
                if let Some(parent) = thumb_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                crate::atomic_file::write(&thumb_path, &bytes)?;
                bytes
            }
            Err(e) => return Err(LibraryError::Io(e)),
        };
        self.decrypt_media_blob(media_id, &blob_bytes)
    }

    /// Returns the plaintext bytes of the full media file from the local cache only.
    /// Returns `MediaNotFound` if the file is not locally cached.
    /// # Errors
    ///
    /// Returns an error if the local media blob is absent, unreadable, or fails authentication/decryption.
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

    /// Whether another media references this one as its companion resource.
    #[must_use]
    pub fn media_is_companion(&self, media_id: MediaUuid) -> bool {
        let state = self.inner.state.read();
        state
            .media(media_id)
            .is_some_and(|entry| entry.companion_kind.is_some())
    }

    pub(crate) fn media_year_month(&self, media_id: MediaUuid) -> Result<(u16, u8)> {
        let state = self.inner.state.read();
        let entry = state
            .media(media_id)
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
            .map_err(|_remote_media_error| LibraryError::MediaNotFound(media_id))
    }

    /// Returns IDs of all non-deleted albums that directly contain `media_id`.
    #[must_use]
    pub fn media_album_ids(&self, media_id: MediaUuid) -> Vec<AlbumUuid> {
        let state = self.inner.state.read();
        state
            .album_entries()
            .iter()
            .filter(|album| album.media_ids.contains(&media_id))
            .map(|a| a.album_id)
            .collect()
    }

    /// Returns IDs of albums containing `media_id`, directly or (when `include_via_groups`
    /// is true) through a non-deleted group whose parent album is non-deleted.
    #[must_use]
    pub fn media_containing_album_ids(
        &self,
        media_id: MediaUuid,
        include_via_groups: bool,
    ) -> Vec<AlbumUuid> {
        let state = self.inner.state.read();
        let mut album_ids: Vec<AlbumUuid> = state
            .album_entries()
            .iter()
            .filter(|album| album.media_ids.contains(&media_id))
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
                if let Some(group) = state.group(group_id) {
                    let parent = group.album_id_parent;
                    let parent_alive = state.album(parent).is_some();
                    if parent_alive && !album_ids.contains(&parent) {
                        album_ids.push(parent);
                    }
                }
            }
        }

        album_ids
    }

    /// Whether the full media blob is confirmed present on any of `remote_ids`, according to
    /// the media list this client has cached for each of them.
    fn media_full_backed_by_remotes(
        &self,
        remote_ids: &[String],
    ) -> std::collections::HashSet<MediaUuid> {
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
                backed_up.extend(
                    list.media
                        .into_iter()
                        .filter(|(_, entry)| entry.full.is_some())
                        .map(|(media_id, _)| media_id),
                );
            }
        }
        backed_up
    }

    /// Whether the full media blob is present in this device's local state.
    fn media_full_cached_locally(&self, media_id: MediaUuid) -> bool {
        let Ok((year, month)) = self.media_year_month(media_id) else {
            return false;
        };
        self.inner
            .local_dirs
            .local_state_media_dir()
            .data_path(year, month, &media_id)
            .exists()
    }

    /// Returns the ids of media whose full blob has no known home once `scope` is applied.
    ///
    /// Only the full media file counts, a thumbnail alone is never a backup. Remote knowledge
    /// comes from the media list json this client cached for each remote at its last update,
    /// never from a live listing, so a media confirmed nowhere may still sit on a remote this
    /// client has not talked to recently. The answer is therefore an upper bound on what could
    /// be lost, never an under-count.
    ///
    /// Every known media entry is considered, companion resources and album-orphaned media
    /// included, because both are real blobs that a caller can really lose.
    #[must_use]
    pub fn media_ids_without_backup(
        &self,
        remote_ids: &[String],
        scope: BackupScope,
    ) -> Vec<MediaUuid> {
        let backed_up = self.media_full_backed_by_remotes(remote_ids);

        self.media_list(MediaListScope::All)
            .into_iter()
            .map(|entry| entry.media_id)
            .filter(|media_id| !backed_up.contains(media_id))
            .filter(|&media_id| match scope {
                BackupScope::RemotesOnly => true,
                BackupScope::RemotesOrLocal => !self.media_full_cached_locally(media_id),
            })
            .collect()
    }

    /// Removes `media_id` from every non-deleted album that contains it.
    /// After this call the media will no longer appear in `media_list(MediaListScope::Reachable)`.
    /// # Errors
    ///
    /// Returns an error if media is absent or the delete operation cannot be persisted.
    pub async fn media_delete(&self, media_id: MediaUuid) -> Result<()> {
        let album_ids: Vec<AlbumUuid> = {
            let state = self.inner.state.read();
            state
                .album_entries()
                .iter()
                .filter(|album| album.media_ids.contains(&media_id))
                .map(|a| a.album_id)
                .collect()
        };
        for album_id in album_ids {
            let observed = self
                .inner
                .state
                .read()
                .album_member_dots(album_id, media_id);
            self.record_local_operation(
                chrono::Utc::now(),
                OperationContent::AlbumMediaRemove {
                    album_id,
                    media_id,
                    observed,
                },
            )?;
        }
        Ok(())
    }
}

#[allow(
    dead_code,
    reason = "Used by the retained filesystem-destination media export helper."
)]
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
    use crate::storage::{AtomicWriteMode, Storage, StorageMockMemory};
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::super::upload::MediaAddSource;
    use super::*;

    fn make_library(tmp: &TempDir) -> (Library, LocalDirs) {
        use crate::operations::{LibraryPassword, LibraryUsername};
        let library_id = LibraryId(Uuid::new_v4());
        let local_dirs = LocalDirs::new(tmp.path(), &library_id);
        local_dirs.ensure_state_dirs().unwrap();
        let (lib, _password_uuid) = Library::init(
            local_dirs.clone(),
            library_id,
            crate::crdt::DeviceId(1),
            Credentials {
                username: LibraryUsername("alice".into()),
                password: LibraryPassword("pass".into()),
            },
        )
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
        let (lib, _) = make_library(&tmp);
        let (media_id, _) = add_media_to_album(&lib, &tmp, "photo.jpg", b"data").await;

        let list = lib.media_list(MediaListScope::Reachable);
        assert!(list.iter().any(|e| e.media_id == media_id));
    }

    #[tokio::test]
    async fn media_by_date_range_is_counted_and_non_overlapping() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
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
        let (lib, _) = make_library(&tmp);
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
        let (lib, _) = make_library(&tmp);
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

    /// Records `media_id` as fully present on `remote_id` in that remote's cached media list.
    fn record_full_on_remote(lib: &Library, remote_id: &str, media_id: MediaUuid) {
        let remote_media_list = lib.inner.local_dirs.remote_media_list(remote_id);
        let path = remote_media_list.media_list_path();
        let mut list = MediaList::load_or_default(&path).unwrap();
        list.record(media_id, true, false);
        list.save(&path).unwrap();
    }

    fn evict_local_full(lib: &Library, media_id: MediaUuid) {
        let entry = lib.media_show(media_id).unwrap();
        let path = lib.inner.local_dirs.local_state_media_dir().data_path(
            entry.storage_date.year,
            entry.storage_date.month,
            &media_id,
        );
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn without_backup_ignores_a_thumbnail_only_confirmation() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"photo data").await;

        let remote_media_list = lib.inner.local_dirs.remote_media_list("remote-a");
        let path = remote_media_list.media_list_path();
        let mut list = MediaList::load_or_default(&path).unwrap();
        list.record(media_id, false, true);
        list.save(&path).unwrap();

        let remotes = vec!["remote-a".to_string()];
        assert_eq!(
            lib.media_ids_without_backup(&remotes, BackupScope::RemotesOnly),
            vec![media_id]
        );
    }

    #[tokio::test]
    async fn without_backup_clears_once_a_remote_confirms_the_full_blob() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"photo data").await;
        record_full_on_remote(&lib, "remote-a", media_id);

        let remotes = vec!["remote-a".to_string()];
        assert!(
            lib.media_ids_without_backup(&remotes, BackupScope::RemotesOnly)
                .is_empty()
        );
    }

    #[tokio::test]
    async fn without_backup_counts_everything_when_no_remote_is_configured() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"photo data").await;
        record_full_on_remote(&lib, "remote-a", media_id);

        // A remote that is not in the list contributes nothing, which is what makes removing
        // the last remote report the whole library.
        assert_eq!(
            lib.media_ids_without_backup(&[], BackupScope::RemotesOnly),
            vec![media_id]
        );
    }

    // The local copy is what clearing local media deletes, so it can never stand in for a
    // backup there. Removing a remote leaves it alone, so there it counts.
    #[tokio::test]
    async fn local_copy_counts_as_a_home_only_for_the_remote_removal_scope() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"photo data").await;

        assert_eq!(
            lib.media_ids_without_backup(&[], BackupScope::RemotesOnly),
            vec![media_id]
        );
        assert!(
            lib.media_ids_without_backup(&[], BackupScope::RemotesOrLocal)
                .is_empty()
        );

        evict_local_full(&lib, media_id);
        assert_eq!(
            lib.media_ids_without_backup(&[], BackupScope::RemotesOrLocal),
            vec![media_id]
        );
    }

    // Companion resources and album-orphaned media are real blobs a user can really lose, so
    // both are counted even though neither shows up in the visible media list.
    #[tokio::test]
    async fn without_backup_counts_companion_and_orphaned_media() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);

        let video_src = tmp.path().join("live.mov");
        std::fs::write(&video_src, b"motion").unwrap();
        let video_id = lib
            .media_add(MediaAddSource::CopyFrom(video_src), None, None, None, None)
            .await
            .unwrap()
            .id();
        let still_src = tmp.path().join("live.jpg");
        std::fs::write(&still_src, b"pixels").unwrap();
        let still_id = lib
            .media_add(
                MediaAddSource::CopyFrom(still_src),
                None,
                None,
                None,
                Some(video_id),
            )
            .await
            .unwrap()
            .id();

        let unbacked = lib.media_ids_without_backup(&[], BackupScope::RemotesOnly);
        assert!(unbacked.contains(&video_id), "companion must be counted");
        assert!(unbacked.contains(&still_id), "orphan must be counted");
        assert_eq!(unbacked.len(), 2);
        assert!(
            lib.media_list(MediaListScope::Visible)
                .iter()
                .all(|entry| entry.media_id != video_id),
            "the companion is invisible in the media list, which is why the count looks high"
        );
    }

    #[tokio::test]
    async fn media_get_cache_hit_decrypts_correctly() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let content = b"original photo content";
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", content).await;

        let dest = tmp.path().join("out.jpg");
        lib.media_get(media_id, &dest, None).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), content);
    }

    #[tokio::test]
    async fn original_download_does_not_record_source_media_presence() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"photo data").await;
        let entry = lib.media_show(media_id).unwrap();
        let media_dir = lib.inner.local_dirs.local_state_media_dir();
        let data_path =
            media_dir.data_path(entry.storage_date.year, entry.storage_date.month, &media_id);
        let encrypted = std::fs::read(&data_path).unwrap();
        let storage = StorageMockMemory::new();
        let key = format!(
            "media/{}/{:02}/{}.data",
            entry.storage_date.year, entry.storage_date.month, media_id
        );
        storage
            .put_atomic(&key, &encrypted, AtomicWriteMode::Replace)
            .await
            .unwrap();
        std::fs::remove_file(&data_path).unwrap();

        assert_eq!(
            lib.media_get_bytes(media_id, Some(&storage)).await.unwrap(),
            b"photo data"
        );

        // Downloading a blob to display it is not a sync procedure, so it leaves the
        // remote inventory untouched. Fetch and push are the only writers.
        let media_list = MediaList::load_or_default(
            &lib.inner
                .local_dirs
                .remote_media_list("source-remote")
                .media_list_path(),
        )
        .unwrap();
        assert!(!media_list.has_full(&media_id));
    }

    #[tokio::test]
    async fn media_get_thumbnail_decrypts_correctly() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let (media_id, _) = add_media_to_album(&lib, &tmp, "img.jpg", b"photo data").await;

        let thumb_data = vec![0u8; 64];
        lib.media_set_thumbnail(media_id, &thumb_data).unwrap();

        let thumb_bytes = lib.media_get_thumbnail(media_id, None).await.unwrap();
        assert_eq!(thumb_bytes, thumb_data);
    }

    #[tokio::test]
    // A companion resource is never given a thumbnail, so asking for one must fail on the
    // spot instead of reaching for a remote that cannot hold it.
    async fn media_get_thumbnail_on_a_companion_never_touches_the_remote() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
        let storage = StorageMockMemory::new();
        let video_src = tmp.path().join("live.mov");
        std::fs::write(&video_src, b"motion").unwrap();
        let video_id = lib
            .media_add(MediaAddSource::CopyFrom(video_src), None, None, None, None)
            .await
            .unwrap()
            .id();
        let still_src = tmp.path().join("live.jpg");
        std::fs::write(&still_src, b"pixels").unwrap();
        lib.media_add(
            MediaAddSource::CopyFrom(still_src),
            None,
            None,
            None,
            Some(video_id),
        )
        .await
        .unwrap();

        let gets_before = storage.get_call_count();
        let error = lib
            .media_get_thumbnail(video_id, Some(&storage))
            .await
            .unwrap_err();

        assert!(matches!(error, LibraryError::MediaNotFound(id) if id == video_id));
        assert_eq!(
            storage.get_call_count(),
            gets_before,
            "a companion thumbnail must never be requested from a remote"
        );
    }

    #[tokio::test]
    async fn media_show_works_on_unreachable_media() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp);
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
        let (lib, _) = make_library(&tmp);
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
        let (lib, _) = make_library(&tmp);
        let content = b"secret bytes";
        let (media_id, album_id) = add_media_to_album(&lib, &tmp, "img.jpg", content).await;

        lib.album_remove_media(album_id, media_id).await.unwrap();

        let dest = tmp.path().join("out.jpg");
        lib.media_get(media_id, &dest, None).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), content);
    }
}
