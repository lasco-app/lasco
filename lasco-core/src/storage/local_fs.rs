use std::fs;
use std::io;
use std::path::PathBuf;

use async_trait::async_trait;
use walkdir::WalkDir;

use super::{Result, Storage, StorageError};

#[derive(Debug)]
pub struct StorageLocalFs {
    root: PathBuf,
}

impl StorageLocalFs {
    #[must_use]
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self { root }
    }
}

#[async_trait]
impl Storage for StorageLocalFs {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        let path = self.root.join(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StorageError::Other(Box::new(e)))?;
        }
        fs::write(path, data).map_err(|e| StorageError::Other(Box::new(e)))
    }

    async fn put_atomic(&self, key: &str, data: &[u8]) -> Result<()> {
        let path = self.root.join(key);
        let temp_path = path.with_file_name(format!(
            "{}.temp",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StorageError::Other(Box::new(e)))?;
        }
        fs::write(&temp_path, data).map_err(|e| StorageError::Other(Box::new(e)))?;
        fs::rename(temp_path, path).map_err(|e| StorageError::Other(Box::new(e)))
    }

    /// Not atomic across processes — acceptable for single-client testing.
    async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool> {
        let path = self.root.join(key);
        if path.exists() {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StorageError::Other(Box::new(e)))?;
        }
        fs::write(path, data).map_err(|e| StorageError::Other(Box::new(e)))?;
        Ok(true)
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.root.join(key);
        fs::read(&path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                StorageError::NotFound
            } else {
                StorageError::Other(Box::new(e))
            }
        })
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.root.join(key);
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Other(Box::new(e))),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let base = self.root.join(prefix);
        if !base.exists() {
            return Err(StorageError::NotFound);
        }
        let mut keys = Vec::new();
        for entry in WalkDir::new(&base)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if entry.file_type().is_file()
                && let Ok(rel) = entry.path().strip_prefix(&self.root)
                && let Some(s) = rel.to_str()
            {
                keys.push(s.to_owned());
            }
        }
        Ok(keys)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.root.join(key).exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store() -> (StorageLocalFs, TempDir) {
        let dir = TempDir::new().unwrap();
        let s = StorageLocalFs::new(dir.path());
        (s, dir)
    }

    #[tokio::test]
    async fn put_then_get_returns_identical_bytes() {
        let (s, _dir) = store();
        s.put_atomic("k", b"hello").await.unwrap();
        assert_eq!(s.get("k").await.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn put_atomic_replaces_file_and_removes_temp_file() {
        let (s, dir) = store();
        s.put_atomic("nested/file", b"old").await.unwrap();
        s.put_atomic("nested/file", b"new").await.unwrap();

        assert_eq!(s.get("nested/file").await.unwrap(), b"new");
        assert!(!dir.path().join("nested/file.temp").exists());
    }

    #[tokio::test]
    async fn get_missing_key_returns_not_found() {
        let (s, _dir) = store();
        assert!(matches!(
            s.get("missing").await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let (s, _dir) = store();
        s.put_atomic("k", b"v").await.unwrap();
        s.delete("k").await.unwrap();
        assert!(matches!(s.get("k").await, Err(StorageError::NotFound)));
    }

    #[tokio::test]
    async fn delete_missing_key_is_ok() {
        let (s, _dir) = store();
        s.delete("nope").await.unwrap();
    }

    #[tokio::test]
    async fn list_returns_only_matching_prefix() {
        let (s, _dir) = store();
        s.put_atomic("operations/a", b"1").await.unwrap();
        s.put_atomic("operations/b", b"2").await.unwrap();
        s.put_atomic("other/c", b"3").await.unwrap();
        let mut keys = s.list("operations/").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["operations/a", "operations/b"]);
    }

    #[tokio::test]
    async fn list_nonexistent_prefix_returns_not_found() {
        let (s, _dir) = store();
        assert!(matches!(s.list("nope/").await, Err(StorageError::NotFound)));
    }

    #[tokio::test]
    async fn new_does_not_create_root_dir() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("does_not_exist_yet");
        let s = StorageLocalFs::new(&root);
        assert!(!root.exists());
        assert!(matches!(s.list("").await, Err(StorageError::NotFound)));
    }

    #[tokio::test]
    async fn exists_after_put_and_missing() {
        let (s, _dir) = store();
        assert!(!s.exists("k").await.unwrap());
        s.put_atomic("k", b"v").await.unwrap();
        assert!(s.exists("k").await.unwrap());
    }

    #[tokio::test]
    async fn put_if_absent_new_key_returns_true_existing_returns_false() {
        let (s, _dir) = store();
        assert!(s.put_if_absent("k", b"original").await.unwrap());
        assert!(!s.put_if_absent("k", b"overwrite").await.unwrap());
        assert_eq!(s.get("k").await.unwrap(), b"original");
    }

    #[tokio::test]
    async fn nested_key_paths_created_and_retrieved() {
        let (s, _dir) = store();
        s.put_atomic("a/b/c.bin", b"deep").await.unwrap();
        assert_eq!(s.get("a/b/c.bin").await.unwrap(), b"deep");
    }
}
