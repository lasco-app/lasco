use lasco_core::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use lasco_core::state::GroupEntry;

use super::remotes::media_entry_to_ffi;
use super::{FfiGroup, FfiLibrary, FfiMediaItem};
use crate::error::LascoError;

fn parse_uuid(s: &str) -> Result<uuid::Uuid, LascoError> {
    uuid::Uuid::parse_str(s).map_err(|e| LascoError::Other { msg: e.to_string() })
}

pub(super) fn group_entry_to_ffi(e: &GroupEntry) -> FfiGroup {
    FfiGroup {
        group_id: e.group_id.to_string(),
        album_id_parent: e.album_id_parent.to_string(),
        media_ids: e.media_ids.iter().map(|id| id.to_string()).collect(),
    }
}

#[uniffi::export]
impl FfiLibrary {
    pub fn album_list_groups(&self, album_id: String) -> Result<Vec<FfiGroup>, LascoError> {
        let album_uuid = AlbumUuid::from_uuid(parse_uuid(&album_id)?);
        let entries = self.inner.album_list_groups(album_uuid).map_err(LascoError::from)?;
        Ok(entries.iter().map(group_entry_to_ffi).collect())
    }

    pub fn create_group(&self, album_id: String) -> Result<String, LascoError> {
        let album_uuid = AlbumUuid::from_uuid(parse_uuid(&album_id)?);
        let group_id = self
            .rt
            .block_on(self.inner.group_create(album_uuid))
            .map_err(LascoError::from)?;
        Ok(group_id.to_string())
    }

    pub fn delete_group(&self, group_id: String) -> Result<(), LascoError> {
        let group_uuid = GroupUuid::from_uuid(parse_uuid(&group_id)?);
        self.rt
            .block_on(self.inner.group_delete(group_uuid))
            .map_err(LascoError::from)
    }

    pub fn group_list_media(&self, group_id: String) -> Result<Vec<FfiMediaItem>, LascoError> {
        let group_uuid = GroupUuid::from_uuid(parse_uuid(&group_id)?);
        let entries = self.inner.group_list_media(group_uuid).map_err(LascoError::from)?;
        Ok(entries.into_iter().map(media_entry_to_ffi).collect())
    }

    pub fn add_media_to_group(&self, group_id: String, media_id: String) -> Result<(), LascoError> {
        let group_uuid = GroupUuid::from_uuid(parse_uuid(&group_id)?);
        let media_uuid = MediaUuid::from_uuid(parse_uuid(&media_id)?);
        self.rt
            .block_on(self.inner.group_add_media(group_uuid, media_uuid))
            .map_err(LascoError::from)
    }

    pub fn remove_media_from_group(
        &self,
        group_id: String,
        media_id: String,
    ) -> Result<(), LascoError> {
        let group_uuid = GroupUuid::from_uuid(parse_uuid(&group_id)?);
        let media_uuid = MediaUuid::from_uuid(parse_uuid(&media_id)?);
        self.rt
            .block_on(self.inner.group_remove_media(group_uuid, media_uuid))
            .map_err(LascoError::from)
    }
}
