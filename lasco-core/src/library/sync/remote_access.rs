//! Restricted views of a remote storage backend used by sync procedures.
//!
//! Keep the underlying [`Storage`] private: a procedure which receives
//! [`StorageRead`] cannot upload or delete remote data.

use crate::storage::{Result, Storage};

pub struct StorageRead<'a> {
    storage: &'a dyn Storage,
}

impl std::fmt::Debug for StorageRead<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageRead")
            .finish_non_exhaustive()
    }
}

impl<'a> StorageRead<'a> {
    pub fn new(storage: &'a dyn Storage) -> Self {
        Self { storage }
    }

    pub(crate) async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.storage.get(key).await
    }

    pub(crate) async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.storage.list(prefix).await
    }

    pub(crate) async fn exists(&self, key: &str) -> Result<bool> {
        self.storage.exists(key).await
    }
}

pub struct StorageReadWrite<'a> {
    storage: &'a dyn Storage,
}

impl std::fmt::Debug for StorageReadWrite<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageReadWrite")
            .finish_non_exhaustive()
    }
}

impl<'a> StorageReadWrite<'a> {
    pub fn new(storage: &'a dyn Storage) -> Self {
        Self { storage }
    }

    pub fn as_read(&self) -> StorageRead<'_> {
        StorageRead {
            storage: self.storage,
        }
    }

    #[allow(
        dead_code,
        reason = "Retained to keep the read/write capability surface aligned with StorageRead."
    )]
    pub(crate) async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.storage.get(key).await
    }

    pub(crate) async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.storage.list(prefix).await
    }

    #[allow(
        dead_code,
        reason = "Retained to keep the read/write capability surface aligned with StorageRead."
    )]
    pub(crate) async fn exists(&self, key: &str) -> Result<bool> {
        self.storage.exists(key).await
    }

    #[deprecated(note = "Use put_atomic for remote writes.")]
    #[allow(
        deprecated,
        reason = "Required by the current remote access compatibility layer."
    )]
    #[allow(
        dead_code,
        reason = "Retained for compatibility; current push uses put_atomic to avoid partial writes."
    )]
    pub(crate) async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.storage.put(key, data).await
    }

    pub(crate) async fn put_atomic(&self, key: &str, data: &[u8]) -> Result<()> {
        self.storage.put_atomic(key, data).await
    }

    pub(crate) async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool> {
        self.storage.put_if_absent(key, data).await
    }

    pub(crate) async fn delete(&self, key: &str) -> Result<()> {
        self.storage.delete(key).await
    }
}
