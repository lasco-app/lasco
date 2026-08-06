//! Wired USB storage selected through Apple's document picker.
//!
//! The bookmark is resolved once and its security scope stays active until the
//! storage object is dropped, which is after the Rust sync operation releases
//! its `Box<dyn Storage>`.

#![allow(unsafe_code)] // Objective-C security-scope selectors are unsafe in objc2.

use async_trait::async_trait;
use base64::Engine;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSData, NSURL, NSURLBookmarkResolutionOptions};

use super::{Result, Storage, StorageError, StorageLocalFs};

#[derive(Debug)]
pub struct StorageUsbApple {
    storage: StorageLocalFs,
    /// Retaining the URL is required until its security scope is relinquished.
    security_scoped_url: Retained<NSURL>,
}

impl StorageUsbApple {
    pub fn new(bookmark_base64: &str) -> Result<Self> {
        let bookmark = base64::engine::general_purpose::STANDARD
            .decode(bookmark_base64)
            .map_err(|e| StorageError::Unavailable(format!("invalid USB bookmark: {e}")))?;
        let bookmark = NSData::with_bytes(&bookmark);
        let mut is_stale = Bool::from(false);

        // SAFETY: `bookmark` is owned for the duration of this call and
        // `is_stale` is a valid mutable Boolean out-parameter.
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &bookmark,
                NSURLBookmarkResolutionOptions::WithSecurityScope,
                None,
                &mut is_stale,
            )
        }
        .map_err(|e| StorageError::Unavailable(format!("could not resolve USB bookmark: {e}")))?;

        if is_stale.as_bool() {
            return Err(StorageError::Unavailable(
                "USB bookmark is stale; select the drive again".to_string(),
            ));
        }

        // SAFETY: Apple requires the returned security-scoped URL to remain
        // retained while access is active; this struct retains it until Drop.
        if !unsafe { url.startAccessingSecurityScopedResource() } {
            return Err(StorageError::Unavailable(
                "USB drive is unavailable or access was denied".to_string(),
            ));
        }

        let path = url.path().ok_or_else(|| {
            StorageError::Unavailable("USB bookmark did not resolve to a filesystem path".to_string())
        })?;

        Ok(Self {
            storage: StorageLocalFs::new(path.to_string())?,
            security_scoped_url: url,
        })
    }
}

impl Drop for StorageUsbApple {
    fn drop(&mut self) {
        // SAFETY: the matching successful start call is made in `new`, and the
        // retained URL remains valid for the entire lifetime of this object.
        unsafe { self.security_scoped_url.stopAccessingSecurityScopedResource() };
    }
}

#[async_trait]
impl Storage for StorageUsbApple {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.storage.put(key, data).await
    }

    async fn put_if_absent(&self, key: &str, data: &[u8]) -> Result<bool> {
        self.storage.put_if_absent(key, data).await
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.storage.get(key).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.storage.delete(key).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.storage.list(prefix).await
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.storage.exists(key).await
    }
}
