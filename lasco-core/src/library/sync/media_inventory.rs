use std::collections::{BTreeMap, HashSet};

use crate::identifiers::MediaUuid;
use crate::library::local_dirs::RemoteMediaList;
use crate::library::remote_media_list_lock::RemoteMediaListLock;
use crate::operations::StorageDate;
use crate::remote::MediaList;
use crate::storage::{Result as StorageResult, StorageError};
use uuid::Uuid;

use super::remote_access::StorageRead;

/// One media to look for on a remote. `expects_thumb` is false for a companion resource,
/// which never has a thumbnail, so its inventory entry is complete without one.
pub(crate) struct KnownMedia {
    pub(crate) media_id: MediaUuid,
    pub(crate) storage_date: StorageDate,
    pub(crate) expects_thumb: bool,
}

/// Returns a complete physical snapshot of every recognized media blob on one remote.
///
/// `Storage::list_recursive` is required to exhaust backend pagination before it returns. A
/// listing error is intentionally propagated: an incomplete scan must never be treated as an
/// empty remote. Unknown files under `media/` are ignored.
pub(crate) async fn list_all_remote_media(storage: &StorageRead<'_>) -> StorageResult<MediaList> {
    let keys = match storage.list_recursive("media/").await {
        Ok(keys) => keys,
        // Filesystem-like backends do not create the directory until the first media write.
        Err(StorageError::NotFound) => Vec::new(),
        Err(error) => return Err(error),
    };

    let mut inventory = MediaList::default();
    for key in keys {
        let Some((media_id, full, thumb)) = parse_media_key(&key) else {
            continue;
        };
        inventory.record(media_id, full, thumb);
    }
    Ok(inventory)
}

fn parse_media_key(key: &str) -> Option<(MediaUuid, bool, bool)> {
    let remainder = key.strip_prefix("media/")?;
    let mut parts = remainder.split('/');
    let year = parts.next()?.parse::<u16>().ok()?;
    let month = parts.next()?.parse::<u8>().ok()?;
    if year == 0 || !(1..=12).contains(&month) || parts.clone().count() != 1 {
        return None;
    }
    let (id, extension) = parts.next()?.rsplit_once('.')?;
    let media_id = MediaUuid::from_uuid(Uuid::parse_str(id).ok()?);
    match extension {
        "data" => Some((media_id, true, false)),
        "thumb" => Some((media_id, false, true)),
        _ => None,
    }
}

/// Confirms which of the media known to the reconstructed state are present on a remote and
/// records them in that remote's positive-only inventory.
///
/// Only the blobs missing from the inventory are probed, and the data blob and the thumbnail
/// of one media are confirmed independently. Candidates are grouped by their `media/YYYY/MM/`
/// folder so one listing covers every candidate stored in that folder, both blobs included,
/// instead of one existence check per blob.
///
/// Every error is ignored. This is opportunistic bookkeeping and must never fail its caller.
/// An unconfirmed blob stays absent from the inventory, which only means unconfirmed.
///
/// Returns how many blobs it newly confirmed.
pub(crate) async fn confirm_known_media(
    storage: &StorageRead<'_>,
    known_media: &[KnownMedia],
    remote_id: &str,
    remote_media_list: &RemoteMediaList,
    remote_media_list_lock: &RemoteMediaListLock,
) -> usize {
    let Ok(media_list) =
        remote_media_list_lock.with_lock(remote_id, remote_media_list, |remote_media_list| {
            MediaList::load_or_default(&remote_media_list.media_list_path())
        })
    else {
        return 0;
    };

    let mut candidates_by_folder: BTreeMap<(u16, u8), Vec<MediaUuid>> = BTreeMap::new();
    for known in known_media {
        let thumb_settled = !known.expects_thumb || media_list.has_thumb(&known.media_id);
        if media_list.has_full(&known.media_id) && thumb_settled {
            continue;
        }
        candidates_by_folder
            .entry((known.storage_date.year, known.storage_date.month))
            .or_default()
            .push(known.media_id);
    }
    if candidates_by_folder.is_empty() {
        return 0;
    }

    let mut confirmed = Vec::new();
    for ((year, month), candidates) in candidates_by_folder {
        let prefix = format!("media/{year}/{month:02}/");
        // A folder holding no media yet reports an error on some backends, which is the same
        // as an empty listing here.
        let Ok(keys) = storage.list(&prefix).await else {
            continue;
        };
        let present: HashSet<String> = keys.into_iter().collect();
        for media_id in candidates {
            let full = present.contains(&format!("{prefix}{media_id}.data"));
            let thumb = present.contains(&format!("{prefix}{media_id}.thumb"));
            if full || thumb {
                confirmed.push((media_id, full, thumb));
            }
        }
    }
    if confirmed.is_empty() {
        return 0;
    }

    // Reload under the lock so this write preserves any observation made by another remote's
    // sync while the listings above were in progress.
    remote_media_list_lock.with_lock(remote_id, remote_media_list, |remote_media_list| {
        let path = remote_media_list.media_list_path();
        let Ok(mut media_list) = MediaList::load_or_default(&path) else {
            return 0;
        };
        let mut newly_confirmed = 0;
        for (media_id, full, thumb) in confirmed {
            if media_list.record(media_id, full, thumb) {
                newly_confirmed += 1;
            }
        }
        if newly_confirmed > 0 && media_list.save(&path).is_err() {
            return 0;
        }
        newly_confirmed
    })
}
