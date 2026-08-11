use lasco_core::library_json::LibraryJson;

use lasco_core::library::media::upload::MediaAddResult;

use super::remotes::media_entry_to_ffi;
use super::types::{FfiLocalStateStats, FfiMediaAddResult, FfiMediaNeighbors};
use super::{FfiLibrary, FfiMediaItem, ffi_count};
use crate::error::LascoError;
use crate::ids::{FfiAlbumUuid, FfiLibraryId, FfiMediaUuid, FfiRemoteUuid};

pub(super) fn inclusive_range(start: u32, end: u32) -> Result<(usize, usize), LascoError> {
    if start > end {
        return Err(LascoError::Other {
            msg: "pos_start_inclusive must not exceed pos_end_inclusive".to_string(),
        });
    }
    Ok((start as usize, end as usize))
}

#[uniffi::export]
impl FfiLibrary {
    /// # Errors
    ///
    /// Returns an error when rebuilding the local state from persisted operations fails.
    pub fn load_local_state(&self) -> Result<(), LascoError> {
        self.rt
            .block_on(self.inner.load_local_state())
            .map_err(Into::into)
    }

    pub fn library_id(&self) -> FfiLibraryId {
        self.inner.library_id().into()
    }

    pub fn get_default_fetch_remote(&self) -> Option<FfiRemoteUuid> {
        let library_id = self.inner.library_id();
        let lib_config = LibraryJson::load(&self.app_dir, &library_id).ok()??;
        lib_config
            .default_fetch_remote
            .as_ref()
            .copied()
            .map(Into::into)
    }

