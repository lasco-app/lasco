use lasco_core::identifiers::{AlbumUuid, MediaUuid};
use lasco_core::library::albums::{AlbumItem, AlbumSummary, DatedAlbumItem};
use lasco_core::operations::AlbumName;

use super::groups::group_entry_to_ffi;
use super::remotes::media_entry_to_ffi;
use super::{FfiAlbum, FfiAlbumItem, FfiLibrary, FfiMediaItem};
use crate::error::LascoError;

use super::media::inclusive_range;

fn album_summary_to_ffi(a: AlbumSummary) -> FfiAlbum {
    FfiAlbum {
        album_id: a.album_id.to_string(),
        name: a.name.0,
        parent_album_id: a.album_id_parent.map(|p| p.to_string()),
        media_count: a.media_count as u32,
        deleted: false,
        is_disconnected: false,
        thumbnail_media_id: a.thumbnail_media_id.map(|m| m.to_string()),
    }
}

fn dated_album_item_to_ffi(item: DatedAlbumItem) -> FfiAlbumItem {
    let effective_date = item.effective_date.to_rfc3339();
    match item.item {
        AlbumItem::Media(media) => FfiAlbumItem {
            kind: "media".to_string(),
            media: Some(media_entry_to_ffi(media)),
            group: None,
            effective_date,
        },
        AlbumItem::Group(group) => FfiAlbumItem {
            kind: "group".to_string(),
            media: None,
            group: Some(group_entry_to_ffi(&group)),
            effective_date,
        },
    }
}

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

    pub fn album_albums_count(&self, parent_album_id: Option<String>) -> Result<u32, LascoError> {
        let parent = parent_album_id
            .map(|id| uuid::Uuid::parse_str(&id).map(AlbumUuid::from_uuid))
            .transpose()
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        Ok(self.inner.album_albums_count(parent) as u32)
    }

    /// Returns direct albums under `parent_album_id`; `None` means root albums.
    /// Positions are zero-based and both ends of the range are inclusive.
    pub fn album_albums_range(
        &self,
        parent_album_id: Option<String>,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiAlbum>, LascoError> {
        let parent = parent_album_id
            .map(|id| uuid::Uuid::parse_str(&id).map(AlbumUuid::from_uuid))
            .transpose()
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let (start, end) = inclusive_range(pos_start_inclusive, pos_end_inclusive)?;
        Ok(self
            .inner
            .album_albums_range(parent, start, end)
            .into_iter()
            .map(album_summary_to_ffi)
            .collect())
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

        let count = self.inner.album_items_count(album_uuid).map_err(LascoError::from)?;
        Ok(self
            .inner
            .album_items_by_date_range(album_uuid, ascending, 0, count.saturating_sub(1))
            .map_err(LascoError::from)?
            .into_iter()
            .map(dated_album_item_to_ffi)
            .collect())
    }

    pub fn album_items_count(&self, album_id: String) -> Result<u32, LascoError> {
        let album_uuid = uuid::Uuid::parse_str(&album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        Ok(self.inner.album_items_count(album_uuid).map_err(LascoError::from)? as u32)
    }

    /// Positions are zero-based and both ends of the range are inclusive.
    pub fn album_items_by_date_range(
        &self,
        album_id: String,
        ascending: bool,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiAlbumItem>, LascoError> {
        let album_uuid = uuid::Uuid::parse_str(&album_id)
            .map(AlbumUuid::from_uuid)
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;
        let (start, end) = inclusive_range(pos_start_inclusive, pos_end_inclusive)?;
        Ok(self
            .inner
            .album_items_by_date_range(album_uuid, ascending, start, end)
            .map_err(LascoError::from)?
            .into_iter()
            .map(dated_album_item_to_ffi)
            .collect())
    }
}
