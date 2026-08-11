use lasco_core::identifiers::AlbumUuid;
use lasco_core::library::albums::{AlbumItem, AlbumSummary, DatedAlbumItem};
use lasco_core::operations::AlbumName;

use super::groups::group_entry_to_ffi;
use super::remotes::media_entry_to_ffi;
use super::{FfiAlbum, FfiAlbumItem, FfiLibrary, FfiMediaItem, FfiMediaOrGroupNeighbors};
use crate::error::LascoError;
use crate::ids::{FfiAlbumUuid, FfiMediaUuid};

use super::media::inclusive_range;

fn album_summary_to_ffi(a: AlbumSummary) -> FfiAlbum {
    FfiAlbum {
        album_id: a.album_id.into(),
        name: a.name.0,
        parent_album_id: a.album_id_parent.map(Into::into),
        media_count: a.media_count as u32,
        deleted: false,
        is_disconnected: false,
        thumbnail_media_id: a.thumbnail_media_id.map(Into::into),
    }
}

fn disconnected_album_to_ffi(library: &FfiLibrary, id: AlbumUuid) -> Option<FfiAlbum> {
    library
        .inner
        .album_node_by_id(id)
        .map(
            |(name, parent_id, media_count, thumbnail_media_id)| FfiAlbum {
                album_id: id.into(),
                name: name.0,
                parent_album_id: parent_id.map(Into::into),
                media_count: media_count as u32,
                deleted: false,
                is_disconnected: true,
                thumbnail_media_id: thumbnail_media_id.map(Into::into),
            },
        )
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
    /// # Errors
    ///
    /// This method currently cannot fail; the `Result` preserves the FFI query API.
    pub fn list_albums(&self) -> Result<Vec<FfiAlbum>, LascoError> {
        let mut albums: Vec<FfiAlbum> = self
            .inner
            .album_list()
            .into_iter()
            .map(|a| FfiAlbum {
                album_id: a.album_id.into(),
                name: a.name.0,
                parent_album_id: a.album_id_parent.map(Into::into),
                media_count: a.media_count as u32,
                deleted: false,
                is_disconnected: false,
                thumbnail_media_id: a.thumbnail_media_id.map(Into::into),
            })
            .collect();
        albums.extend(
            self.inner
                .album_disconnected_ids()
                .into_iter()
                .filter_map(|id| disconnected_album_to_ffi(self, id)),
        );
        Ok(albums)
    }

    /// # Errors
    ///
    /// Returns an error if `parent_album_id` is not a valid UUID.
    pub fn album_albums_count(
        &self,
        parent_album_id: Option<FfiAlbumUuid>,
    ) -> Result<u32, LascoError> {
        let parent = parent_album_id.map(TryInto::try_into).transpose()?;
        Ok(self.inner.album_albums_count(parent) as u32)
    }

    /// Returns direct albums under `parent_album_id`; `None` means root albums.
    /// Positions are zero-based and both ends of the range are inclusive.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid parent ID or an inverted position range.
    pub fn album_albums_range(
        &self,
        parent_album_id: Option<FfiAlbumUuid>,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiAlbum>, LascoError> {
        let parent = parent_album_id.map(TryInto::try_into).transpose()?;
        let (start, end) = inclusive_range(pos_start_inclusive, pos_end_inclusive)?;
        Ok(self
            .inner
            .album_albums_range(parent, start, end)
            .into_iter()
            .map(album_summary_to_ffi)
            .collect())
    }

    pub fn disconnected_albums_count(&self) -> u32 {
        self.inner.album_disconnected_ids().len() as u32
    }

    /// Returns disconnected albums in the same order as `list_albums`.
    /// Positions are zero-based and both ends of the range are inclusive.
    ///
    /// # Errors
    ///
    /// Returns an error when the start position exceeds the end position.
    pub fn disconnected_albums_range(
        &self,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiAlbum>, LascoError> {
        let (start, end) = inclusive_range(pos_start_inclusive, pos_end_inclusive)?;
        Ok(self
            .inner
            .album_disconnected_ids()
            .into_iter()
            .skip(start)
            .take(end.saturating_sub(start).saturating_add(1))
            .filter_map(|id| disconnected_album_to_ffi(self, id))
            .collect())
    }

    /// # Errors
    ///
    /// Returns an error if the ID is invalid or absent, or the rename cannot be persisted.
    pub fn rename_album(&self, album_id: FfiAlbumUuid, name: String) -> Result<(), LascoError> {
        let album_uuid = album_id.try_into()?;
        self.rt
            .block_on(self.inner.album_rename(album_uuid, AlbumName(name)))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error for invalid or absent IDs, a cyclic move, or an unpersistable operation.
    pub fn reparent_album(
        &self,
        album_id: FfiAlbumUuid,
        new_parent_album_id: Option<FfiAlbumUuid>,
    ) -> Result<(), LascoError> {
        let album_uuid = album_id.try_into()?;
        let parent = new_parent_album_id.map(TryInto::try_into).transpose()?;
        self.rt
            .block_on(self.inner.album_reparent(album_uuid, parent))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error if the optional parent ID is invalid or absent, or creation cannot be persisted.
    pub fn create_album(
        &self,
        name: String,
        parent_album_id: Option<FfiAlbumUuid>,
    ) -> Result<FfiAlbumUuid, LascoError> {
        let parent = parent_album_id.map(TryInto::try_into).transpose()?;
        let album_id = self
            .rt
            .block_on(self.inner.album_create(AlbumName(name), parent))
            .map_err(LascoError::from)?;
        Ok(album_id.into())
    }

    /// # Errors
    ///
    /// Returns an error if `album_id` is invalid or absent.
    pub fn media_in_album(&self, album_id: FfiAlbumUuid) -> Result<Vec<FfiMediaItem>, LascoError> {
        let album_uuid = album_id.try_into()?;
        let entries = self
            .inner
            .album_list_media(album_uuid)
            .map_err(LascoError::from)?;
        Ok(entries.into_iter().map(media_entry_to_ffi).collect())
    }

    /// # Errors
    ///
    /// Returns an error if `album_id` is invalid or absent, or deletion cannot be persisted.
    pub fn delete_album(&self, album_id: FfiAlbumUuid) -> Result<(), LascoError> {
        self.rt
            .block_on(self.inner.album_delete(album_id.try_into()?))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error for invalid or absent album/media IDs, or an unpersistable operation.
    pub fn set_album_thumbnail(
        &self,
        album_id: FfiAlbumUuid,
        media_id: Option<FfiMediaUuid>,
    ) -> Result<(), LascoError> {
        let album_uuid = album_id.try_into()?;
        let media_uuid = media_id.map(TryInto::try_into).transpose()?;
        self.rt
            .block_on(self.inner.album_set_thumbnail(album_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error for invalid or absent IDs, missing membership, or an unpersistable operation.
    pub fn remove_media_from_album(
        &self,
        album_id: FfiAlbumUuid,
        media_id: FfiMediaUuid,
    ) -> Result<(), LascoError> {
        let album_uuid = album_id.try_into()?;
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.album_remove_media(album_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error for invalid or absent IDs, or an unpersistable membership operation.
    pub fn add_media_to_album(
        &self,
        album_id: FfiAlbumUuid,
        media_id: FfiMediaUuid,
    ) -> Result<(), LascoError> {
        let album_uuid = album_id.try_into()?;
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.album_add_media(album_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error for invalid or absent IDs, missing source membership, or a failed remove/add operation.
    pub fn move_media_to_album(
        &self,
        media_id: FfiMediaUuid,
        from_album_id: FfiAlbumUuid,
        to_album_id: FfiAlbumUuid,
    ) -> Result<(), LascoError> {
        let media_uuid = media_id.try_into()?;
        let from_uuid = from_album_id.try_into()?;
        let to_uuid = to_album_id.try_into()?;
        self.rt
            .block_on(async {
                self.inner.album_remove_media(from_uuid, media_uuid).await?;
                self.inner.album_add_media(to_uuid, media_uuid).await
            })
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error if `album_id` is invalid or absent.
    pub fn album_list_items_sorted(
        &self,
        album_id: FfiAlbumUuid,
        ascending: bool,
    ) -> Result<Vec<FfiAlbumItem>, LascoError> {
        let album_uuid = album_id.try_into()?;

        let count = self
            .inner
            .album_items_count(album_uuid)
            .map_err(LascoError::from)?;
        Ok(self
            .inner
            .album_items_by_date_range(album_uuid, ascending, 0, count.saturating_sub(1))
            .map_err(LascoError::from)?
            .into_iter()
            .map(dated_album_item_to_ffi)
            .collect())
    }

    /// # Errors
    ///
    /// Returns an error if `album_id` is invalid or absent.
    pub fn album_items_count(&self, album_id: FfiAlbumUuid) -> Result<u32, LascoError> {
        let album_uuid = album_id.try_into()?;
        Ok(self
            .inner
            .album_items_count(album_uuid)
            .map_err(LascoError::from)? as u32)
    }

    /// Returns the entries immediately surrounding a zero-based album position.
    ///
    /// # Errors
    ///
    /// Returns an error if the album ID is invalid or absent, or `position` is outside its item list.
    pub fn album_items_by_date_neighbors(
        &self,
        album_id: FfiAlbumUuid,
        ascending: bool,
        position: u32,
    ) -> Result<FfiMediaOrGroupNeighbors, LascoError> {
        let album_uuid = album_id.try_into()?;
        let count = self
            .inner
            .album_items_count(album_uuid)
            .map_err(LascoError::from)?;
        let position = position as usize;
        if count == 0 || position >= count {
            return Err(LascoError::NotFound);
        }

        let start = position.saturating_sub(1);
        let end = (position + 1).min(count - 1);
        let mut entries = self
            .inner
            .album_items_by_date_range(album_uuid, ascending, start, end)
            .map_err(LascoError::from)?
            .into_iter()
            .map(dated_album_item_to_ffi);
        let previous = (position > 0).then(|| entries.next()).flatten();
        let current = entries.next().ok_or(LascoError::NotFound)?;
        Ok(FfiMediaOrGroupNeighbors {
            previous,
            current,
            next: entries.next(),
        })
    }

    /// Positions are zero-based and both ends of the range are inclusive.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or absent album ID, or an inverted position range.
    pub fn album_items_by_date_range(
        &self,
        album_id: FfiAlbumUuid,
        ascending: bool,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiAlbumItem>, LascoError> {
        let album_uuid = album_id.try_into()?;
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
