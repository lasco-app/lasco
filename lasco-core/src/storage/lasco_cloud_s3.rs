//! Lazily refreshed Lasco Cloud S3 storage.

use async_trait::async_trait;

use crate::identifiers::RemoteUuid;
use crate::library::cloud::{CloudError, SharedCloudRuntime};

use super::{AtomicWriteMode, Result, Storage, StorageError, StorageS3};

/// S3 storage backed by a Lasco Cloud runtime session.
///
/// Every operation checks expiry immediately before it starts. Credentials are
/// consequently refreshed only when a remote is actually used, rather than by
/// a background timer. A recognised authentication failure retries safe reads
/// once after refresh to tolerate clock skew and server-side early expiry.
#[derive(Clone, Debug)]
pub struct StorageLascoCloudS3 {
    remote_id: RemoteUuid,
    runtime: SharedCloudRuntime,
}

impl StorageLascoCloudS3 {
    #[must_use]
    pub fn new(remote_id: RemoteUuid, runtime: SharedCloudRuntime) -> Self {
        Self { remote_id, runtime }
    }

    async fn current_storage(&self) -> Result<StorageS3> {
        self.runtime
            .refresh_if_needed(&self.remote_id)
            .await
            .map_err(cloud_error)?;
        self.storage_from_current_state()
    }

    fn storage_from_current_state(&self) -> Result<StorageS3> {
        let credentials = self
            .runtime
            .credentials(&self.remote_id)
            .map_err(cloud_error)?;
        StorageS3::new_with_session_token(
            &credentials.endpoint,
            &credentials.bucket,
            &credentials.region,
            (!credentials.path_prefix.is_empty()).then_some(credentials.path_prefix.as_str()),
            &credentials.access_key_id,
            &credentials.secret_access_key,
            credentials.session_token.as_deref(),
        )
    }

    async fn refresh_after_auth_failure(&self) -> Result<StorageS3> {
        self.runtime.refresh_now().await.map_err(cloud_error)?;
        self.storage_from_current_state()
    }
}

fn cloud_error(error: CloudError) -> StorageError {
    StorageError::Unavailable(error.to_string())
}

fn is_authentication_error(error: &StorageError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        "403",
        "401",
        "accessdenied",
        "expiredtoken",
        "expired token",
        "invalidaccesskeyid",
        "signaturedoesnotmatch",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

#[async_trait]
impl Storage for StorageLascoCloudS3 {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.put_atomic(key, data, AtomicWriteMode::Replace)
            .await
            .map(|_| ())
    }

    async fn put_atomic(&self, key: &str, data: &[u8], mode: AtomicWriteMode) -> Result<bool> {
        let storage = self.current_storage().await?;
        match storage.put_atomic(key, data, mode).await {
            Ok(result) => Ok(result),
            // Replacing an object is idempotent. Do not replay create-if-absent:
            // the first attempt might have committed before its response failed.
            Err(error) if mode == AtomicWriteMode::Replace && is_authentication_error(&error) => {
                self.refresh_after_auth_failure()
                    .await?
                    .put_atomic(key, data, mode)
                    .await
            }
            Err(error) => Err(error),
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let storage = self.current_storage().await?;
        match storage.get(key).await {
            Ok(value) => Ok(value),
            Err(error) if is_authentication_error(&error) => {
                self.refresh_after_auth_failure().await?.get(key).await
            }
            Err(error) => Err(error),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let storage = self.current_storage().await?;
        match storage.delete(key).await {
            Ok(()) => Ok(()),
            Err(error) if is_authentication_error(&error) => {
                self.refresh_after_auth_failure().await?.delete(key).await
            }
            Err(error) => Err(error),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let storage = self.current_storage().await?;
        match storage.list(prefix).await {
            Ok(value) => Ok(value),
            Err(error) if is_authentication_error(&error) => {
                self.refresh_after_auth_failure().await?.list(prefix).await
            }
            Err(error) => Err(error),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let storage = self.current_storage().await?;
        match storage.exists(key).await {
            Ok(value) => Ok(value),
            Err(error) if is_authentication_error(&error) => {
                self.refresh_after_auth_failure().await?.exists(key).await
            }
            Err(error) => Err(error),
        }
    }
}
