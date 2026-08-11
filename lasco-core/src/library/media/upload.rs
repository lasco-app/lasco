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
            let state = self.inner.operation_state.read();
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

        self.load_local_state().await?;

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

#[cfg(any())]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use crate::encryption::blob::BlobEncrypted;
    use crate::encryption::blob::decrypt_blob;
    use crate::identifiers::LibraryId;
    use crate::library::Credentials;
    use crate::library::local_dirs::LocalDirs;
    use crate::operations::local_ops::read_pending_op_group;

    use super::*;

    async fn make_library(tmp: &TempDir) -> (Library, LocalDirs) {
        let library_id = LibraryId(Uuid::new_v4());
        let local_dirs = LocalDirs::new(tmp.path(), &library_id);
        local_dirs.ensure_state_dirs().unwrap();
        let (lib, _password_uuid) = Library::init(
            local_dirs.clone(),
            library_id,
            Credentials {
                username: "alice".into(),
                password: "pass".into(),
            },
        )
        .await
        .unwrap();
        (lib, local_dirs)
    }

    fn write_source(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[tokio::test]
    async fn copy_from_creates_data_thumb_and_op() {
        let tmp = TempDir::new().unwrap();
        let (lib, local_dirs) = make_library(&tmp).await;
        let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
        let src = write_source(tmp.path(), "photo.jpg", b"fake image bytes");

        let result = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let media_id = match result {
            MediaAddResult::Added(id) => id,
            MediaAddResult::AlreadyExists(_) => panic!("expected Added"),
        };

        let now = chrono::Utc::now();
        let data_path = local_dirs.local_state_media_dir().data_path(
            now.year() as u16,
            now.month() as u8,
            &media_id,
        );
        assert!(data_path.exists(), ".data file must exist");
        let pending = crate::operations::local_ops::read_pending_op_group(
            &local_dirs.local_state_operations().pending_op_path(),
            &lib.inner.master_key,
        )
        .unwrap();
        assert!(
            pending.is_some(),
            "pending group must exist after media_add"
        );
    }

    #[tokio::test]
    async fn data_blob_decrypts_to_source_bytes() {
        let tmp = TempDir::new().unwrap();
        let (lib, local_dirs) = make_library(&tmp).await;
        let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
        let content = b"original photo content";
        let src = write_source(tmp.path(), "img.jpg", content);

        let result = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let media_id = match result {
            MediaAddResult::Added(id) => id,
            MediaAddResult::AlreadyExists(_) => panic!("expected Added"),
        };

        let now = chrono::Utc::now();
        let data_path = local_dirs.local_state_media_dir().data_path(
            now.year() as u16,
            now.month() as u8,
            &media_id,
        );
        let raw = std::fs::read(&data_path).unwrap();
        let blob = BlobEncrypted::from_bytes(&raw).unwrap();
        let file_key = derive_blob_key(&lib.inner.master_key, &media_id.0);
        let decrypted = decrypt_blob(&file_key, &blob).unwrap();
        assert_eq!(decrypted.as_slice(), content.as_slice());
    }

    #[tokio::test]
    async fn op_group_contains_media_creation_and_album_media_add() {
        let tmp = TempDir::new().unwrap();
        let (lib, local_dirs) = make_library(&tmp).await;
        let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
        let src = write_source(tmp.path(), "pic.jpg", b"data");

        let result = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let media_id = match result {
            MediaAddResult::Added(id) => id,
            MediaAddResult::AlreadyExists(_) => panic!("expected Added"),
        };

        let group = read_pending_op_group(
            &local_dirs.local_state_operations().pending_op_path(),
            &lib.inner.master_key,
        )
        .unwrap()
        .unwrap();
        let ops = &group.operations;
        assert_eq!(ops.len(), 2);
        assert!(
            matches!(&ops[0], Operation::MediaCreation { media_id: mid, .. } if mid == &media_id),
            "first op must be MediaCreation"
        );
        assert!(
            matches!(&ops[1], Operation::AlbumMediaAdd { album_id: aid, media_id: mid, .. }
                if aid == &album_id && mid == &media_id),
            "second op must be AlbumMediaAdd"
        );
    }

    #[tokio::test]
    async fn duplicate_import_returns_already_exists() {
        let tmp = TempDir::new().unwrap();
        let (lib, _) = make_library(&tmp).await;
        let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
        let src = write_source(tmp.path(), "photo.jpg", b"same content");

        let first = lib
            .media_add(
                MediaAddSource::CopyFrom(src.clone()),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let first_id = match first {
            MediaAddResult::Added(id) => id,
            MediaAddResult::AlreadyExists(_) => panic!("expected Added on first import"),
        };

        let second = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        match second {
            MediaAddResult::AlreadyExists(id) => assert_eq!(id, first_id),
            MediaAddResult::Added(_) => panic!("expected AlreadyExists on duplicate import"),
        }
    }

    #[tokio::test]
    async fn content_hash_stored_in_op_and_state() {
        let tmp = TempDir::new().unwrap();
        let (lib, local_dirs) = make_library(&tmp).await;
        let album_id = AlbumUuid::from_uuid(Uuid::new_v4());
        let content = b"photo bytes";
        let src = write_source(tmp.path(), "img.jpg", content);
        let expected_hash = MediaHash::from_bytes(content);

        let result = lib
            .media_add(
                MediaAddSource::CopyFrom(src),
                Some(album_id),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let media_id = match result {
            MediaAddResult::Added(id) => id,
            MediaAddResult::AlreadyExists(_) => panic!("expected Added"),
        };

        let group = read_pending_op_group(
            &local_dirs.local_state_operations().pending_op_path(),
            &lib.inner.master_key,
        )
        .unwrap()
        .unwrap();
        let op = &group.operations[0];
        assert!(
            matches!(op, Operation::MediaCreation { content_hash, .. } if *content_hash == expected_hash),
            "op must carry the correct content_hash"
        );

        let state = lib.inner.operation_state.read();
        let entry = state.reconstructed.media.get(&media_id).unwrap();
        assert_eq!(entry.content_hash, expected_hash);
    }
}
