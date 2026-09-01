//! Rust-owned Lasco Cloud device-session protocol.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;

use crate::identifiers::LibraryId;

use super::lasco_cloud_auth_store::{
    LascoCloudSessionStore, LascoCloudSessionStoreError, StoredLascoCloudSession,
};

const ACCESS_TOKEN_SAFETY_MARGIN: Duration = Duration::minutes(2);

pub struct LascoCloudAuthManager {
    base_url: String,
    client: reqwest::Client,
    store: LascoCloudSessionStore,
    state: RwLock<LascoCloudAuthState>,
    refresh_gate: Mutex<()>,
}

#[derive(Default)]
struct LascoCloudAuthState {
    session: Option<StoredLascoCloudSession>,
    generation: u64,
}

#[derive(Clone, Deserialize)]
pub struct LascoCloudRemoteInfo {
    pub id: String,
    pub library_id: Option<String>,
    pub name: String,
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub path_prefix: String,
}

#[derive(Clone, Deserialize)]
pub struct LascoCloudStorageCredentials {
    pub id: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default)]
    pub session_token: Option<String>,
    pub expires_at: String,
}

#[derive(Clone, Deserialize)]
pub struct LascoCloudAccount {
    pub email: String,
    pub subscription: Option<LascoCloudSubscription>,
}

#[derive(Clone, Deserialize)]
pub struct LascoCloudSubscription {
    pub plan_id: String,
    pub plan_name: String,
    pub status: String,
    pub storage_quota_bytes: u64,
    pub renews_at: String,
}

#[derive(Clone, Deserialize)]
pub struct LascoCloudStorageUsageCheck {
    pub allowed: bool,
    pub storage_quota_bytes: u64,
    pub approximate_used_bytes: u64,
    pub proposed_media_bytes: u64,
}
#[derive(Clone, Deserialize)]
pub struct LascoCloudStorageUsage { pub approximate_used_bytes: u64 }

