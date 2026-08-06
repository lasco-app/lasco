use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("key not found")]
    NotFound,
    #[error("storage error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[async_trait]
pub trait Storage: Send + Sync {
    #[deprecated(note = "Use put_atomic for remote writes.")]
    async fn put(&self, key: &str, data: &[u8]) -> Result<()>;
    /// Replaces a key without exposing a partially-written value to readers.
    async fn put_atomic(&self, key: &str, data: &[u8]) -> Result<()>;
    async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool>;
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn exists(&self, key: &str) -> Result<bool>;
}

mod mock_memory;
pub use mock_memory::StorageMockMemory;

mod local_fs;
pub use local_fs::StorageLocalFs;

mod s3;
pub use s3::StorageS3;
