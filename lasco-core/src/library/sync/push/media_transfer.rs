//! Media-blob transfer helpers used by Push.

use super::super::remote_access::StorageRead;
use super::super::{MediaBlob, PlannedMediaSource, PushMediaSource};
use crate::encryption::blob::BlobEncrypted;
use crate::encryption::blob_key::derive_blob_key;
use crate::error::LibraryError;
use crate::identifiers::{MediaUuid, RemoteUuid};

/// Where to read one thumbnail that is not cached locally.
///
/// A plan names a source per blob, so a thumbnail can come from a different remote than its
/// original. Returning `None` leaves the thumbnail unconfirmed on the target, which is never an
/// error: nothing records whether a media ever had a thumbnail.
pub(super) fn thumb_source<'a>(
    media_source: &'a PushMediaSource<'a>,
    relay_source: &Option<(RemoteUuid, &'a StorageRead<'a>)>,
    media_id: MediaUuid,
) -> Option<&'a StorageRead<'a>> {
    match media_source {
        PushMediaSource::Plan(plan) => match plan.assignments.get(&(media_id, MediaBlob::Thumb))? {
            PlannedMediaSource::Local => None,
            PlannedMediaSource::Remote(id) => plan.sources.get(id),
        },
        _ => relay_source.as_ref().map(|(_, source)| *source),
    }
}

/// Download one encrypted blob into an isolated staging file, prove it decrypts, then return
/// its bytes and path. Callers remove the path immediately after the target upload succeeds.
pub(super) fn stage_and_validate_media(
    staging_dir: &std::path::Path,
    bytes: &[u8],
    media_id: MediaUuid,
    master_key: &crate::encryption::master_key::MasterKey,
) -> Result<(Vec<u8>, std::path::PathBuf), LibraryError> {
    let path = staging_dir.join(format!("{}.stage", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes)?;
    let staged = std::fs::read(&path)?;
    let blob = BlobEncrypted::from_bytes(&staged).map_err(crate::error::OperationError::Blob)?;
    let file_key = derive_blob_key(master_key, &media_id.0);
    crate::encryption::blob::decrypt_blob(&file_key, &blob)
        .map_err(crate::error::OperationError::Crypto)?;
    Ok((staged, path))
}