#[derive(Debug, thiserror::Error)]
pub enum LascoCloudAuthError {
    #[error("Lasco Cloud login is required")]
    LoginRequired,
    #[error("Lasco Cloud is unavailable: {0}")]
    Offline(#[source] reqwest::Error),
    #[error("Lasco Cloud request failed (HTTP {status_code}): {detail}")]
    RequestFailed { status_code: u16, detail: String },
    #[error("Lasco Cloud returned an invalid response")]
    InvalidResponse,
    #[error(transparent)]
    SessionStore(#[from] LascoCloudSessionStoreError),
}

impl LascoCloudAuthManager {
    pub fn new(library_id: LibraryId, base_url: String) -> Arc<Self> {
        Arc::new(Self {
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            store: LascoCloudSessionStore::new(library_id),
            state: RwLock::new(LascoCloudAuthState::default()),
            refresh_gate: Mutex::new(()),
        })
    }

    pub async fn restore(&self) -> Result<(), LascoCloudAuthError> {
        let session = self.store.load().await?;
        let mut state = self.state.write();
        state.session = session;
        state.generation = state.generation.wrapping_add(1);
        Ok(())
    }

    pub fn is_authenticated(&self) -> bool {
        self.state.read().session.is_some()
    }

    pub async fn login(
        &self,
        email: String,
        password: String,
        platform: String,
        app_version: String,
    ) -> Result<(), LascoCloudAuthError> {
        #[derive(Serialize)]
        struct Request {
            email: String,
            password: String,
            platform: String,
            app_version: String,
        }
        let session: LascoCloudLoginResponse = self
            .request_json_raw(
                Method::POST,
                "api/v1/sessions",
                None,
                Some(Request {
                    email,
                    password,
                    platform,
                    app_version,
                }),
            )
            .await?;
        self.install_session(session.into()).await
    }

    pub async fn list_remotes(&self) -> Result<Vec<LascoCloudRemoteInfo>, LascoCloudAuthError> {
        #[derive(Deserialize)]
        struct Response {
            remotes: Vec<LascoCloudRemoteInfo>,
        }
        let response: Response = self
            .authenticated_json(Method::GET, "api/v1/remotes", None::<()>)
            .await?;
        Ok(response.remotes)
    }

    pub async fn assign_remote_to_library(
        &self,
        remote_id: &str,
        library_id: LibraryId,
    ) -> Result<(), LascoCloudAuthError> {
        #[derive(Serialize)]
        struct Request {
            library_id: String,
        }
        let _: () = self
            .authenticated_json(
                Method::PUT,
                &format!("api/v1/remotes/{remote_id}/library-id"),
                Some(Request {
                    library_id: library_id.to_string(),
                }),
            )
            .await?;
        Ok(())
    }

    pub async fn subscription(&self) -> Result<LascoCloudAccount, LascoCloudAuthError> {
        self.authenticated_json(Method::GET, "api/v1/subscription", None::<()>)
            .await
    }

    pub async fn storage_credentials(
        &self,
    ) -> Result<Vec<LascoCloudStorageCredentials>, LascoCloudAuthError> {
        #[derive(Deserialize)]
        struct Response {
            credentials: Vec<LascoCloudStorageCredentials>,
        }
        let response: Response = self
            .authenticated_json(Method::POST, "api/v1/storage-credentials", None::<()>)
            .await?;
        Ok(response.credentials)
    }

    pub async fn check_storage_usage(
        &self,
        remotes: Vec<(String, u64)>,
    ) -> Result<LascoCloudStorageUsageCheck, LascoCloudAuthError> {
        #[derive(Serialize)]
        struct Remote { remote_id: String, media_bytes: u64 }
        #[derive(Serialize)]
        struct Request { remotes: Vec<Remote> }
        self.authenticated_json(Method::POST, "api/v1/storage-usage/check", Some(Request {
            remotes: remotes.into_iter().map(|(remote_id, media_bytes)| Remote { remote_id, media_bytes }).collect(),
        })).await
    }
    pub async fn storage_usage(&self) -> Result<LascoCloudStorageUsage, LascoCloudAuthError> {
        self.authenticated_json(Method::GET, "api/v1/storage-usage", None::<()>).await
    }

    pub async fn confirm_storage_usage(
        &self, remote_id: String, media_bytes_added: u64,
    ) -> Result<(), LascoCloudAuthError> {
        #[derive(Serialize)]
        struct Request { remote_id: String, media_bytes_added: u64 }
        self.authenticated_json(Method::POST, "api/v1/storage-usage/confirm", Some(Request { remote_id, media_bytes_added })).await
    }

    pub async fn revoke(&self) -> Result<(), LascoCloudAuthError> {
        let _: () = self
            .authenticated_json(Method::POST, "api/v1/sessions/revoke", None::<()>)
            .await?;
        self.clear_local_session().await
    }

    pub async fn clear_local_session(&self) -> Result<(), LascoCloudAuthError> {
        {
            let mut state = self.state.write();
            state.session = None;
            state.generation = state.generation.wrapping_add(1);
        }
        self.store.clear().await?;
        Ok(())
    }

    async fn authenticated_json<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<B>,
    ) -> Result<T, LascoCloudAuthError> {
        self.ensure_fresh_access_token().await?;
        let (token, generation) = self.current_access_token()?;
        let body = body
            .map(|value| serde_json::to_vec(&value))
            .transpose()
            .map_err(|_| LascoCloudAuthError::InvalidResponse)?;
        match self
            .request_json_bytes(method.clone(), path, Some(&token), body.as_deref())
            .await
        {
            Ok(value) => Ok(value),
            Err(LascoCloudAuthError::RequestFailed {
                status_code: 401, ..
            }) => {
                self.refresh_after_unauthorized(generation).await?;
                let (token, _) = self.current_access_token()?;
                self.request_json_bytes(method, path, Some(&token), body.as_deref())
                    .await
            }
            Err(error) => Err(error),
        }
    }

    async fn ensure_fresh_access_token(&self) -> Result<(), LascoCloudAuthError> {
        let should_refresh = self
            .state
            .read()
            .session
            .as_ref()
            .map(|session| session.expires_at <= Utc::now() + ACCESS_TOKEN_SAFETY_MARGIN)
            .ok_or(LascoCloudAuthError::LoginRequired)?;
        if should_refresh {
            let generation = self.state.read().generation;
            self.refresh_after_unauthorized(generation).await?;
        }
        Ok(())
    }

    async fn refresh_after_unauthorized(
        &self,
        failed_generation: u64,
    ) -> Result<(), LascoCloudAuthError> {
        let _guard = self.refresh_gate.lock().await;
        if self.state.read().generation != failed_generation {
            return Ok(());
        }
        let refresh_token = self
            .state
            .read()
            .session
            .as_ref()
            .map(|session| session.refresh_token.clone())
            .ok_or(LascoCloudAuthError::LoginRequired)?;
        let response: LascoCloudLoginResponse = match self
            .request_json_raw::<LascoCloudLoginResponse, ()>(
                Method::POST,
                "api/v1/sessions/refresh",
                Some(&refresh_token),
                None,
            )
            .await
        {
            Ok(response) => response,
            Err(LascoCloudAuthError::RequestFailed {
                status_code: 401, ..
            }) => {
                self.clear_local_session().await?;
                return Err(LascoCloudAuthError::LoginRequired);
            }
            Err(error) => return Err(error),
        };
        if let Err(error) = self.install_session(response.into()).await {
            let _ = self.clear_local_session().await;
            return Err(error);
        }
        Ok(())
    }

    fn current_access_token(&self) -> Result<(String, u64), LascoCloudAuthError> {
        let state = self.state.read();
        let token = state
            .session
            .as_ref()
            .map(|session| session.access_token.clone())
            .ok_or(LascoCloudAuthError::LoginRequired)?;
        Ok((token, state.generation))
    }

    async fn install_session(
        &self,
        session: StoredLascoCloudSession,
    ) -> Result<(), LascoCloudAuthError> {
        self.store.replace(&session).await?;
        let mut state = self.state.write();
        state.session = Some(session);
        state.generation = state.generation.wrapping_add(1);
        Ok(())
    }

    async fn request_json_raw<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<B>,
    ) -> Result<T, LascoCloudAuthError> {
        let body = body
            .map(|value| serde_json::to_vec(&value))
            .transpose()
            .map_err(|_| LascoCloudAuthError::InvalidResponse)?;
        self.request_json_bytes(method, path, bearer, body.as_deref())
            .await
    }

