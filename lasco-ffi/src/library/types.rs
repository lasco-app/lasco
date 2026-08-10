#[derive(uniffi::Record, Debug)]
pub struct FfiCreateLibraryResult {
    pub library_id: String,
    pub master_key_hex: String,
}

// already_existed is true when a media with the same content hash was already
// in the library, in which case media_id is the existing entry and nothing was
// written. Callers use it to skip work they would otherwise redo, such as
// regenerating a thumbnail.
#[derive(uniffi::Record, Debug)]
pub struct FfiMediaAddResult {
    pub media_id: String,
    pub already_existed: bool,
}

/// A media identifier returned to clients when a local-only push cannot find
/// every required original in this device's cache.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMediaId {
    pub value: String,
}

impl From<lasco_core::identifiers::MediaUuid> for FfiMediaId {
    fn from(value: lasco_core::identifiers::MediaUuid) -> Self {
        Self {
            value: value.to_string(),
        }
    }
}

#[derive(uniffi::Record, Debug)]
pub struct FfiMediaItem {
    pub media_id: String,
    pub filename_original: String,
    pub name: Option<String>,
    pub date: String,
    pub year: u16,
    pub month: u8,
    pub size_bytes: u64,
    pub content_hash: String,
    pub author: String,
    pub apple_aae_media_id: Option<String>,
    pub apple_live_photo_media_id: Option<String>,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiMediaNeighbors {
    pub previous: Option<FfiMediaItem>,
    pub current: FfiMediaItem,
    pub next: Option<FfiMediaItem>,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiAlbum {
    pub album_id: String,
    pub name: String,
    pub parent_album_id: Option<String>,
    pub media_count: u32,
    pub deleted: bool,
    pub is_disconnected: bool,
    pub thumbnail_media_id: Option<String>,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiGroup {
    pub group_id: String,
    pub album_id_parent: String,
    pub media_ids: Vec<String>,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiAlbumItem {
    pub kind: String,
    pub media: Option<FfiMediaItem>,
    pub group: Option<FfiGroup>,
    pub effective_date: String,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiMediaOrGroupNeighbors {
    pub previous: Option<FfiAlbumItem>,
    pub current: FfiAlbumItem,
    pub next: Option<FfiAlbumItem>,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiSyncResult {
    pub pushed: u32,
    pub pulled: u32,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiLibraryEntry {
    pub id: String,
    pub nickname: String,
    pub username: Option<String>,
    pub load_error: Option<String>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiRemote {
    pub id: String,
    pub name: String,
    pub auto_push: bool,
    pub kind: String,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub path: Option<String>,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiKv {
    pub key: String,
    pub value: String,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiOperation {
    pub kind: String,
    pub timestamp: String,
    pub args: Vec<FfiKv>,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiLocalStateStats {
    pub media_cached_count: u32,
    pub media_cached_bytes: u64,
    pub thumb_cached_count: u32,
    pub thumb_cached_bytes: u64,
}

#[derive(uniffi::Record, Debug)]
pub struct FfiOperationGroup {
    pub op_id: String,
    pub parent_op_id: Option<String>,
    pub operations: Vec<FfiOperation>,
    pub author: String,
}
