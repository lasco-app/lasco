use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("key not found")]
    NotFound,
    #[error("storage unavailable: {0}")]
    Unavailable(String),
    #[error("storage error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Publication policy for [`Storage::put_atomic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicWriteMode {
    /// Atomically replace any existing value.
    Replace,
    /// Atomically create the value only when the key is absent.
    CreateIfAbsent,
}

#[async_trait]
pub trait Storage: Send + Sync {
    #[deprecated(note = "Use put_atomic for remote writes.")]
    async fn put(&self, key: &str, data: &[u8]) -> Result<()>;
    /// Atomically publishes `data` according to `mode`, without exposing a partial value.
    ///
    /// Returns whether this invocation published the value. `Replace` always returns `true`;
    /// `CreateIfAbsent` returns `false` and leaves the existing value untouched when the key
    /// already exists. Backends must implement the existence check and publication as one action.
    async fn put_atomic(&self, key: &str, data: &[u8], mode: AtomicWriteMode) -> Result<bool>;
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    /// Returns the keys of the objects directly under `prefix`, without descending into
    /// nested prefixes. A key whose remainder after `prefix` still contains a separator is
    /// left out, so subdirectories are invisible rather than reported as entries. Listing
    /// `media/` therefore yields nothing, since every media key sits under `YYYY/MM/`.
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn exists(&self, key: &str) -> Result<bool>;
}

mod mock_memory;
pub use mock_memory::StorageMockMemory;

mod mock_memory_faulty;
pub use mock_memory_faulty::{StorageMockMemoryFaulty, StorageMockOperation};

mod local_fs;
pub use local_fs::StorageLocalFs;

#[cfg(target_os = "android")]
mod usb_android;
#[cfg(target_os = "android")]
pub use usb_android::{StorageUsbAndroid, initialize_android_runtime};

#[cfg(target_vendor = "apple")]
mod usb_apple;
#[cfg(target_vendor = "apple")]
pub use usb_apple::StorageUsbApple;

mod s3;
pub use s3::StorageS3;

mod lasco_cloud_s3;
pub use lasco_cloud_s3::StorageLascoCloudS3;
