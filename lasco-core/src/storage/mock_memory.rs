use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use super::{Result, Storage, StorageError};

#[derive(Clone, Debug, Default)]
pub struct StorageMockMemory {
    data: Arc<Mutex<FxHashMap<String, Vec<u8>>>>,
    pub list_call_count: Arc<AtomicUsize>,
    pub get_call_count: Arc<AtomicUsize>,
    offline: Arc<std::sync::atomic::AtomicBool>,
}

impl StorageMockMemory {
    /// Force all operations to return an error (simulates unreachable remote).
    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
    }

    pub fn list_call_count(&self) -> usize {
        self.list_call_count.load(Ordering::SeqCst)
    }

    pub fn get_call_count(&self) -> usize {
        self.get_call_count.load(Ordering::SeqCst)
    }
}

impl StorageMockMemory {
    pub fn new() -> Self {
        Self::default()
    }

    fn check_online(&self) -> Result<()> {
        if self.offline.load(Ordering::SeqCst) {
            Err(StorageError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "remote unreachable (offline simulation)",
            ))))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl Storage for StorageMockMemory {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.check_online()?;
        self.data.lock().insert(key.to_owned(), data.to_vec());
        Ok(())
    }

    async fn put_atomic(&self, key: &str, data: &[u8]) -> Result<()> {
        self.check_online()?;
        self.data.lock().insert(key.to_owned(), data.to_vec());
        Ok(())
    }

    async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool> {
        self.check_online()?;
        use std::collections::hash_map::Entry;
        let mut guard = self.data.lock();
        match guard.entry(key.to_owned()) {
            Entry::Vacant(e) => {
                e.insert(data.to_vec());
                Ok(true)
            }
            Entry::Occupied(_) => Ok(false),
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.check_online()?;
        self.get_call_count.fetch_add(1, Ordering::SeqCst);
        self.data
            .lock()
            .get(key)
            .cloned()
            .ok_or(StorageError::NotFound)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.check_online()?;
        self.data.lock().remove(key);
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.check_online()?;
        self.list_call_count.fetch_add(1, Ordering::SeqCst);
        let keys = self
            .data
            .lock()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.check_online()?;
        Ok(self.data.lock().contains_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_then_get_returns_identical_bytes() {
        let s = StorageMockMemory::new();
        s.put_atomic("k", b"hello").await.unwrap();
        assert_eq!(s.get("k").await.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn get_missing_key_returns_not_found() {
        let s = StorageMockMemory::new();
        assert!(matches!(s.get("missing").await, Err(StorageError::NotFound)));
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let s = StorageMockMemory::new();
        s.put_atomic("k", b"v").await.unwrap();
        s.delete("k").await.unwrap();
        assert!(matches!(s.get("k").await, Err(StorageError::NotFound)));
    }

    #[tokio::test]
    async fn list_returns_only_matching_prefix() {
        let s = StorageMockMemory::new();
        s.put_atomic("files/a", b"1").await.unwrap();
        s.put_atomic("files/b", b"2").await.unwrap();
        s.put_atomic("other/c", b"3").await.unwrap();
        let mut keys = s.list("files/").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["files/a", "files/b"]);
    }

    #[tokio::test]
    async fn exists_after_put_and_missing() {
        let s = StorageMockMemory::new();
        assert!(!s.exists("k").await.unwrap());
        s.put_atomic("k", b"v").await.unwrap();
        assert!(s.exists("k").await.unwrap());
    }

    #[tokio::test]
    async fn put_if_absent_new_key_returns_true_existing_returns_false() {
        let s = StorageMockMemory::new();
        assert!(s.put_if_absent("k", b"original").await.unwrap());
        assert!(!s.put_if_absent("k", b"overwrite").await.unwrap());
        assert_eq!(s.get("k").await.unwrap(), b"original");
    }
}
