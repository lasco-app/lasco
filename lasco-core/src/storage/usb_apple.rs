//! Wired USB storage selected through Apple's document picker.
//!
//! The bookmark is resolved once and its security scope stays active until the
//! storage object is dropped, which is after the Rust sync operation releases
//! its `Box<dyn Storage>`.

#![allow(
    unsafe_code,
    reason = "Objective-C security-scope selectors are unsafe in objc2."
)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::ptr::NonNull;

use async_trait::async_trait;
use base64::Engine;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{
    NSData, NSError, NSFileCoordinator, NSFileCoordinatorReadingOptions,
    NSFileCoordinatorWritingOptions, NSURL, NSURLBookmarkResolutionOptions,
};

use super::{AtomicWriteMode, Result, Storage, StorageError, StorageLocalFs};

#[derive(Debug)]
pub struct StorageUsbApple {
    /// Retaining the URL is required until its security scope is relinquished.
    security_scoped_url: Retained<NSURL>,
}

impl StorageUsbApple {
    /// Opens the security-scoped folder represented by an Apple document-picker bookmark.
    pub fn new(bookmark_base64: &str) -> Result<Self> {
        let url = Self::resolve_bookmark(bookmark_base64)?;

        // SAFETY: Apple requires the returned security-scoped URL to remain
        // retained while access is active; this struct retains it until Drop.
        if !unsafe { url.startAccessingSecurityScopedResource() } {
            return Err(StorageError::Unavailable(
                "USB drive is unavailable or access was denied".to_string(),
            ));
        }

        if url.path().is_none() {
            // SAFETY: this is the matching stop call for the successful start
            // above; the URL remains retained for the duration of this call.
            unsafe {
                url.stopAccessingSecurityScopedResource();
            };
            return Err(StorageError::Unavailable(
                "USB bookmark did not resolve to a filesystem path".to_string(),
            ));
        }

        Ok(Self {
            security_scoped_url: url,
        })
    }

    /// Resolves a bookmark just long enough to identify its selected folder.
    ///
    /// The FFI layer uses this while adding a remote to reject a folder that
    /// overlaps an existing USB remote. The scope is always released before
    /// returning; persistent access is owned by a `StorageUsbApple` instance.
    pub fn bookmark_folder_path(bookmark_base64: &str) -> Result<PathBuf> {
        let url = Self::resolve_bookmark(bookmark_base64)?;

        // SAFETY: the URL is retained for this scope and is released only
        // after its matching `stopAccessingSecurityScopedResource` call.
        if !unsafe { url.startAccessingSecurityScopedResource() } {
            return Err(StorageError::Unavailable(
                "USB drive is unavailable or access was denied".to_string(),
            ));
        }
        let path = url.path().map(|path| PathBuf::from(path.to_string()));
        // SAFETY: this is the matching stop call for the successful start.
        unsafe {
            url.stopAccessingSecurityScopedResource();
        };
        path.ok_or_else(|| {
            StorageError::Unavailable(
                "USB bookmark did not resolve to a filesystem path".to_string(),
            )
        })
    }

    fn resolve_bookmark(bookmark_base64: &str) -> Result<Retained<NSURL>> {
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
                // iOS document-picker bookmarks resolve to security-scoped
                // URLs implicitly. The explicit `WithSecurityScope` option is
                // a macOS-only API and must not be used on iOS.
                NSURLBookmarkResolutionOptions::empty(),
                None,
                &raw mut is_stale,
            )
        }
        .map_err(|e| StorageError::Unavailable(format!("could not resolve USB bookmark: {e}")))?;

        if is_stale.as_bool() {
            return Err(StorageError::Unavailable(
                "USB bookmark is stale; select the drive again".to_string(),
            ));
        }
        Ok(url)
    }

    /// Coordinates a synchronous filesystem operation against the selected
    /// folder. NSFileCoordinator invokes its accessor before another File
    /// Provider modifies the directory, and gives us the URL valid for the
    /// duration of that accessor.
    fn coordinate<T>(
        &self,
        writing: bool,
        operation: impl FnOnce(&StorageLocalFs) -> Result<T>,
    ) -> Result<T> {
        let operation = RefCell::new(Some(operation));
        let result = RefCell::new(None);
        let accessor = RcBlock::new(|coordinated_url: NonNull<NSURL>| {
            let operation = operation
                .borrow_mut()
                .take()
                .expect("file coordinator accessor invoked more than once");
            // SAFETY: NSFileCoordinator passes a non-null URL that stays
            // valid through this synchronous accessor invocation.
            let storage = unsafe { coordinated_url.as_ref() }
                .path()
                .map(|path| StorageLocalFs::new(path.to_string()))
                .ok_or_else(|| {
                    StorageError::Unavailable(
                        "USB drive did not yield a coordinated filesystem path".to_string(),
                    )
                });
            *result.borrow_mut() = Some(storage.and_then(|storage| operation(&storage)));
        });
        let coordinator = NSFileCoordinator::new();
        let mut coordination_error: Option<Retained<NSError>> = None;

        if writing {
            coordinator.coordinateWritingItemAtURL_options_error_byAccessor(
                &self.security_scoped_url,
                NSFileCoordinatorWritingOptions::empty(),
                Some(&mut coordination_error),
                &accessor,
            );
        } else {
            coordinator.coordinateReadingItemAtURL_options_error_byAccessor(
                &self.security_scoped_url,
                NSFileCoordinatorReadingOptions::empty(),
                Some(&mut coordination_error),
                &accessor,
            );
        }
        // The accessor holds the only borrow of `result`; file coordination is
        // synchronous, so it is safe to release the block before consuming it.
        drop(accessor);

        if let Some(error) = coordination_error {
            return Err(StorageError::Unavailable(format!(
                "USB drive file coordination failed: {error}"
            )));
        }
        result.into_inner().unwrap_or_else(|| {
            Err(StorageError::Unavailable(
                "USB drive did not grant coordinated file access".to_string(),
            ))
        })
    }
}

impl Drop for StorageUsbApple {
    fn drop(&mut self) {
        // SAFETY: the matching successful start call is made in `new`, and the
        // retained URL remains valid for the entire lifetime of this object.
        unsafe {
            self.security_scoped_url
                .stopAccessingSecurityScopedResource();
        };
    }
}

#[async_trait]
impl Storage for StorageUsbApple {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.coordinate(true, |storage| storage.put_sync(key, data))
    }

    async fn put_atomic(&self, key: &str, data: &[u8], mode: AtomicWriteMode) -> Result<bool> {
        self.coordinate(true, |storage| storage.put_atomic_sync(key, data, mode))
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.coordinate(false, |storage| storage.get_sync(key))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.coordinate(true, |storage| storage.delete_sync(key))
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.coordinate(false, |storage| storage.list_sync(prefix))
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.coordinate(false, |storage| storage.exists_sync(key))
    }
}
