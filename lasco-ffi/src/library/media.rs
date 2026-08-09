use lasco_core::identifiers::{AlbumUuid, MediaUuid};
use lasco_core::library_json::LibraryJson;

use lasco_core::library::media::upload::MediaAddResult;

use super::remotes::media_entry_to_ffi;
use super::types::{FfiLocalStateStats, FfiMediaAddResult, FfiMediaNeighbors};
use super::{FfiLibrary, FfiMediaItem};
use crate::error::LascoError;

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
    pub fn load_local_state(&self) -> Result<(), LascoError> {
        self.rt
            .block_on(self.inner.load_local_state())
            .map_err(Into::into)
    }

    pub fn library_id(&self) -> String {
        self.inner.library_id().to_string()
    }

    pub fn get_default_fetch_remote(&self) -> Option<String> {
        let library_id = self.inner.library_id();
        let lib_config = LibraryJson::load(&self.app_dir, &library_id).ok()??;
        lib_config
            .default_fetch_remote
            .as_ref()
            .map(|id| id.to_string())
    }

    pub fn set_default_fetch_remote(&self, remote_id: Option<String>) -> Result<(), LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config =
            LibraryJson::load(&self.app_dir, &library_id)?.ok_or(LascoError::NotFound)?;
        lib_config.default_fetch_remote = match remote_id {
            Some(s) => Some(
                s.parse::<uuid::Uuid>()
                    .map(lasco_core::identifiers::RemoteUuid::from_uuid)
                    .map_err(|e| LascoError::Other {
                        msg: format!("invalid remote id: {e}"),
                    })?,
            ),
            None => None,
        };
        lib_config.save(&self.app_dir, &library_id)?;
        Ok(())
    }

    pub fn get_auto_import_device_media(&self) -> bool {
        let library_id = self.inner.library_id();
        LibraryJson::load(&self.app_dir, &library_id)
            .ok()
            .flatten()
            .map(|c| c.auto_import_device_media)
            .unwrap_or(false)
    }

    pub fn set_auto_import_device_media(&self, enabled: bool) -> Result<(), LascoError> {
        let library_id = self.inner.library_id();
        let mut lib_config =
            LibraryJson::load(&self.app_dir, &library_id)?.ok_or(LascoError::NotFound)?;
        lib_config.auto_import_device_media = enabled;
        lib_config.save(&self.app_dir, &library_id)?;
        Ok(())
    }

    pub fn set_media_thumbnail(&self, media_id: String, data: Vec<u8>) -> Result<(), LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(lasco_core::identifiers::MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.inner
            .media_set_thumbnail(media_uuid, &data)
            .map_err(LascoError::from)
    }

    pub fn rename_media(&self, media_id: String, name: Option<String>) -> Result<(), LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(lasco_core::identifiers::MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let name = name.map(lasco_core::operations::MediaName);
        self.rt
            .block_on(self.inner.media_rename(media_uuid, name))
            .map_err(LascoError::from)
    }

    pub fn get_media_thumbnail(
        &self,
        media_id: String,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(lasco_core::identifiers::MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        match self
            .rt
            .block_on(self.inner.media_get_thumbnail(media_uuid, None))
        {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let lib_config = self.load_library_json()?;
                let remote_id = lib_config
                    .remotes
                    .first()
                    .ok_or(LascoError::NotFound)?
                    .remote_uuid
                    .to_string();
                let storage =
                    self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
                self.rt
                    .block_on(
                        self.inner
                            .media_get_thumbnail(media_uuid, Some(storage.as_ref())),
                    )
                    .map_err(LascoError::from)
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    pub fn get_media_bytes(
        &self,
        media_id: String,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(lasco_core::identifiers::MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        match self
            .rt
            .block_on(self.inner.media_get_bytes(media_uuid, None))
        {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let lib_config = self.load_library_json()?;
                let remote_id = lib_config
                    .remotes
                    .first()
                    .ok_or(LascoError::NotFound)?
                    .remote_uuid
                    .to_string();
                let storage =
                    self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
                self.rt
                    .block_on(self.inner.media_get_bytes_from_remote(
                        media_uuid,
                        &remote_id,
                        storage.as_ref(),
                    ))
                    .map_err(LascoError::from)
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    pub async fn get_media_thumbnail_async(
        &self,
        media_id: String,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(lasco_core::identifiers::MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        match self.inner.media_get_thumbnail(media_uuid, None).await {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let lib_config = self.load_library_json()?;
                let remote_id = lib_config
                    .remotes
                    .first()
                    .ok_or(LascoError::NotFound)?
                    .remote_uuid
                    .to_string();
                let storage =
                    self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
                let inner = self.inner.clone();
                // Storage backends like S3 rely on hyper/tokio networking, which
                // needs an active reactor. Uniffi polls this future from its own
                // foreign executor with no reactor entered, so the remote fetch
                // is spawned onto our own tokio runtime instead of awaited inline.
                self.rt
                    .spawn(async move {
                        inner
                            .media_get_thumbnail(media_uuid, Some(storage.as_ref()))
                            .await
                    })
                    .await
                    .map_err(|e| LascoError::Other { msg: e.to_string() })?
                    .map_err(LascoError::from)
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    pub async fn get_media_bytes_async(
        &self,
        media_id: String,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(lasco_core::identifiers::MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        match self.inner.media_get_bytes(media_uuid, None).await {
            Ok(b) => Ok(b),
            Err(lasco_core::error::LibraryError::MediaNotFound(_)) => {
                let lib_config = self.load_library_json()?;
                let remote_id = lib_config
                    .remotes
                    .first()
                    .ok_or(LascoError::NotFound)?
                    .remote_uuid
                    .to_string();
                let storage =
                    self.build_storage_for_remote(&remote_id, app_support_dir.as_deref())?;
                let inner = self.inner.clone();
                // Storage backends like S3 rely on hyper/tokio networking, which
                // needs an active reactor. Uniffi polls this future from its own
                // foreign executor with no reactor entered, so the remote fetch
                // is spawned onto our own tokio runtime instead of awaited inline.
                self.rt
                    .spawn(async move {
                        inner
                            .media_get_bytes_from_remote(media_uuid, &remote_id, storage.as_ref())
                            .await
                    })
                    .await
                    .map_err(|e| LascoError::Other { msg: e.to_string() })?
                    .map_err(LascoError::from)
            }
            Err(e) => Err(LascoError::from(e)),
        }
    }

    pub fn import_media(
        &self,
        path: String,
        album_id: Option<String>,
        original_filename: Option<String>,
        apple_aae_media_id: Option<String>,
        apple_live_photo_media_id: Option<String>,
    ) -> Result<FfiMediaAddResult, LascoError> {
        let album_uuid = album_id
            .map(|s| {
                uuid::Uuid::parse_str(&s)
                    .map(AlbumUuid::from_uuid)
                    .map_err(|e| LascoError::Other { msg: e.to_string() })
            })
            .transpose()?;
        let apple_aae_media_uuid = apple_aae_media_id
            .map(|s| {
                uuid::Uuid::parse_str(&s)
                    .map(MediaUuid::from_uuid)
                    .map_err(|e| LascoError::Other { msg: e.to_string() })
            })
            .transpose()?;
        let apple_live_photo_media_uuid = apple_live_photo_media_id
            .map(|s| {
                uuid::Uuid::parse_str(&s)
                    .map(MediaUuid::from_uuid)
                    .map_err(|e| LascoError::Other { msg: e.to_string() })
            })
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
                media_id: id.to_string(),
                already_existed: false,
            },
            MediaAddResult::AlreadyExists(id) => FfiMediaAddResult {
                media_id: id.to_string(),
                already_existed: true,
            },
        })
    }

    pub fn pending_media_count(&self) -> u32 {
        self.inner.pending_media_count().unwrap_or(0)
    }

    pub fn has_unpushed_changes(&self, remote_id: String) -> bool {
        self.inner.has_unpushed_changes(&remote_id).unwrap_or(false)
    }

    pub fn list_media(&self) -> Result<Vec<FfiMediaItem>, LascoError> {
        Ok(self
            .inner
            .media_list(lasco_core::library::media::query::MediaListScope::Reachable)
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    pub fn show_media(&self, media_id: String) -> Result<FfiMediaItem, LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(lasco_core::identifiers::MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let entry = self
            .inner
            .media_show(media_uuid)
            .map_err(LascoError::from)?;
        Ok(media_entry_to_ffi(entry))
    }

    pub fn media_by_date(&self) -> Result<Vec<FfiMediaItem>, LascoError> {
        let count = self.inner.media_by_date_count(false);
        Ok(self
            .inner
            .media_by_date_range(false, 0, count.saturating_sub(1))
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    pub fn media_by_date_count(&self) -> u32 {
        self.inner.media_by_date_count(false) as u32
    }

    /// Returns the entries immediately surrounding a zero-based home position.
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

    pub fn orphan_media_by_date(&self) -> Result<Vec<FfiMediaItem>, LascoError> {
        let count = self.inner.media_by_date_count(true);
        Ok(self
            .inner
            .media_by_date_range(true, 0, count.saturating_sub(1))
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    pub fn orphan_media_by_date_count(&self) -> u32 {
        self.inner.media_by_date_count(true) as u32
    }

    /// Returns the entries immediately surrounding a zero-based orphan position.
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

    pub fn media_album_ids(&self, media_id: String) -> Result<Vec<String>, LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        Ok(self
            .inner
            .media_album_ids(media_uuid)
            .into_iter()
            .map(|id| id.to_string())
            .collect())
    }

    pub fn media_containing_album_ids(
        &self,
        media_id: String,
        include_via_groups: bool,
    ) -> Result<Vec<String>, LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        Ok(self
            .inner
            .media_containing_album_ids(media_uuid, include_via_groups)
            .into_iter()
            .map(|id| id.to_string())
            .collect())
    }

    pub fn evict_local_data(&self, media_ids: Vec<String>) -> Result<(), LascoError> {
        let uuids = media_ids
            .iter()
            .map(|s| {
                uuid::Uuid::parse_str(s)
                    .map(MediaUuid::from_uuid)
                    .map_err(|e| LascoError::Other { msg: e.to_string() })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.inner
            .evict_local_data(&uuids)
            .map_err(LascoError::from)
    }

    pub fn evict_local_thumbnails(&self, media_ids: Vec<String>) -> Result<(), LascoError> {
        let uuids = media_ids
            .iter()
            .map(|s| {
                uuid::Uuid::parse_str(s)
                    .map(MediaUuid::from_uuid)
                    .map_err(|e| LascoError::Other { msg: e.to_string() })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.inner
            .evict_local_thumbnails(&uuids)
            .map_err(LascoError::from)
    }

    pub fn all_media_ids(&self) -> Vec<String> {
        self.inner
            .media_list(lasco_core::library::media::query::MediaListScope::All)
            .into_iter()
            .map(|entry| entry.media_id.to_string())
            .collect()
    }

    pub fn media_ids_without_remote_backup(&self) -> Result<Vec<String>, LascoError> {
        let library_id = self.inner.library_id();
        let remote_ids = LibraryJson::load(&self.app_dir, &library_id)?
            .map(|cfg| lasco_core::library_json::list_remote_ids(&cfg))
            .unwrap_or_default();
        Ok(self
            .inner
            .media_ids_without_remote_backup(&remote_ids)
            .into_iter()
            .map(|id| id.to_string())
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

    pub fn delete_media(&self, media_id: String) -> Result<(), LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.rt
            .block_on(self.inner.media_delete(media_uuid))
            .map_err(LascoError::from)
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
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    (count, bytes)
}