    /// # Errors
    ///
    /// Returns an error if the library config cannot be read or saved, or `remote_id` is invalid or unconfigured.
    pub fn set_default_fetch_remote(
        &self,
        remote_id: Option<FfiRemoteUuid>,
    ) -> Result<(), LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config =
            LibraryJson::load(&self.app_dir, &library_id)?.ok_or(LascoError::NotFound)?;
        let remote_uuid = remote_id.map(TryInto::try_into).transpose()?;
        if let Some(remote_uuid) = remote_uuid
            && !lib_config
                .remotes
                .iter()
                .any(|remote| remote.remote_uuid == remote_uuid)
        {
            return Err(LascoError::Other {
                msg: format!("remote '{remote_uuid}' not found"),
            });
        }
        lib_config.default_fetch_remote = remote_uuid;
        lib_config.save(&self.app_dir, &library_id)?;
        Ok(())
    }

    pub fn get_auto_import_device_media(&self) -> bool {
        let library_id = self.inner.library_id();
        LibraryJson::load(&self.app_dir, &library_id)
            .ok()
            .flatten()
            .is_some_and(|c| c.auto_import_device_media)
    }

    /// # Errors
    ///
    /// Returns an error if the library configuration is missing, malformed, or cannot be saved.
    pub fn set_auto_import_device_media(&self, enabled: bool) -> Result<(), LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config =
            LibraryJson::load(&self.app_dir, &library_id)?.ok_or(LascoError::NotFound)?;
        lib_config.auto_import_device_media = enabled;
        lib_config.save(&self.app_dir, &library_id)?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if `media_id` is invalid or the local thumbnail cannot be written.
    pub fn set_media_thumbnail(
        &self,
        media_id: FfiMediaUuid,
        data: Vec<u8>,
    ) -> Result<(), LascoError> {
        let media_uuid = media_id.try_into()?;
        self.inner
            .media_set_thumbnail(media_uuid, &data)
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error if `media_id` is invalid, absent, or the rename operation cannot be persisted.
    pub fn rename_media(
        &self,
        media_id: FfiMediaUuid,
        name: Option<String>,
    ) -> Result<(), LascoError> {
        let media_uuid = media_id.try_into()?;
        let name = name.map(lasco_core::operations::MediaName);
        self.rt
            .block_on(self.inner.media_rename(media_uuid, name))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error if the ID is invalid, no local or configured remote copy is available, or reading, decrypting, or caching it fails.
    pub fn get_media_thumbnail(
        &self,
        media_id: FfiMediaUuid,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = media_id.try_into()?;
        match self
            .rt
            .block_on(self.inner.media_get_thumbnail(media_uuid, None))
        {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let mut last_error = None;
                for remote_id in self.media_fetch_remote_ids()? {
                    let storage = match self
                        .build_storage_for_remote(&remote_id, app_support_dir.as_deref())
                    {
                        Ok(storage) => storage,
                        Err(error) => {
                            last_error = Some(error);
                            continue;
                        }
                    };
                    match self.rt.block_on(
                        self.inner
                            .media_get_thumbnail(media_uuid, Some(storage.as_ref())),
                    ) {
                        Ok(bytes) => return Ok(bytes),
                        Err(error) => last_error = Some(LascoError::from(error)),
                    }
                }
                Err(last_error.unwrap_or(LascoError::NotFound))
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    /// # Errors
    ///
    /// Returns an error if the ID is invalid, no local or configured remote copy is available, or reading, decrypting, or caching it fails.
    pub fn get_media_bytes(
        &self,
        media_id: FfiMediaUuid,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = media_id.try_into()?;
        match self
            .rt
            .block_on(self.inner.media_get_bytes(media_uuid, None))
        {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let mut last_error = None;
                for remote_id in self.media_fetch_remote_ids()? {
                    let storage = match self
                        .build_storage_for_remote(&remote_id, app_support_dir.as_deref())
                    {
                        Ok(storage) => storage,
                        Err(error) => {
                            last_error = Some(error);
                            continue;
                        }
                    };
                    match self.rt.block_on(
                        self.inner
                            .media_get_bytes(media_uuid, Some(storage.as_ref())),
                    ) {
                        Ok(bytes) => return Ok(bytes),
                        Err(error) => last_error = Some(LascoError::from(error)),
                    }
                }
                Err(last_error.unwrap_or(LascoError::NotFound))
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    /// # Errors
    ///
    /// Returns an error if the ID is invalid, no local or configured remote thumbnail is available, or a remote read or cache write fails.
    pub async fn get_media_thumbnail_async(
        &self,
        media_id: FfiMediaUuid,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = media_id.try_into()?;
        match self.inner.media_get_thumbnail(media_uuid, None).await {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let mut last_error = None;
                for remote_id in self.media_fetch_remote_ids()? {
                    let storage = match self
                        .build_storage_for_remote(&remote_id, app_support_dir.as_deref())
                    {
                        Ok(storage) => storage,
                        Err(error) => {
                            last_error = Some(error);
                            continue;
                        }
                    };
                    let inner = self.inner.clone();
                    match self
                        .rt
                        .spawn(async move {
                            inner
                                .media_get_thumbnail(media_uuid, Some(storage.as_ref()))
                                .await
                        })
                        .await
                    {
                        Ok(Ok(bytes)) => return Ok(bytes),
                        Ok(Err(error)) => last_error = Some(LascoError::from(error)),
                        Err(error) => {
                            last_error = Some(LascoError::Other {
                                msg: error.to_string(),
                            });
                        }
                    }
                }
                Err(last_error.unwrap_or(LascoError::NotFound))
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    /// # Errors
    ///
    /// Returns an error if the ID is invalid, no local or configured remote blob is available, or a remote read, decryption, or cache write fails.
    pub async fn get_media_bytes_async(
        &self,
        media_id: FfiMediaUuid,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = media_id.try_into()?;
        match self.inner.media_get_bytes(media_uuid, None).await {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let mut last_error = None;
                for remote_id in self.media_fetch_remote_ids()? {
                    let storage = match self
                        .build_storage_for_remote(&remote_id, app_support_dir.as_deref())
                    {
                        Ok(storage) => storage,
                        Err(error) => {
                            last_error = Some(error);
                            continue;
                        }
                    };
                    let inner = self.inner.clone();
                    match self
                        .rt
                        .spawn(async move {
                            inner
                                .media_get_bytes(media_uuid, Some(storage.as_ref()))
                                .await
                        })
                        .await
                    {
                        Ok(Ok(bytes)) => return Ok(bytes),
                        Ok(Err(error)) => last_error = Some(LascoError::from(error)),
                        Err(error) => {
                            last_error = Some(LascoError::Other {
                                msg: error.to_string(),
                            });
                        }
                    }
                }
                Err(last_error.unwrap_or(LascoError::NotFound))
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    /// # Errors
    ///
    /// Returns an error if an ID is invalid, the source cannot be read, media encryption/storage fails, or the creation operation cannot be persisted.
    pub fn import_media(
        &self,
        path: String,
        album_id: Option<FfiAlbumUuid>,
        original_filename: Option<String>,
        apple_aae_media_id: Option<FfiMediaUuid>,
        apple_live_photo_media_id: Option<FfiMediaUuid>,
    ) -> Result<FfiMediaAddResult, LascoError> {
        let album_uuid = album_id.map(TryInto::try_into).transpose()?;
        let apple_aae_media_uuid = apple_aae_media_id.map(TryInto::try_into).transpose()?;
        let apple_live_photo_media_uuid = apple_live_photo_media_id
            .map(TryInto::try_into)
            .transpose()?;
        let source = lasco_core::library::media::upload::MediaAddSource::CopyFrom(
            std::path::PathBuf::from(path),
        );
        let result = self
            .rt
            .block_on(self.inner.media_add(
                source,
                album_uuid,
                original_filename,
                apple_aae_media_uuid,
                apple_live_photo_media_uuid,
            ))
            .map_err(LascoError::from)?;
        Ok(match result {
            MediaAddResult::Added(id) => FfiMediaAddResult {
                media_id: id.into(),
                already_existed: false,
            },
            MediaAddResult::AlreadyExists(id) => FfiMediaAddResult {
                media_id: id.into(),
                already_existed: true,
            },
        })
    }

    pub fn pending_media_count(&self) -> u64 {
        ffi_count(self.inner.pending_media_count().unwrap_or(0))
    }

    pub fn has_unpushed_changes(&self, remote_id: FfiRemoteUuid) -> bool {
        self.inner
            .has_unpushed_changes(&remote_id.value.clone())
            .unwrap_or(false)
    }

    /// # Errors
    ///
    /// This method currently cannot fail; the `Result` preserves the FFI query API.
    pub fn list_media(&self) -> Result<Vec<FfiMediaItem>, LascoError> {
        Ok(self
            .inner
            .media_list(lasco_core::library::media::query::MediaListScope::Reachable)
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    /// # Errors
    ///
    /// Returns an error if `media_id` is invalid or does not identify media in the local state.
    pub fn show_media(&self, media_id: FfiMediaUuid) -> Result<FfiMediaItem, LascoError> {
        let media_uuid = media_id.try_into()?;
        let entry = self
            .inner
            .media_show(media_uuid)
            .map_err(LascoError::from)?;
        Ok(media_entry_to_ffi(entry))
    }

    /// # Errors
    ///
    /// This method currently cannot fail; the `Result` preserves the FFI query API.
    pub fn media_by_date(&self) -> Result<Vec<FfiMediaItem>, LascoError> {
        let count = self.inner.media_by_date_count(false);
        Ok(self
            .inner
            .media_by_date_range(false, 0, count.saturating_sub(1))
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    pub fn media_by_date_count(&self) -> u64 {
        ffi_count(self.inner.media_by_date_count(false))
    }

    /// Returns the entries immediately surrounding a zero-based home position.
    ///
    /// # Errors
    ///
    /// Returns an error when `position` is outside the dated-media list.
    pub fn media_by_date_neighbors(&self, position: u32) -> Result<FfiMediaNeighbors, LascoError> {
        let count = self.inner.media_by_date_count(false);
        let position = position as usize;
        if count == 0 || position >= count {
            return Err(LascoError::NotFound);
        }

        let start = position.saturating_sub(1);
        let end = (position + 1).min(count - 1);
        let mut entries = self
            .inner
            .media_by_date_range(false, start, end)
            .into_iter()
            .map(media_entry_to_ffi);
        let previous = (position > 0).then(|| entries.next()).flatten();
        let current = entries.next().ok_or(LascoError::NotFound)?;
        Ok(FfiMediaNeighbors {
            previous,
            current,
            next: entries.next(),
        })
    }

    /// Positions are zero-based and both ends of the range are inclusive.
    ///
    /// # Errors
    ///
    /// Returns an error when the start position exceeds the end position.
    pub fn media_by_date_range(
        &self,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiMediaItem>, LascoError> {
        let (start, end) = inclusive_range(pos_start_inclusive, pos_end_inclusive)?;
        Ok(self
            .inner
            .media_by_date_range(false, start, end)
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    /// # Errors
    ///
    /// This method currently cannot fail; the `Result` preserves the FFI query API.
    pub fn orphan_media_by_date(&self) -> Result<Vec<FfiMediaItem>, LascoError> {
        let count = self.inner.media_by_date_count(true);
        Ok(self
            .inner
            .media_by_date_range(true, 0, count.saturating_sub(1))
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    pub fn orphan_media_by_date_count(&self) -> u64 {
        ffi_count(self.inner.media_by_date_count(true))
    }

    /// Returns the entries immediately surrounding a zero-based orphan position.
    ///
    /// # Errors
    ///
    /// Returns an error when `position` is outside the dated orphan-media list.
    pub fn orphan_media_by_date_neighbors(
        &self,
        position: u32,
    ) -> Result<FfiMediaNeighbors, LascoError> {
        let count = self.inner.media_by_date_count(true);
        let position = position as usize;
        if count == 0 || position >= count {
            return Err(LascoError::NotFound);
        }

        let start = position.saturating_sub(1);
        let end = (position + 1).min(count - 1);
        let mut entries = self
            .inner
            .media_by_date_range(true, start, end)
            .into_iter()
            .map(media_entry_to_ffi);
        let previous = (position > 0).then(|| entries.next()).flatten();
        let current = entries.next().ok_or(LascoError::NotFound)?;
        Ok(FfiMediaNeighbors {
            previous,
            current,
            next: entries.next(),
        })
    }

    /// Positions are zero-based and both ends of the range are inclusive.
    ///
    /// # Errors
    ///
    /// Returns an error when the start position exceeds the end position.
    pub fn orphan_media_by_date_range(
        &self,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiMediaItem>, LascoError> {
        let (start, end) = inclusive_range(pos_start_inclusive, pos_end_inclusive)?;
        Ok(self
            .inner
            .media_by_date_range(true, start, end)
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    /// # Errors
    ///
    /// Returns an error if `media_id` is not a valid UUID.
    pub fn media_album_ids(&self, media_id: FfiMediaUuid) -> Result<Vec<FfiAlbumUuid>, LascoError> {
        let media_uuid = media_id.try_into()?;
        Ok(self
            .inner
            .media_album_ids(media_uuid)
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// # Errors
    ///
    /// Returns an error if `media_id` is not a valid UUID.
    pub fn media_containing_album_ids(
        &self,
        media_id: FfiMediaUuid,
        include_via_groups: bool,
    ) -> Result<Vec<FfiAlbumUuid>, LascoError> {
        let media_uuid = media_id.try_into()?;
        Ok(self
            .inner
            .media_containing_album_ids(media_uuid, include_via_groups)
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// # Errors
    ///
    /// Returns an error if an ID is invalid or removing a cached media file fails.
    pub fn evict_local_data(&self, media_ids: Vec<FfiMediaUuid>) -> Result<(), LascoError> {
        let uuids = media_ids
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        self.inner
            .evict_local_data(&uuids)
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error if an ID is invalid or removing a cached thumbnail fails.
    pub fn evict_local_thumbnails(&self, media_ids: Vec<FfiMediaUuid>) -> Result<(), LascoError> {
        let uuids = media_ids
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        self.inner
            .evict_local_thumbnails(&uuids)
            .map_err(LascoError::from)
    }

    pub fn all_media_ids(&self) -> Vec<FfiMediaUuid> {
        self.inner
            .media_list(lasco_core::library::media::query::MediaListScope::All)
            .into_iter()
            .map(|entry| entry.media_id.into())
            .collect()
    }

    /// # Errors
    ///
    /// Returns an error if the library configuration cannot be read.
    pub fn media_ids_without_remote_backup(&self) -> Result<Vec<FfiMediaUuid>, LascoError> {
        let library_id = self.inner.library_id();
        let remote_ids = LibraryJson::load(&self.app_dir, &library_id)?
            .map(|cfg| lasco_core::library_json::list_remote_ids(&cfg))
            .unwrap_or_default();
        Ok(self
            .inner
            .media_ids_without_remote_backup(&remote_ids)
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub fn local_state_stats(&self) -> FfiLocalStateStats {
        let local_dirs = lasco_core::library::local_dirs::LocalDirs::new(
            self.app_dir.clone(),
            &self.inner.library_id(),
        );
        let media_dir = local_dirs.local_state_media_dir();
        let (media_cached_count, media_cached_bytes) =
            count_files_with_ext(media_dir.path(), "data");
        let (thumb_cached_count, thumb_cached_bytes) =
            count_files_with_ext(media_dir.path(), "thumb");
        FfiLocalStateStats {
            media_cached_count,
            media_cached_bytes,
            thumb_cached_count,
            thumb_cached_bytes,
        }
    }

    /// # Errors
    ///
    /// Returns an error if `media_id` is invalid, absent, or the delete operation cannot be persisted.
    pub fn delete_media(&self, media_id: FfiMediaUuid) -> Result<(), LascoError> {
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.media_delete(media_uuid))
            .map_err(LascoError::from)
    }
}

impl FfiLibrary {
    fn media_fetch_remote_ids(
        &self,
    ) -> Result<Vec<lasco_core::identifiers::RemoteUuid>, LascoError> {
        let library_id = self.inner.library_id();
        let lib_config =
            LibraryJson::load(&self.app_dir, &library_id)?.ok_or(LascoError::NotFound)?;
        let mut remotes: Vec<_> = lib_config
            .remotes
            .into_iter()
            .filter(|remote| !remote.exclude_from_media_fetch)
            .enumerate()
            .collect();
        remotes.sort_by_key(|(index, remote)| (remote.media_fetch_priority, *index));
        Ok(remotes
            .into_iter()
            .map(|(_, remote)| remote.remote_uuid)
            .collect())
    }
}

fn count_files_with_ext(dir: &std::path::Path, ext: &str) -> (u32, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut count = 0u32;
    let mut bytes = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let (c, b) = count_files_with_ext(&path, ext);
            count += c;
            bytes += b;
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            count += 1;
            bytes += entry.metadata().map_or(0, |m| m.len());
        }
    }
    (count, bytes)
}
