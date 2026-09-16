use lasco_core::library::media::upload::MediaAddResult;

use super::native_media_bytes::FfiNativeMediaBytes;
use super::remotes::media_entry_to_ffi;
use super::types::{
    FfiApplePhotosAssetRevision, FfiApplePhotosCollectionIdentity, FfiApplePhotosCollectionKind,
    FfiApplePhotosCollectionLink, FfiApplePhotosResourceOrigin, FfiApplePhotosResourceType,
    FfiLocalStateStats, FfiMediaAddResult, FfiMediaImportMetadata, FfiMediaNeighbors,
    FfiRemoteMediaShortfall,
};
use super::{FfiLibrary, FfiMediaItem, ffi_count};
use crate::error::LascoError;
use crate::ids::{FfiAlbumUuid, FfiLibraryId, FfiMediaUuid, FfiRemoteUuid};
use chrono::{DateTime, Utc};
use lasco_core::crdt::{ApplePhotosCollectionKind, ApplePhotosCollectionLink, ApplePhotosResourceType};
use lasco_core::identifiers::RemoteUuid;
use lasco_core::library::media::upload::MediaAddMetadata;
use lasco_core::operations::{ApplePhotosCloudAssetId, ApplePhotosCloudCollectionId, GpsCoords};
use std::path::PathBuf;
use std::sync::Arc;

/// `Vec<u8>` results cross the UniFFI boundary into a Kotlin `ByteArray`.
/// Keep that allocation bounded; large media must be materialized to a file
/// instead (currently used by Android video playback).
const MAX_MEDIA_BYTES: u64 = 100 * 1024 * 1024;

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
    /// Returns the selected media IDs when this exact Apple Photos asset revision has already
    /// been associated with Lasco media. This performs no resource download.
    pub fn apple_photos_asset_revision_media_ids(
        &self,
        revision: FfiApplePhotosAssetRevision,
    ) -> Result<Option<Vec<FfiMediaUuid>>, LascoError> {
        let modification_date =
            parse_import_timestamp(revision.modification_date, "modification_date")?;
        let resources = revision
            .resources
            .into_iter()
            .map(|resource| {
                (
                    apple_resource_type(resource.resource_type),
                    resource.filename,
                )
            })
            .collect::<Vec<_>>();
        Ok(
            lasco_core::library::media::apple_photos::complete_revision_media_ids(
                &self.inner,
                &ApplePhotosCloudAssetId(revision.cloud_asset_id),
                modification_date,
                &resources,
            )
            .map(|ids| ids.into_iter().map(Into::into).collect()),
        )
    }

    /// Records immutable provenance after an Apple Photos resource has been imported or reused
    /// by content hash. Importers call this once per selected resource.
    pub fn record_apple_photos_resource_origin(
        &self,
        origin: FfiApplePhotosResourceOrigin,
    ) -> Result<(), LascoError> {
        let media_id = origin.media_id.try_into()?;
        let modification_date =
            parse_import_timestamp(origin.modification_date, "modification_date")?;
        lasco_core::library::media::apple_photos::record_resource_origin(
            &self.inner,
            lasco_core::crdt::ApplePhotosResourceOrigin {
                media_id,
                cloud_asset_id: ApplePhotosCloudAssetId(origin.cloud_asset_id),
                modification_date,
                resource_type: apple_resource_type(origin.resource_type),
                filename: origin.filename,
            },
        )
        .map_err(LascoError::from)
    }

    /// Returns the canonical Lasco album for each known Apple Photos collection identity.
    pub fn apple_photos_collection_links(
        &self,
        collections: Vec<FfiApplePhotosCollectionIdentity>,
    ) -> Result<Vec<Option<FfiAlbumUuid>>, LascoError> {
        Ok(collections
            .into_iter()
            .map(|collection| {
                lasco_core::library::media::apple_photos::collection_album_id(
                    &self.inner,
                    &ApplePhotosCloudCollectionId(collection.cloud_collection_id),
                    apple_collection_kind(collection.kind),
                )
                .map(Into::into)
            })
            .collect())
    }

    /// Records immutable provenance after creating a Lasco album for an Apple Photos collection.
    pub fn record_apple_photos_collection_link(
        &self,
        link: FfiApplePhotosCollectionLink,
    ) -> Result<(), LascoError> {
        lasco_core::library::media::apple_photos::record_collection_link(
            &self.inner,
            ApplePhotosCollectionLink {
                album_id: link.album_id.try_into()?,
                cloud_collection_id: ApplePhotosCloudCollectionId(link.cloud_collection_id),
                kind: apple_collection_kind(link.kind),
            },
        )
        .map_err(LascoError::from)
    }
}

