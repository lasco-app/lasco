//! Persistent Lasco Cloud device-session storage.
//!
//! The session is one JSON document in the platform credential store. Keeping
//! the rotated access and refresh tokens in one entry prevents a persisted
//! half-rotation.

use chrono::{DateTime, Utc};
use keyring_core::Entry;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "ios")]
use std::{collections::HashMap, sync::OnceLock};

use crate::identifiers::LibraryId;

pub(crate) const LASCO_CLOUD_KEYRING_SERVICE: &str = "com.lasco.lasco.cloud";

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StoredLascoCloudSession {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub(crate) struct LascoCloudSessionStore {
    account: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LascoCloudSessionStoreError {
    #[error("platform credential store failed: {0}")]
    Keyring(String),
    #[error("stored Lasco Cloud session is invalid: {0}")]
    Invalid(#[from] serde_json::Error),
    #[error("credential-store worker failed: {0}")]
    Worker(String),
}

impl LascoCloudSessionStore {
    pub(crate) fn new(library_id: LibraryId) -> Self {
        Self {
            account: library_id.to_string(),
        }
    }

    pub(crate) async fn load(
        &self,
    ) -> Result<Option<StoredLascoCloudSession>, LascoCloudSessionStoreError> {
        let account = self.account.clone();
        tokio::task::spawn_blocking(move || {
            initialize_platform_credential_store()?;
            let entry = Entry::new(LASCO_CLOUD_KEYRING_SERVICE, &account)
                .map_err(|error| LascoCloudSessionStoreError::Keyring(error.to_string()))?;
            match entry.get_secret() {
                Ok(bytes) => match serde_json::from_slice(&bytes) {
                    Ok(session) => Ok(Some(session)),
                    // The previous iOS client stored a bare access token in
                    // this service/account slot. It cannot be refreshed, so
                    // migrate it to a logged-out state rather than failing to
                    // configure the Rust auth manager.
                    Err(_) => {
                        entry.delete_credential().map_err(|error| {
                            LascoCloudSessionStoreError::Keyring(error.to_string())
                        })?;
                        Ok(None)
                    }
                },
                Err(keyring_core::Error::NoEntry) => Ok(None),
                Err(error) => Err(LascoCloudSessionStoreError::Keyring(error.to_string())),
            }
        })
        .await
        .map_err(|error| LascoCloudSessionStoreError::Worker(error.to_string()))?
    }

    pub(crate) async fn replace(
        &self,
        session: &StoredLascoCloudSession,
    ) -> Result<(), LascoCloudSessionStoreError> {
        let account = self.account.clone();
        let bytes = serde_json::to_vec(session)?;
        tokio::task::spawn_blocking(move || {
            initialize_platform_credential_store()?;
            let entry = Entry::new(LASCO_CLOUD_KEYRING_SERVICE, &account)
                .map_err(|error| LascoCloudSessionStoreError::Keyring(error.to_string()))?;
            entry
                .set_secret(&bytes)
                .map_err(|error| LascoCloudSessionStoreError::Keyring(error.to_string()))
        })
        .await
        .map_err(|error| LascoCloudSessionStoreError::Worker(error.to_string()))?
    }

    pub(crate) async fn clear(&self) -> Result<(), LascoCloudSessionStoreError> {
        let account = self.account.clone();
        tokio::task::spawn_blocking(move || {
            initialize_platform_credential_store()?;
            let entry = Entry::new(LASCO_CLOUD_KEYRING_SERVICE, &account)
                .map_err(|error| LascoCloudSessionStoreError::Keyring(error.to_string()))?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
                Err(error) => Err(LascoCloudSessionStoreError::Keyring(error.to_string())),
            }
        })
        .await
        .map_err(|error| LascoCloudSessionStoreError::Worker(error.to_string()))?
    }
}

/// Keyring's `Entry` convenience API initializes a default store on macOS,
/// but deliberately leaves iOS to applications because iOS must use Protected
/// Data. The default store is process-global, so set it exactly once before
/// any entry is created.
#[cfg(target_os = "ios")]
fn initialize_platform_credential_store() -> Result<(), LascoCloudSessionStoreError> {
    static INITIALIZATION: OnceLock<Result<(), String>> = OnceLock::new();
    INITIALIZATION
        .get_or_init(|| {
            apple_native_keyring_store::protected::Store::new_with_configuration(&HashMap::new())
                .map(|store| {
                    let store: std::sync::Arc<keyring_core::CredentialStore> = store;
                    keyring_core::set_default_store(store);
                })
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map(|_| ())
        .map_err(|error| LascoCloudSessionStoreError::Keyring(error.clone()))
}

#[cfg(not(target_os = "ios"))]
fn initialize_platform_credential_store() -> Result<(), LascoCloudSessionStoreError> {
    Ok(())
}
