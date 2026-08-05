use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

/// Decides whether a fetch or push may run against a given remote, and whether a
/// fetch may run at all. Push against different remotes is independent, push
/// against the same remote is exclusive with any fetch or push already running on
/// it. Fetch is exclusive across all remotes, because two fetches racing on
/// different remotes can both decide, from stale snapshots of the local op log,
/// to append the same op group and duplicate it. Does not know anything about what
/// fetch or push actually do.
pub(crate) struct SyncPolicy {
    active_remotes: parking_lot::Mutex<HashSet<String>>,
    fetch_running: AtomicBool,
}

impl SyncPolicy {
    pub(crate) fn new() -> Self {
        Self {
            active_remotes: parking_lot::Mutex::new(HashSet::new()),
            fetch_running: AtomicBool::new(false),
        }
    }

    pub(crate) fn try_acquire_remote(&self, remote_id: &str) -> Option<RemoteSyncGuard<'_>> {
        let mut active = self.active_remotes.lock();
        if active.insert(remote_id.to_string()) {
            Some(RemoteSyncGuard {
                policy: self,
                remote_id: remote_id.to_string(),
            })
        } else {
            None
        }
    }

    pub(crate) fn try_acquire_fetch_slot(&self) -> Option<FetchSlotGuard<'_>> {
        self.fetch_running
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| FetchSlotGuard { policy: self })
    }
}

pub(crate) struct RemoteSyncGuard<'a> {
    policy: &'a SyncPolicy,
    remote_id: String,
}

impl Drop for RemoteSyncGuard<'_> {
    fn drop(&mut self) {
        self.policy.active_remotes.lock().remove(&self.remote_id);
    }
}

pub(crate) struct FetchSlotGuard<'a> {
    policy: &'a SyncPolicy,
}

impl Drop for FetchSlotGuard<'_> {
    fn drop(&mut self) {
        self.policy.fetch_running.store(false, Ordering::Release);
    }
}
