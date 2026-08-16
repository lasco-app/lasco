use lasco_core::crdt::GroupEntry;

use super::remotes::media_entry_to_ffi;
use super::{FfiGroup, FfiLibrary, FfiMediaItem};
use crate::error::LascoError;
use crate::ids::{FfiAlbumUuid, FfiGroupUuid, FfiMediaUuid};

pub(super) fn group_entry_to_ffi(e: &GroupEntry) -> FfiGroup {
    FfiGroup {
        group_id: e.group_id.into(),
        album_id_parent: e.album_id_parent.into(),
        media_ids: e.media_ids.iter().copied().map(Into::into).collect(),
    }
}

#[uniffi::export]
impl FfiLibrary {
    /// # Errors
    ///
    /// Returns an error if `album_id` is invalid or absent.
    pub fn album_list_groups(&self, album_id: FfiAlbumUuid) -> Result<Vec<FfiGroup>, LascoError> {
        let album_uuid = album_id.try_into()?;
        let entries = self
            .inner
            .album_list_groups(album_uuid)
            .map_err(LascoError::from)?;
        Ok(entries.iter().map(group_entry_to_ffi).collect())
    }

    /// # Errors
    ///
    /// Returns an error if `album_id` is invalid or absent, or creation cannot be persisted.
    pub fn create_group(&self, album_id: FfiAlbumUuid) -> Result<FfiGroupUuid, LascoError> {
        let album_uuid = album_id.try_into()?;
        let group_id = self
            .rt
            .block_on(self.inner.group_create(album_uuid))
            .map_err(LascoError::from)?;
        Ok(group_id.into())
    }

    /// # Errors
    ///
    /// Returns an error if `group_id` is invalid or absent, or deletion cannot be persisted.
    pub fn delete_group(&self, group_id: FfiGroupUuid) -> Result<(), LascoError> {
        let group_uuid = group_id.try_into()?;
        self.rt
            .block_on(self.inner.group_delete(group_uuid))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error if `group_id` is invalid or absent.
    pub fn group_list_media(
        &self,
        group_id: FfiGroupUuid,
    ) -> Result<Vec<FfiMediaItem>, LascoError> {
        let group_uuid = group_id.try_into()?;
        let entries = self
            .inner
            .group_list_media(group_uuid)
            .map_err(LascoError::from)?;
        Ok(entries.into_iter().map(media_entry_to_ffi).collect())
    }

    /// # Errors
    ///
    /// Returns an error for invalid or absent IDs, or an unpersistable membership operation.
    pub fn add_media_to_group(
        &self,
        group_id: FfiGroupUuid,
        media_id: FfiMediaUuid,
    ) -> Result<(), LascoError> {
        let group_uuid = group_id.try_into()?;
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.group_add_media(group_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    /// # Errors
    ///
    /// Returns an error for invalid or absent IDs, missing membership, or an unpersistable operation.
    pub fn remove_media_from_group(
        &self,
        group_id: FfiGroupUuid,
        media_id: FfiMediaUuid,
    ) -> Result<(), LascoError> {
        let group_uuid = group_id.try_into()?;
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.group_remove_media(group_uuid, media_uuid))
            .map_err(LascoError::from)
    }
}
