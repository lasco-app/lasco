use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Datelike, Utc};
use uuid::Uuid;

use crate::crdt::{MediaCreation, OperationContent};
use crate::encryption::blob::encrypt_blob;
use crate::encryption::blob_key::derive_blob_key;
use crate::error::LibraryError;
use crate::identifiers::{AlbumUuid, MediaUuid};
use crate::library::Library;
use crate::library::media::MediaHash;
use crate::operations::MediaFilename;

pub type Result<T> = std::result::Result<T, LibraryError>;

#[derive(Debug)]
pub enum MediaAddSource {
    /// The media at the given path is read, encrypted, and written into the library's local cache.
    CopyFrom(PathBuf),
}

#[derive(Debug)]
pub enum MediaAddResult {
    /// A new media entry was created.
    Added(MediaUuid),
    /// A media with the same content hash already exists. Returns the existing ID.
    AlreadyExists(MediaUuid),
}

impl MediaAddResult {
    #[must_use]
    pub fn id(&self) -> MediaUuid {
        match self {
            MediaAddResult::Added(id) | MediaAddResult::AlreadyExists(id) => *id,
        }
    }
}

impl Library {
    /// # Errors
    ///
    /// Returns an error if the source cannot be read, media encryption/storage fails, an associated album is absent, or the creation operation cannot be persisted.
    pub async fn media_add(
        &self,
        source: MediaAddSource,
        album_id: Option<AlbumUuid>,
        original_filename_override: Option<String>,
        apple_aae_media_id: Option<MediaUuid>,
        apple_live_photo_media_id: Option<MediaUuid>,
    ) -> Result<MediaAddResult> {
        let MediaAddSource::CopyFrom(p) = &source;
        let bytes: Vec<u8> = std::fs::read(p)?;
        let content_hash = MediaHash::from_bytes(&bytes);

        // If a media with the same hash already exists, return it.
        {
            let state = self.inner.state.read();
            if let Some(existing_ids) = state.views.by_content_hash.get(&content_hash)
                && let Some(&existing_id) = existing_ids.first()
            {
                return Ok(MediaAddResult::AlreadyExists(existing_id));
            }
        }

        let media_id = MediaUuid::from_uuid(Uuid::new_v4());

        let metadata = std::fs::metadata(p)?;
        let mtime = metadata.modified().unwrap_or_else(|_| SystemTime::now());
        let datetime: DateTime<Utc> = mtime.into();
        let filename_original = original_filename_override.unwrap_or_else(|| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });
        let size_bytes = metadata.len();
        let storage_date = crate::operations::StorageDate {
            year: u16::try_from(datetime.year())
                .map_err(|_| LibraryError::UnsupportedStorageDate)?,
            month: u8::try_from(datetime.month())
                .map_err(|_| LibraryError::UnsupportedStorageDate)?,
        };

        let master_key = &self.inner.master_key;
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();
        let file_key = derive_blob_key(master_key, &media_id.0);

        let blob = encrypt_blob(&file_key, &bytes);
        let data_path =
            local_state_media_dir.data_path(storage_date.year, storage_date.month, &media_id);
        if let Some(parent) = data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&data_path, blob.to_bytes())?;

        self.record_local_operation(
            Utc::now(),
            OperationContent::MediaCreation(MediaCreation {
                media_id,
                filename_original: MediaFilename(filename_original),
                date: datetime,
                storage_date,
                size_bytes,
                content_hash,
                modified_at: None,
                gps: None,
                apple_aae_media_id,
                apple_live_photo_media_id,
            }),
        )?;
        if let Some(album_id) = album_id {
            self.record_local_operation(
                Utc::now(),
                OperationContent::AlbumMediaAdd { album_id, media_id },
            )?;
        }

        Ok(MediaAddResult::Added(media_id))
    }

    /// Deletes the local `.data` file for each media ID, silently skipping IDs that have
    /// no local file (e.g. already evicted or never cached).
    /// # Errors
    ///
    /// Returns an error if removing a cached media blob fails.
    pub fn evict_local_data(&self, media_ids: &[MediaUuid]) -> Result<()> {
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();

        for &media_id in media_ids {
            let (year, month) = self.media_year_month(media_id)?;
            let data_path = local_state_media_dir.data_path(year, month, &media_id);
            if data_path.exists() {
                std::fs::remove_file(&data_path)?;
            }
        }

        Ok(())
    }

    /// Deletes the local `.thumb` file for each media ID, silently skipping IDs that have
    /// no local thumbnail (e.g. already evicted or never cached).
    /// # Errors
    ///
    /// Returns an error if removing a cached thumbnail fails.
    pub fn evict_local_thumbnails(&self, media_ids: &[MediaUuid]) -> Result<()> {
        let local_state_media_dir = self.inner.local_dirs.local_state_media_dir();

        for &media_id in media_ids {
            let (year, month) = self.media_year_month(media_id)?;
            let thumb_path = local_state_media_dir.thumb_path(year, month, &media_id);
            if thumb_path.exists() {
                std::fs::remove_file(&thumb_path)?;
            }
        }

        Ok(())
    }

    /// Encrypts `data` and overwrites the `.thumb` blob for `media_id`.
    ///
    /// `data` must be raw image bytes (e.g. JPEG) no larger than `THUMBNAIL_SIZE`×`THUMBNAIL_SIZE`.
    /// Year/month are resolved from the in-memory state.
    /// # Errors
    ///
    /// Returns an error if the thumbnail cannot be encrypted or written to the local cache.
    pub fn media_set_thumbnail(&self, media_id: MediaUuid, data: &[u8]) -> Result<()> {
        let (year, month) = self.media_year_month(media_id)?;
        let file_key = derive_blob_key(&self.inner.master_key, &media_id.0);
        let blob = encrypt_blob(&file_key, data);
        let thumb_path = self
            .inner
            .local_dirs
            .local_state_media_dir()
            .thumb_path(year, month, &media_id);
        if let Some(parent) = thumb_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&thumb_path, blob.to_bytes())?;
        Ok(())
    }
}
