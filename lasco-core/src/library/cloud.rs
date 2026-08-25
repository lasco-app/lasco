//! Lasco Cloud connection state and encrypted resolved-S3 credential cache.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use tokio::sync::Mutex as AsyncMutex;

use crate::encryption::master_key::MasterKey;
use crate::identifiers::{LibraryId, RemoteUuid};

use super::cloud_runtime_cache::{CachedCloudS3Credentials, CloudRuntimeCache};
use super::lasco_cloud_auth::{LascoCloudAuthError, LascoCloudAuthManager};

/// Refresh when one minute or less remains. This keeps a long upload from
/// starting an S3 request with credentials that will expire mid-operation.
pub const REFRESH_SAFETY_MARGIN: Duration = Duration::minutes(1);

#[derive(Clone, Debug)]
pub struct CloudS3Credentials {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub path_prefix: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct CloudRemoteState {
    pub cloud_storage_id: String,
    pub credentials: Option<CloudS3Credentials>,
}

#[derive(Default)]
struct CloudRuntimeState {
    auth: Option<Arc<LascoCloudAuthManager>>,
    remotes: HashMap<RemoteUuid, CloudRemoteState>,
    credentials_by_storage_id: HashMap<String, CloudS3Credentials>,
}

/// Shared Cloud state for one open library.
///
/// `refresh_lock` deliberately covers all Cloud remotes: the service returns
/// credentials as one set, so concurrent transfers coalesce into one refresh.
pub struct CloudRuntime {
    state: Mutex<CloudRuntimeState>,
    refresh_lock: AsyncMutex<()>,
    library_id: LibraryId,
    cache: CloudRuntimeCache,
}

impl std::fmt::Debug for CloudRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudRuntime").finish_non_exhaustive()
    }
}

impl CloudRuntime {
    pub(crate) fn open(
        cache_path: std::path::PathBuf,
        library_id: LibraryId,
        master_key: &MasterKey,
    ) -> Self {
        let cache = CloudRuntimeCache::new(cache_path, library_id, master_key);
        let credentials_by_storage_id = match cache.load() {
            Ok(credentials) => credentials
                .into_iter()
                .map(|(id, credential)| (id, from_cached(credential)))
                .collect(),
            Err(error) => {
                tracing::warn!(path = %cache.path().display(), "ignoring unusable Lasco Cloud credential cache: {error}");
                HashMap::new()
            }
        };
        Self {
            state: Mutex::new(CloudRuntimeState {
                auth: None,
                remotes: HashMap::new(),
                credentials_by_storage_id,
            }),
            refresh_lock: AsyncMutex::new(()),
            library_id,
            cache,
        }
    }
    pub async fn configure_auth(&self, base_url: String) -> Result<(), CloudError> {
        if base_url.trim().is_empty() {
            return Err(CloudError::InvalidSession);
        }
        let auth = LascoCloudAuthManager::new(self.library_id, base_url);
        auth.restore().await.map_err(CloudError::Auth)?;
        self.state.lock().auth = Some(auth);
        Ok(())
    }

    pub fn lasco_cloud_auth(&self) -> Option<Arc<LascoCloudAuthManager>> {
        self.state.lock().auth.clone()
    }

    pub async fn clear_auth_and_credentials(&self) -> Result<(), CloudError> {
        let auth = self.state.lock().auth.take();
        if let Some(auth) = auth {
            auth.clear_local_session().await.map_err(CloudError::Auth)?;
        }
        self.clear_persisted_credentials()
    }

    pub fn clear_persisted_credentials(&self) -> Result<(), CloudError> {
        let mut state = self.state.lock();
        state.credentials_by_storage_id.clear();
        for remote in state.remotes.values_mut() {
            remote.credentials = None;
        }
        self.cache
            .clear()
            .map_err(|error| CloudError::Cache(error.to_string()))
    }

    pub fn register_remote(&self, remote_id: RemoteUuid, cloud_storage_id: String) {
        let mut state = self.state.lock();
        let credentials = state
            .credentials_by_storage_id
            .get(&cloud_storage_id)
            .cloned();
        state.remotes.entry(remote_id).or_insert(CloudRemoteState {
            cloud_storage_id,
            credentials,
        });
    }

    pub fn remove_remote(&self, remote_id: &RemoteUuid) {
        let mut state = self.state.lock();
        let removed = state.remotes.remove(remote_id);
        let Some(removed) = removed else { return };
        Self::forget_storage_id_locked(&mut state, &removed.cloud_storage_id, &self.cache);
    }

    /// Removes cached credentials for a deleted Cloud storage destination, even
    /// if that destination has not been materialized in this runtime yet.
    pub fn forget_storage_id(&self, cloud_storage_id: &str) {
        let mut state = self.state.lock();
        state
            .remotes
            .retain(|_, remote| remote.cloud_storage_id != cloud_storage_id);
        Self::forget_storage_id_locked(&mut state, cloud_storage_id, &self.cache);
    }

