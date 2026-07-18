use lasco_core::identifiers::{AlbumUuid, MediaUuid};
use lasco_core::operations::AlbumName;

use super::groups::group_entry_to_ffi;
use super::remotes::media_entry_to_ffi;
use super::{FfiAlbum, FfiAlbumItem, FfiLibrary, FfiMediaItem};
use crate::error::LascoError;

#[uniffi::export]
impl FfiLibrary {
    pub fn list_albums(&self) -> Result<Vec<FfiAlbum>, LascoError> {
        let mut albums: Vec<FfiAlbum> = self
            .inner
            .album_list()
            .into_iter()
            .map(|a| FfiAlbum {
                album_id: a.album_id.to_string(),
                name: a.name.0,
                parent_album_id: a.album_id_parent.map(|p| p.to_string()),
                media_count: a.media_count as u32,
                deleted: false,
                is_disconnected: false,
                thumbnail_media_id: a.thumbnail_media_id.map(|m| m.to_string()),
            })
            .collect();
        for id in self.inner.album_disconnected_ids() {
            if let Some((name, parent_id, media_count, thumbnail_media_id)) = self.inner.album_node_by_id(id) {
                albums.push(FfiAlbum {
                    album_id: id.to_string(),
                    name: name.0,
                    parent_album_id: parent_id.map(|p| p.to_string()),
                    media_count: media_count as u32,
                    deleted: false,
                    is_disconnected: true,
                    thumbnail_media_id: thumbnail_media_id.map(|m| m.to_string()),
                });
            }
        }
        Ok(albums)
    }

    pub fn rename_album(&self, album_id: String, name: String) -> Result<(), LascoError> {
        let uuid = uuid::Uuid::parse_str(&album_id)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let album_uuid = AlbumUuid::from_uuid(uuid);
        self.rt
            .block_on(self.inner.album_rename(album_uuid, AlbumName(name)))
            .map_err(LascoError::from)
    }

    pub fn reparent_album(
        &self,
        album_id: String,
        new_parent_album_id: Option<String>,
    ) -> Result<(), LascoError> {
        let uuid = uuid::Uuid::parse_str(&album_id)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let album_uuid = AlbumUuid::from_uuid(uuid);
        let parent = new_parent_album_id
            .map(|s| uuid::Uuid::parse_str(&s).map(AlbumUuid::from_uuid))
            .transpose()
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.rt
            .block_on(self.inner.album_reparent(album_uuid, parent))
            .map_err(LascoError::from)
    }

    pub fn create_album(
        &self,
        name: String,
        parent_album_id: Option<String>,
    ) -> Result<String, LascoError> {
        let parent = parent_album_id
            .map(|s| uuid::Uuid::parse_str(&s).map(AlbumUuid::from_uuid))
            .transpose()
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let album_id = self
            .rt
            .block_on(self.inner.album_create(AlbumName(name), parent))
            .map_err(LascoError::from)?;
        Ok(album_id.to_string())
    }

    pub fn media_in_album(&self, album_id: String) -> Result<Vec<FfiMediaItem>, LascoError> {
        let uuid = uuid::Uuid::parse_str(&album_id)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let album_uuid = AlbumUuid::from_uuid(uuid);
        let entries = self.inner.album_list_media(album_uuid).map_err(LascoError::from)?;
        Ok(entries.into_iter().map(media_entry_to_ffi).collect())
    }

    pub fn delete_album(&self, album_id: String) -> Result<(), LascoError> {
        let uuid = uuid::Uuid::parse_str(&album_id)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.rt
            .block_on(self.inner.album_delete(AlbumUuid::from_uuid(uuid)))
            .map_err(LascoError::from)
    }

    pub fn set_album_thumbnail(
        &self,
        album_id: String,
        media_id: Option<String>,
    ) -> Result<(), LascoError> {
        let album_uuid = uuid::Uuid::parse_str(&album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let media_uuid = media_id
            .map(|s| uuid::Uuid::parse_str(&s).map(MediaUuid::from_uuid))
            .transpose()
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.rt
            .block_on(self.inner.album_set_thumbnail(album_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    pub fn remove_media_from_album(
        &self,
        album_id: String,
        media_id: String,
    ) -> Result<(), LascoError> {
        let album_uuid = uuid::Uuid::parse_str(&album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.rt
            .block_on(self.inner.album_remove_media(album_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    pub fn add_media_to_album(
        &self,
        album_id: String,
        media_id: String,
    ) -> Result<(), LascoError> {
        let album_uuid = uuid::Uuid::parse_str(&album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.rt
            .block_on(self.inner.album_add_media(album_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    pub fn move_media_to_album(
        &self,
        media_id: String,
        from_album_id: String,
        to_album_id: String,
    ) -> Result<(), LascoError> {
        let media_uuid = uuid::Uuid::parse_str(&media_id)
            .map(MediaUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let from_uuid = uuid::Uuid::parse_str(&from_album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let to_uuid = uuid::Uuid::parse_str(&to_album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        self.rt.block_on(async {
            self.inner.album_remove_media(from_uuid, media_uuid).await?;
            self.inner.album_add_media(to_uuid, media_uuid).await
        }).map_err(LascoError::from)
    }

    pub fn album_list_items_sorted(
        &self,
        album_id: String,
        ascending: bool,
    ) -> Result<Vec<FfiAlbumItem>, LascoError> {
        let album_uuid = uuid::Uuid::parse_str(&album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        let media = self.inner.album_list_media(album_uuid).map_err(LascoError::from)?;
        let groups = self.inner.album_list_groups(album_uuid).map_err(LascoError::from)?;

        let mut items: Vec<FfiAlbumItem> = Vec::with_capacity(media.len() + groups.len());

        for entry in media {
            let ffi = media_entry_to_ffi(entry);
            let date = ffi.date.clone();
            items.push(FfiAlbumItem { kind: "media".to_string(), effective_date: date, media: Some(ffi), group: None });
        }

        for entry in &groups {
            let group_media = self.inner.group_list_media(entry.group_id).map_err(LascoError::from)?;
            let effective_date = group_media
                .into_iter()
                .map(|m| media_entry_to_ffi(m).date)
                .max()
                .unwrap_or_default();
            items.push(FfiAlbumItem {
                kind: "group".to_string(),
                effective_date,
                media: None,
                group: Some(group_entry_to_ffi(entry)),
            });
        }

        if ascending {
            items.sort_by(|a, b| a.effective_date.cmp(&b.effective_date));
        } else {
            items.sort_by(|a, b| b.effective_date.cmp(&a.effective_date));
        }

        Ok(items)
    }
}