fn apple_resource_type(value: FfiApplePhotosResourceType) -> ApplePhotosResourceType {
    match value {
        FfiApplePhotosResourceType::Photo => ApplePhotosResourceType::Photo,
        FfiApplePhotosResourceType::FullSizePhoto => ApplePhotosResourceType::FullSizePhoto,
        FfiApplePhotosResourceType::Video => ApplePhotosResourceType::Video,
        FfiApplePhotosResourceType::FullSizeVideo => ApplePhotosResourceType::FullSizeVideo,
        FfiApplePhotosResourceType::AdjustmentData => ApplePhotosResourceType::AdjustmentData,
        FfiApplePhotosResourceType::PairedVideo => ApplePhotosResourceType::PairedVideo,
        FfiApplePhotosResourceType::FullSizePairedVideo => {
            ApplePhotosResourceType::FullSizePairedVideo
        }
    }
}

fn apple_collection_kind(value: FfiApplePhotosCollectionKind) -> ApplePhotosCollectionKind {
    match value {
        FfiApplePhotosCollectionKind::Folder => ApplePhotosCollectionKind::Folder,
        FfiApplePhotosCollectionKind::Album => ApplePhotosCollectionKind::Album,
    }
}

impl FfiLibrary {
    fn ensure_media_byte_result_is_safe(
        &self,
        media_id: lasco_core::identifiers::MediaUuid,
    ) -> Result<(), LascoError> {
        let size_bytes = self
            .inner
            .media_show(media_id)
            .map_err(LascoError::from)?
            .size_bytes;
        if size_bytes > MAX_MEDIA_BYTES {
            return Err(LascoError::MediaTooLarge {
                size_bytes,
                limit_bytes: MAX_MEDIA_BYTES,
            });
        }
        Ok(())
    }
}

#[uniffi::export]
impl FfiLibrary {
    /// # Errors
    ///
    /// Views are rebuilt atomically with every state change; retained as a no-op for FFI compatibility.
    pub fn load_local_state(&self) -> Result<(), LascoError> {
        Ok(())
    }

    pub fn library_id(&self) -> FfiLibraryId {
        self.inner.library_id().into()
    }

    pub fn get_default_fetch_remote(&self) -> Option<FfiRemoteUuid> {
        let lib_config = self.load_library_json().ok()?;
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
        let library_json = self.library_json_read_write();
        let mut lib_config = library_json.read()?;
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
        library_json.write(&lib_config)?;
        Ok(())
    }

    pub fn get_auto_import_device_media(&self) -> bool {
        self.load_library_json()
            .is_ok_and(|config| config.auto_import_device_media)
    }

