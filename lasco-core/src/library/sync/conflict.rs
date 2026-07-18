// Stub — full implementation in Phase 12b §4.

use crate::identifiers::{AlbumUuid, MediaUuid};

#[allow(dead_code, reason = "Stub — full implementation in Phase 12b §4")]
#[derive(Debug)]
pub enum SyncConflict {
    AlbumDeletedWithPendingAdds { album_id: AlbumUuid, pending_media_ids: Vec<MediaUuid> },
    DefaultAlbumChanged { user: String, local_album: AlbumUuid, remote_album: AlbumUuid },
}
