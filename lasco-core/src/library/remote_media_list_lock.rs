use std::collections::HashMap;
use std::sync::Arc;

use crate::library::local_dirs::RemoteMediaList;

/// Owns a normal mutex for each remote's `media_list.json`.
///
/// `with_lock` accepts a synchronous closure, making it impossible to hold an inventory lock
/// across remote storage awaits.
pub(crate) struct RemoteMediaListLock {
    mutexes: parking_lot::Mutex<HashMap<String, Arc<parking_lot::Mutex<()>>>>,
}

impl RemoteMediaListLock {
    pub(crate) fn new() -> Self {
        Self {
            mutexes: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn with_lock<T>(
        &self,
        remote_id: &str,
        remote_media_list: &RemoteMediaList,
        action: impl FnOnce(&RemoteMediaList) -> T,
    ) -> T {
        let mutex = self
            .mutexes
            .lock()
            .entry(remote_id.to_string())
            .or_default()
            .clone();
        let _guard = mutex.lock();
        action(remote_media_list)
    }
}
