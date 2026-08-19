use std::collections::{BTreeMap, HashSet};

use crate::identifiers::MediaUuid;
use crate::library::local_dirs::RemoteMediaList;
use crate::library::remote_media_list_lock::RemoteMediaListLock;
use crate::operations::StorageDate;
use crate::remote::MediaList;

use super::remote_access::StorageRead;

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
    known_media: &[(MediaUuid, StorageDate)],
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
    for (media_id, storage_date) in known_media {
        if media_list.has_full(media_id) && media_list.has_thumb(media_id) {
            continue;
        }
        candidates_by_folder
            .entry((storage_date.year, storage_date.month))
            .or_default()
            .push(*media_id);
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