    pub fn credentials(&self, remote_id: &RemoteUuid) -> Result<CloudS3Credentials, CloudError> {
        self.state
            .lock()
            .remotes
            .get(remote_id)
            .and_then(|remote| remote.credentials.clone())
            .ok_or(CloudError::CredentialsUnavailable)
    }

    pub fn needs_refresh(&self, remote_id: &RemoteUuid, now: DateTime<Utc>) -> bool {
        self.state
            .lock()
            .remotes
            .get(remote_id)
            .and_then(|remote| remote.credentials.as_ref())
            .is_none_or(|credentials| credentials.expires_at <= now + REFRESH_SAFETY_MARGIN)
    }

    /// Lazily resolves fresh credentials. Calling this concurrently from many
    /// object transfers still results in one API round trip.
    pub async fn refresh_if_needed(&self, remote_id: &RemoteUuid) -> Result<(), CloudError> {
        if !self.needs_refresh(remote_id, Utc::now()) {
            return Ok(());
        }
        let _refresh_guard = self.refresh_lock.lock().await;
        if !self.needs_refresh(remote_id, Utc::now()) {
            return Ok(());
        }
        self.refresh_all_unlocked().await
    }

    /// Re-fetches every Cloud remote and atomically replaces the resolved S3
    /// state for locally registered remote ids.
    pub async fn refresh_now(&self) -> Result<(), CloudError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        self.refresh_all_unlocked().await
    }

    async fn refresh_all_unlocked(&self) -> Result<(), CloudError> {
        let auth = self
            .state
            .lock()
            .auth
            .clone()
            .ok_or(CloudError::SessionUnavailable)?;
        let remotes = match auth.list_remotes().await {
            Ok(remotes) => remotes,
            Err(error @ LascoCloudAuthError::LoginRequired) => {
                self.clear_persisted_credentials()?;
                return Err(CloudError::Auth(error));
            }
            Err(error) => return Err(CloudError::Auth(error)),
        };
        let credential_response = match auth.storage_credentials().await {
            Ok(credentials) => credentials,
            Err(error @ LascoCloudAuthError::LoginRequired) => {
                self.clear_persisted_credentials()?;
                return Err(CloudError::Auth(error));
            }
            Err(error) => return Err(CloudError::Auth(error)),
        };
        let infos = remotes
            .into_iter()
            .map(|info| (info.id.clone(), info))
            .collect::<HashMap<_, _>>();
        let credentials = credential_response
            .into_iter()
            .map(|credential| (credential.id.clone(), credential))
            .collect::<HashMap<_, _>>();

        let mut resolved = HashMap::new();
        for cloud_storage_id in self
            .state
            .lock()
            .remotes
            .values()
            .map(|remote| remote.cloud_storage_id.clone())
        {
            let info = infos
                .get(&cloud_storage_id)
                .ok_or_else(|| CloudError::MissingRemote(cloud_storage_id.clone()))?;
            let credential = credentials
                .get(&cloud_storage_id)
                .ok_or_else(|| CloudError::MissingCredentials(cloud_storage_id.clone()))?;
            resolved.insert(
                cloud_storage_id,
                CloudS3Credentials {
                    endpoint: info.endpoint.clone(),
                    bucket: info.bucket.clone(),
                    region: info.region.clone(),
                    path_prefix: info.path_prefix.clone(),
                    access_key_id: credential.access_key_id.clone(),
                    secret_access_key: credential.secret_access_key.clone(),
                    session_token: credential.session_token.clone(),
                    expires_at: DateTime::parse_from_rfc3339(&credential.expires_at)
                        .map_err(|_| CloudError::InvalidExpiry(credential.expires_at.clone()))?
                        .with_timezone(&Utc),
                },
            );
        }
        let mut state = self.state.lock();
        for (storage_id, credentials) in &resolved {
            state
                .credentials_by_storage_id
                .insert(storage_id.clone(), credentials.clone());
        }
        for remote in state.remotes.values_mut() {
            remote.credentials = resolved.get(&remote.cloud_storage_id).cloned();
        }
        self.persist_locked(&state)?;
        Ok(())
    }

    fn persist_locked(&self, state: &CloudRuntimeState) -> Result<(), CloudError> {
        let cached = state
            .credentials_by_storage_id
            .iter()
            .map(|(id, credential)| (id.clone(), to_cached(credential)))
            .collect();
        self.cache
            .save(&cached)
            .map_err(|error| CloudError::Cache(error.to_string()))
    }

    fn forget_storage_id_locked(
        state: &mut CloudRuntimeState,
        cloud_storage_id: &str,
        cache: &CloudRuntimeCache,
    ) {
        if state
            .remotes
            .values()
            .any(|remote| remote.cloud_storage_id == cloud_storage_id)
        {
            return;
        }
        state.credentials_by_storage_id.remove(cloud_storage_id);
        let cached = state
            .credentials_by_storage_id
            .iter()
            .map(|(id, credentials)| (id.clone(), to_cached(credentials)))
            .collect();
        if let Err(error) = cache.save(&cached) {
            tracing::warn!("failed to remove Lasco Cloud credentials from cache: {error}");
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("Lasco Cloud session is required")]
    SessionUnavailable,
    #[error("Lasco Cloud session has expired; authenticate again")]
    SessionExpired,
    #[error(transparent)]
    Auth(#[from] LascoCloudAuthError),
    #[error("Lasco Cloud session details are invalid")]
    InvalidSession,
    #[error("Lasco Cloud credentials are not available")]
    CredentialsUnavailable,
    #[error("Lasco Cloud did not return remote '{0}'")]
    MissingRemote(String),
    #[error("Lasco Cloud did not return credentials for remote '{0}'")]
    MissingCredentials(String),
    #[error("Lasco Cloud returned invalid credential expiry '{0}'")]
    InvalidExpiry(String),
    #[error("Lasco Cloud request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("Lasco Cloud credential cache failed: {0}")]
    Cache(String),
}

fn to_cached(credentials: &CloudS3Credentials) -> CachedCloudS3Credentials {
    CachedCloudS3Credentials {
        endpoint: credentials.endpoint.clone(),
        bucket: credentials.bucket.clone(),
        region: credentials.region.clone(),
        path_prefix: credentials.path_prefix.clone(),
        access_key_id: credentials.access_key_id.clone(),
        secret_access_key: credentials.secret_access_key.clone(),
        session_token: credentials.session_token.clone(),
        expires_at: credentials.expires_at,
    }
}

fn from_cached(credentials: CachedCloudS3Credentials) -> CloudS3Credentials {
    CloudS3Credentials {
        endpoint: credentials.endpoint,
        bucket: credentials.bucket,
        region: credentials.region,
        path_prefix: credentials.path_prefix,
        access_key_id: credentials.access_key_id,
        secret_access_key: credentials.secret_access_key,
        session_token: credentials.session_token,
        expires_at: credentials.expires_at,
    }
}

/// Shared handle stored by `LibraryInner` and cloned into Cloud storage wrappers.
pub type SharedCloudRuntime = Arc<CloudRuntime>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::master_key::generate_master_key;

    #[test]
    fn refresh_threshold_is_one_minute() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = CloudRuntime::open(
            directory.path().join("cloud-runtime.enc"),
            LibraryId::new(),
            &generate_master_key(),
        );
        let remote_id = RemoteUuid::new();
        let now = Utc::now();
        runtime.state.lock().credentials_by_storage_id.insert(
            "cloud-1".to_string(),
            CloudS3Credentials {
                endpoint: "https://s3.example".to_string(),
                bucket: "bucket".to_string(),
                region: "region".to_string(),
                path_prefix: String::new(),
                access_key_id: "key".to_string(),
                secret_access_key: "secret".to_string(),
                session_token: None,
                expires_at: now + Duration::seconds(61),
            },
        );
        runtime.register_remote(remote_id, "cloud-1".to_string());
        assert!(!runtime.needs_refresh(&remote_id, now));
        assert!(runtime.needs_refresh(&remote_id, now + Duration::seconds(1)));
    }

    #[test]
    fn cached_credentials_are_rehydrated_after_reopening() {
        let directory = tempfile::tempdir().unwrap();
        let library_id = LibraryId::new();
        let master_key = generate_master_key();
        let path = directory.path().join("cloud-runtime.enc");
        let remote_id = RemoteUuid::new();
        let runtime = CloudRuntime::open(path.clone(), library_id, &master_key);
        runtime.state.lock().credentials_by_storage_id.insert(
            "cloud-1".to_string(),
            CloudS3Credentials {
                endpoint: "https://s3.example".to_string(),
                bucket: "bucket".to_string(),
                region: "region".to_string(),
                path_prefix: String::new(),
                access_key_id: "key".to_string(),
                secret_access_key: "secret".to_string(),
                session_token: None,
                expires_at: Utc::now() + Duration::hours(1),
            },
        );
        runtime.persist_locked(&runtime.state.lock()).unwrap();

        let reopened = CloudRuntime::open(path, library_id, &master_key);
        reopened.register_remote(remote_id, "cloud-1".to_string());
        assert_eq!(
            reopened.credentials(&remote_id).unwrap().secret_access_key,
            "secret"
        );
    }
}