    async fn request_json_bytes<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&[u8]>,
    ) -> Result<T, LascoCloudAuthError> {
        let mut request = self
            .client
            .request(method, format!("{}/{}", self.base_url, path));
        request = request.header(reqwest::header::ACCEPT, "application/json");
        if let Some(bearer) = bearer {
            request = request.bearer_auth(bearer);
        }
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_vec());
        }
        let response = request.send().await.map_err(LascoCloudAuthError::Offline)?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(LascoCloudAuthError::RequestFailed {
                status_code: 401,
                detail: "unauthorized".to_string(),
            });
        }
        if !response.status().is_success() {
            let status_code = response.status().as_u16();
            let detail = response
                .text()
                .await
                .ok()
                .and_then(|body| {
                    serde_json::from_str::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|json| json["error"].as_str().map(ToOwned::to_owned))
                })
                .unwrap_or_else(|| "request failed".to_string());
            return Err(LascoCloudAuthError::RequestFailed {
                status_code,
                detail,
            });
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| LascoCloudAuthError::InvalidResponse)?;
        decode_json_response(&body)
    }
}

fn decode_json_response<T: DeserializeOwned>(body: &[u8]) -> Result<T, LascoCloudAuthError> {
    if body.is_empty() {
        // Axum's successful mutation endpoints deliberately return 204.
        // Deserialize `null` so `()` callers accept the empty body while
        // structured response types still reject it as invalid.
        serde_json::from_str("null").map_err(|_| LascoCloudAuthError::InvalidResponse)
    } else {
        serde_json::from_slice(body).map_err(|_| LascoCloudAuthError::InvalidResponse)
    }
}

#[derive(Deserialize)]
struct LascoCloudLoginResponse {
    token: String,
    refresh_token: String,
    expires_at: DateTime<Utc>,
}

impl From<LascoCloudLoginResponse> for StoredLascoCloudSession {
    fn from(response: LascoCloudLoginResponse) -> Self {
        Self {
            access_token: response.token,
            refresh_token: response.refresh_token,
            expires_at: response.expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LascoCloudAuthError, decode_json_response};

    #[test]
    fn accepts_an_empty_success_response_for_unit() {
        assert!(decode_json_response::<()>(b"").is_ok());
    }

    #[test]
    fn rejects_an_empty_success_response_for_structured_data() {
        let result = decode_json_response::<serde_json::Value>(b"");
        assert!(matches!(result, Ok(serde_json::Value::Null)));

        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Response {
            value: String,
        }
        let result = decode_json_response::<Response>(b"");
        assert!(matches!(result, Err(LascoCloudAuthError::InvalidResponse)));
    }
}
