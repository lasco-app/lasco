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
/// Only media missing from the inventory are probed. Candidates are grouped by their
/// `media/YYYY/MM/` folder so one listing covers every candidate stored in that folder,
/// instead of one existence check per media.
///
/// Every error is ignored. This is opportunistic bookkeeping and must never fail its caller.
/// An unconfirmed media stays absent from the inventory, which only means unconfirmed.
pub(crate) async fn confirm_known_media(
    storage: &StorageRead<'_>,
    known_media: &[(MediaUuid, StorageDate)],
    remote_id: &str,
    remote_media_list: &RemoteMediaList,
    remote_media_list_lock: &RemoteMediaListLock,
) {
    let Ok(media_list) =
        remote_media_list_lock.with_lock(remote_id, remote_media_list, |remote_media_list| {
            MediaList::load_or_default(&remote_media_list.media_list_path())
        })
    else {
        return;
    };

    let mut candidates_by_folder: BTreeMap<(u16, u8), Vec<MediaUuid>> = BTreeMap::new();
    for (media_id, storage_date) in known_media {
        if media_list.contains(media_id) {
            continue;
        }
        candidates_by_folder
            .entry((storage_date.year, storage_date.month))
            .or_default()
            .push(*media_id);
    }
    if candidates_by_folder.is_empty() {
        return;
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
            if present.contains(&format!("{prefix}{media_id}.data")) {
                confirmed.push(media_id);
            }
        }
    }
    if confirmed.is_empty() {
        return;
    }

    // Reload under the lock so this write preserves any observation made by another remote's
    // sync while the listings above were in progress.
    remote_media_list_lock.with_lock(remote_id, remote_media_list, |remote_media_list| {
        let path = remote_media_list.media_list_path();
        let Ok(mut media_list) = MediaList::load_or_default(&path) else {
            return;
        };
        let mut changed = false;
        for media_id in confirmed {
            changed |= media_list.insert_present(media_id);
        }
        if changed {
            let _ = media_list.save(&path);
        }
    });
}
