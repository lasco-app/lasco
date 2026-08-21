//! Runtime-only Lasco Cloud connection state.
//!
//! The local library configuration persists only a stable Cloud storage id. This
//! module owns the disposable API session and resolved S3 credentials needed to
//! use that id. Neither is written to disk.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::Mutex as AsyncMutex;

use crate::identifiers::RemoteUuid;

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
    session: Option<CloudSession>,
    remotes: HashMap<RemoteUuid, CloudRemoteState>,
}

#[derive(Clone)]
struct CloudSession {
    base_url: String,
    bearer_token: String,
}

/// Shared Cloud state for one open library.
///
/// `refresh_lock` deliberately covers all Cloud remotes: the service returns
/// credentials as one set, so concurrent transfers coalesce into one refresh.
pub struct CloudRuntime {
    state: Mutex<CloudRuntimeState>,
    refresh_lock: AsyncMutex<()>,
    client: reqwest::Client,
}

impl std::fmt::Debug for CloudRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudRuntime").finish_non_exhaustive()
    }
}

impl Default for CloudRuntime {
    fn default() -> Self {
        Self {
            state: Mutex::new(CloudRuntimeState::default()),
            refresh_lock: AsyncMutex::new(()),
            client: reqwest::Client::new(),
        }
    }
}

impl CloudRuntime {
    pub fn set_session(&self, base_url: String, bearer_token: String) -> Result<(), CloudError> {
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() || bearer_token.trim().is_empty() {
            return Err(CloudError::InvalidSession);
        }
        self.state.lock().session = Some(CloudSession {
            base_url,
            bearer_token,
        });
        Ok(())
    }

    pub fn clear_session(&self) {
        self.state.lock().session = None;
    }

    pub fn clear_credentials(&self) {
        for remote in self.state.lock().remotes.values_mut() {
            remote.credentials = None;
        }
    }

    pub fn install_remote(
        &self,
        remote_id: RemoteUuid,
        cloud_storage_id: String,
        credentials: CloudS3Credentials,
    ) {
        self.state.lock().remotes.insert(
            remote_id,
            CloudRemoteState {
                cloud_storage_id,
                credentials: Some(credentials),
            },
        );
    }

    pub fn register_remote(&self, remote_id: RemoteUuid, cloud_storage_id: String) {
        self.state
            .lock()
            .remotes
            .entry(remote_id)
            .or_insert(CloudRemoteState {
                cloud_storage_id,
                credentials: None,
            });
    }

    pub fn remove_remote(&self, remote_id: &RemoteUuid) {
        self.state.lock().remotes.remove(remote_id);
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
        let session = self
            .state
            .lock()
            .session
            .clone()
            .ok_or(CloudError::SessionUnavailable)?;
        let remotes: CloudRemotesResponse = self
            .request_json(&session, reqwest::Method::GET, "api/v1/remotes")
            .await?;
        let credential_response: CloudCredentialsResponse = self
            .request_json(
                &session,
                reqwest::Method::POST,
                "api/v1/storage-credentials",
            )
            .await?;
        let infos = remotes
            .remotes
            .into_iter()
            .map(|info| (info.id.clone(), info))
            .collect::<HashMap<_, _>>();
        let credentials = credential_response
            .credentials
            .into_iter()
            .map(|credential| (credential.id.clone(), credential))
            .collect::<HashMap<_, _>>();

        let mut state = self.state.lock();
        for remote in state.remotes.values_mut() {
            let info = infos
                .get(&remote.cloud_storage_id)
                .ok_or_else(|| CloudError::MissingRemote(remote.cloud_storage_id.clone()))?;
            let credential = credentials
                .get(&remote.cloud_storage_id)
                .ok_or_else(|| CloudError::MissingCredentials(remote.cloud_storage_id.clone()))?;
            remote.credentials = Some(CloudS3Credentials {
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
            });
        }
        Ok(())
    }

    async fn request_json<T: for<'de> Deserialize<'de>>(
        &self,
        session: &CloudSession,
        method: reqwest::Method,
        path: &str,
    ) -> Result<T, CloudError> {
        let url = format!("{}/{}", session.base_url, path);
        let response = self
            .client
            .request(method, url)
            .bearer_auth(&session.bearer_token)
            .send()
            .await
            .map_err(CloudError::Request)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CloudError::SessionExpired);
        }
        let response = response.error_for_status().map_err(CloudError::Request)?;
        response.json().await.map_err(CloudError::Request)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("Lasco Cloud session is required")]
    SessionUnavailable,
    #[error("Lasco Cloud session has expired; authenticate again")]
    SessionExpired,
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
}

#[derive(Deserialize)]
struct CloudRemotesResponse {
    remotes: Vec<CloudRemoteInfo>,
}

#[derive(Deserialize)]
struct CloudRemoteInfo {
    id: String,
    endpoint: String,
    bucket: String,
    region: String,
    path_prefix: String,
}

#[derive(Deserialize)]
struct CloudCredentialsResponse {
    credentials: Vec<CloudCredentials>,
}

#[derive(Deserialize)]
struct CloudCredentials {
    id: String,
    access_key_id: String,
    secret_access_key: String,
    #[serde(default)]
    session_token: Option<String>,
    expires_at: String,
}

/// Shared handle stored by `LibraryInner` and cloned into Cloud storage wrappers.
pub type SharedCloudRuntime = Arc<CloudRuntime>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_threshold_is_one_minute() {
        let runtime = CloudRuntime::default();
        let remote_id = RemoteUuid::new();
        let now = Utc::now();
        runtime.install_remote(
            remote_id,
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
        assert!(!runtime.needs_refresh(&remote_id, now));
        assert!(runtime.needs_refresh(&remote_id, now + Duration::seconds(1)));
    }
}
