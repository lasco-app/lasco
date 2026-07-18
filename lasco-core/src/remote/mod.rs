pub(crate) mod last_known_state;
pub(crate) mod local_state;

pub(crate) use last_known_state::LastKnownState;
pub(crate) use local_state::media_list_json::MediaList;
pub(crate) use local_state::processed_files::ProcessedFiles;

use crate::storage::{Result, Storage};

/// Local-filesystem remote backend, carries sync-path helpers.
#[allow(
    missing_debug_implementations,
    reason = "storage field does not implement Debug"
)]
pub struct Remote<S: Storage> {
    pub(crate) storage: S,
}

/// Full (e.g. S3) remote backend
#[allow(
    missing_debug_implementations,
    reason = "storage field does not implement Debug"
)]
pub struct RemoteFull<S: Storage> {
    pub(crate) storage: S,
}

impl<S: Storage> Remote<S> {
    pub fn new_local(storage: S) -> Self {
        Self { storage }
    }

    pub async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.storage.put(key, data).await
    }

    pub async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool> {
        self.storage.put_if_absent(key, data).await
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.storage.get(key).await
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.storage.delete(key).await
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.storage.list(prefix).await
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.storage.exists(key).await
    }
}

impl<S: Storage> RemoteFull<S> {
    pub fn new_full(storage: S) -> Self {
        Self { storage }
    }

    pub async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.storage.put(key, data).await
    }

    pub async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool> {
        self.storage.put_if_absent(key, data).await
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.storage.get(key).await
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.storage.delete(key).await
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.storage.list(prefix).await
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.storage.exists(key).await
    }
}