    /// # Errors
    ///
    /// Returns an error if the library configuration is missing, malformed, or cannot be saved.
    pub fn set_auto_import_device_media(&self, enabled: bool) -> Result<(), LascoError> {
        let library_json = self.library_json_read_write();
        let mut lib_config = library_json.read()?;
        lib_config.auto_import_device_media = enabled;
        library_json.write(&lib_config)?;
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if `media_id` is invalid or the local thumbnail cannot be written.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
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
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
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
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn get_media_bytes(
        &self,
        media_id: FfiMediaUuid,
        app_support_dir: Option<String>,
    ) -> Result<Vec<u8>, LascoError> {
        let media_uuid = media_id.try_into()?;
        self.ensure_media_byte_result_is_safe(media_uuid)?;
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
        self.ensure_media_byte_result_is_safe(media_uuid)?;
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

    /// Returns Rust-owned plaintext media bytes without serializing them into
    /// a UniFFI byte array. The returned object's lifetime owns the backing
    /// allocation: platform code must retain it while reading `data_pointer`
    /// and close/destroy it as soon as the synchronous consumer is finished.
    ///
    /// Android uses this for image decoding on API 28 and newer, where
    /// `ImageDecoder` accepts a direct `ByteBuffer` view of the native bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the ID is invalid, no local or configured remote
    /// blob is available, or a remote read, decryption, or cache write fails.
    pub async fn get_media_bytes_native_async(
        &self,
        media_id: FfiMediaUuid,
        app_support_dir: Option<String>,
    ) -> Result<Arc<FfiNativeMediaBytes>, LascoError> {
        let bytes = self
            .get_media_bytes_async(media_id, app_support_dir)
            .await?;
        Ok(Arc::new(FfiNativeMediaBytes::new(bytes)))
    }

    /// Materializes decrypted media to an app-private destination without
    /// returning the full plaintext as a Kotlin byte array. Android uses this
    /// for video playback and export, where videos can be far too large for a
    /// safe FFI byte-array result.
    ///
    /// The caller owns the destination and is responsible for retaining or
    /// evicting it. On a remote cache miss this method downloads and caches
    /// the encrypted Lasco blob before writing the plaintext destination.
    pub async fn materialize_media_to_path_async(
        &self,
        media_id: FfiMediaUuid,
        app_support_dir: Option<String>,
        destination_path: String,
    ) -> Result<String, LascoError> {
        let media_uuid = media_id.try_into()?;
        let destination = PathBuf::from(&destination_path);
        match self
            .inner
            .media_materialize_to_path(media_uuid, &destination, None)
            .await
        {
            Ok(()) => Ok(destination_path),
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
                    let destination = destination.clone();
                    match self
                        .rt
                        .spawn(async move {
                            inner
                                .media_materialize_to_path(
                                    media_uuid,
                                    &destination,
                                    Some(storage.as_ref()),
                                )
                                .await
                        })
                        .await
                    {
                        Ok(Ok(())) => return Ok(destination_path),
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
            Err(error) => Err(LascoError::from(error)),
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

    /// Imports a media file with source-supplied metadata.
    ///
    /// Importers must preserve the original bytes and pass the source filename. Timestamps are
    /// RFC 3339 UTC offsets accepted by `chrono`; latitude and longitude must be supplied as a
    /// pair within their geographic ranges.
    ///
    /// # Errors
    ///
    /// Returns an error if an ID or metadata value is invalid, the source cannot be read, media
    /// encryption/storage fails, or the creation operation cannot be persisted.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn import_media_with_metadata(
        &self,
        path: String,
        album_id: Option<FfiAlbumUuid>,
        metadata: FfiMediaImportMetadata,
    ) -> Result<FfiMediaAddResult, LascoError> {
        let album_uuid = album_id.map(TryInto::try_into).transpose()?;
        let apple_aae_media_uuid = metadata
            .apple_aae_media_id
            .map(TryInto::try_into)
            .transpose()?;
        let apple_live_photo_media_uuid = metadata
            .apple_live_photo_media_id
            .map(TryInto::try_into)
            .transpose()?;
        let captured_at = parse_import_timestamp(metadata.captured_at, "captured_at")?;
        let modified_at = parse_import_timestamp(metadata.modified_at, "modified_at")?;
        let gps = parse_import_gps(metadata.latitude, metadata.longitude)?;
        let source =
            lasco_core::library::media::upload::MediaAddSource::CopyFrom(PathBuf::from(path));
        let result = self
            .rt
            .block_on(self.inner.media_add_with_metadata(
                source,
                album_uuid,
                metadata.original_filename,
                apple_aae_media_uuid,
                apple_live_photo_media_uuid,
                MediaAddMetadata {
                    captured_at,
                    modified_at,
                    gps,
                },
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

    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn has_unpushed_changes(&self, remote_id: FfiRemoteUuid) -> bool {
        let Ok(remote_id) = remote_id.try_into() else {
            return false;
        };
        self.inner.has_unpushed_changes(remote_id).unwrap_or(false)
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

    pub fn trashed_media_by_date_count(&self) -> u64 {
        ffi_count(self.inner.trashed_media_by_date_count())
    }

    /// Positions are zero-based and both ends of the range are inclusive.
    pub fn trashed_media_by_date_range(
        &self,
        pos_start_inclusive: u32,
        pos_end_inclusive: u32,
    ) -> Result<Vec<FfiMediaItem>, LascoError> {
        let (start, end) = inclusive_range(pos_start_inclusive, pos_end_inclusive)?;
        Ok(self
            .inner
            .trashed_media_by_date_range(start, end)
            .into_iter()
            .map(media_entry_to_ffi)
            .collect())
    }

    /// Returns every trashed media record, including companions hidden from
    /// normal Trash browsing. This is intended for maintenance flows.
    pub fn trashed_media_all(&self) -> Vec<FfiMediaItem> {
        self.inner
            .trashed_media_all()
            .into_iter()
            .map(media_entry_to_ffi)
            .collect()
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

    /// What `remote_id` is not yet confirmed to hold.
    ///
    /// Only meaningful once every local operation has reached the remote, since media a remote
    /// has never been told about cannot be expected on it.
    ///
    /// # Errors
    ///
    /// Returns an error if `remote_id` is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn remote_media_shortfall(
        &self,
        remote_id: FfiRemoteUuid,
    ) -> Result<FfiRemoteMediaShortfall, LascoError> {
        let remote_uuid: RemoteUuid = remote_id.try_into()?;
        let shortfall = self.inner.remote_media_shortfall(&remote_uuid.to_string());
        Ok(FfiRemoteMediaShortfall {
            missing_full: ffi_count(shortfall.missing_full),
            missing_thumb: ffi_count(shortfall.missing_thumb),
        })
    }

    /// Returns which supplied media IDs are confirmed to have a full original on this remote.
    ///
    /// Callers should refresh the remote inventory with `confirm_remote_media_async` first.
    /// The result reflects this client's cached positive-only inventory and never performs a
    /// network request itself.
    pub fn confirmed_remote_media_ids(
        &self,
        remote_id: FfiRemoteUuid,
        media_ids: Vec<FfiMediaUuid>,
    ) -> Result<Vec<FfiMediaUuid>, LascoError> {
        let remote_uuid: RemoteUuid = remote_id.try_into()?;
        let media_ids = media_ids
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self
            .inner
            .confirmed_remote_media_ids(&remote_uuid.to_string(), &media_ids)
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Counts the media that clearing local media would leave with no known copy anywhere.
    ///
    /// Only a remote can back up a local copy here, because the local copy is what the
    /// operation deletes. The answer is an upper bound, see `media_ids_without_backup`.
    ///
    /// # Errors
    ///
    /// Returns an error if the library configuration cannot be read.
    pub fn media_count_lost_if_local_media_cleared(&self) -> Result<u64, LascoError> {
        let remote_ids = self
            .load_library_json()
            .map(|config| lasco_core::library_json::list_remote_ids(&config))?;
        Ok(ffi_count(
            self.inner
                .media_ids_without_backup(
                    &remote_ids,
                    lasco_core::library::media::query::BackupScope::RemotesOnly,
                )
                .len(),
        ))
    }

    /// Counts the media that removing `remote_id` would leave with no known copy anywhere.
    ///
    /// The local copy survives the removal, so it counts as a home alongside every remaining
    /// remote. The answer is an upper bound, see `media_ids_without_backup`.
    ///
    /// # Errors
    ///
    /// Returns an error if the library configuration cannot be read or `remote_id` is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports owned values across the language boundary; borrowed inputs would complicate the generated binding contract."
    )]
    pub fn media_count_lost_if_remote_removed(
        &self,
        remote_id: FfiRemoteUuid,
    ) -> Result<u64, LascoError> {
        let removed: RemoteUuid = remote_id.try_into()?;
        let removed = removed.to_string();
        let remaining: Vec<String> = self
            .load_library_json()
            .map(|config| lasco_core::library_json::list_remote_ids(&config))?
            .into_iter()
            .filter(|id| *id != removed)
            .collect();
        Ok(ffi_count(
            self.inner
                .media_ids_without_backup(
                    &remaining,
                    lasco_core::library::media::query::BackupScope::RemotesOrLocal,
                )
                .len(),
        ))
    }

    pub fn local_state_stats(&self) -> FfiLocalStateStats {
        let local_dirs = lasco_core::library::local_dirs::LocalDirs::new(
            &self.app_dir,
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
    /// Moves media to Trash. Its encrypted data remains available for Restore.
    pub fn delete_media(&self, media_id: FfiMediaUuid) -> Result<(), LascoError> {
        self.soft_delete_media(media_id)
    }

    pub fn soft_delete_media(&self, media_id: FfiMediaUuid) -> Result<(), LascoError> {
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.media_soft_delete(media_uuid))
            .map_err(LascoError::from)
    }

    pub fn restore_media(&self, media_id: FfiMediaUuid) -> Result<(), LascoError> {
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.media_restore(media_uuid))
            .map_err(LascoError::from)
    }

    /// Permanently deletes media already in Trash and its trashed companions from CRDT state
    /// and the local encrypted cache.
    /// Remote blobs are reclaimed after the tombstone is pushed to each remote.
    pub fn hard_delete_media(&self, media_id: FfiMediaUuid) -> Result<(), LascoError> {
        let media_uuid = media_id.try_into()?;
        self.rt
            .block_on(self.inner.media_hard_delete(media_uuid))
            .map_err(LascoError::from)
    }

    /// Permanently deletes every item in Trash, including hidden companions.
    /// Remote blobs are reclaimed after their tombstones are pushed.
    pub fn empty_trash(&self) -> Result<u64, LascoError> {
        self.rt
            .block_on(self.inner.media_empty_trash())
            .map(ffi_count)
            .map_err(LascoError::from)
    }
}

fn parse_import_timestamp(
    value: Option<String>,
    field: &str,
) -> Result<Option<DateTime<Utc>>, LascoError> {
    value
        .map(|timestamp| {
            DateTime::parse_from_rfc3339(&timestamp)
                .map(|parsed| parsed.with_timezone(&Utc))
                .map_err(|error| LascoError::Other {
                    msg: format!("{field} must be RFC 3339: {error}"),
                })
        })
        .transpose()
}

fn parse_import_gps(
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<Option<GpsCoords>, LascoError> {
    let (Some(latitude), Some(longitude)) = (latitude, longitude) else {
        if latitude.is_none() && longitude.is_none() {
            return Ok(None);
        }
        return Err(LascoError::Other {
            msg: "latitude and longitude must be supplied together".to_string(),
        });
    };
    if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
        return Err(LascoError::Other {
            msg: "latitude must be finite and between -90 and 90".to_string(),
        });
    }
    if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
        return Err(LascoError::Other {
            msg: "longitude must be finite and between -180 and 180".to_string(),
        });
    }
    Ok(Some(GpsCoords {
        latitude,
        longitude,
    }))
}

#[cfg(test)]
mod importer_metadata_tests {
    use super::{parse_import_gps, parse_import_timestamp};

    #[test]
    fn parses_metadata_timestamps_as_utc() {
        let timestamp =
            parse_import_timestamp(Some("2024-06-01T12:30:00+02:00".to_string()), "captured_at")
                .unwrap()
                .unwrap();

        assert_eq!(timestamp.to_rfc3339(), "2024-06-01T10:30:00+00:00");
    }

    #[test]
    fn rejects_partial_or_out_of_range_gps() {
        assert!(parse_import_gps(Some(48.8566), None).is_err());
        assert!(parse_import_gps(Some(91.0), Some(2.3522)).is_err());
        assert!(parse_import_gps(Some(48.8566), Some(181.0)).is_err());
    }
}

impl FfiLibrary {
    fn media_fetch_remote_ids(
        &self,
    ) -> Result<Vec<lasco_core::identifiers::RemoteUuid>, LascoError> {
        let lib_config = self.load_library_json()?;
        Ok(lib_config.media_source_order)
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
