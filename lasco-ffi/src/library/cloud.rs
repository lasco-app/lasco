use super::{FfiLascoCloudAccount, FfiLascoCloudRemote, FfiLascoCloudSubscription, FfiLibrary};
use crate::error::LascoError;

#[uniffi::export]
impl FfiLibrary {
    pub async fn lasco_cloud_storage_usage(&self) -> Result<u64, LascoError> {
        let auth = self
            .inner
            .cloud_runtime()
            .lasco_cloud_auth()
            .ok_or_else(|| LascoError::Other {
                msg: "Lasco Cloud is not configured".to_string(),
            })?;
        self.rt
            .spawn(async move { auth.storage_usage().await })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map(|usage| usage.approximate_used_bytes)
            .map_err(|e| LascoError::Other { msg: e.to_string() })
    }
    /// Rejects an initial import that cannot fit on the configured Cloud remotes.
    /// Only media data bytes are supplied; state and generated thumbnails are excluded.
    pub async fn lasco_cloud_check_initial_import(
        &self,
        media_bytes: u64,
    ) -> Result<(), LascoError> {
        let config = self.load_library_json()?;
        let cloud_remotes: Vec<_> = config
            .remotes
            .iter()
            .filter_map(|remote| match &remote.kind {
                lasco_core::library_json::RemoteKind::CloudS3(cloud) => {
                    self.inner
                        .cloud_runtime()
                        .register_remote(remote.remote_uuid, cloud.cloud_storage_id.clone());
                    Some(remote.remote_uuid)
                }
                _ => None,
            })
            .collect();
        if cloud_remotes.is_empty() {
            return Ok(());
        }
        let runtime = self.inner.cloud_runtime();
        let checked = self
            .rt
            .spawn(async move {
                runtime
                    .check_storage_usage_for_remotes(&cloud_remotes, media_bytes)
                    .await
            })
            .await
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .map_err(|e| LascoError::CloudQuotaExceeded { msg: e.to_string() })?;
        if checked.allowed {
            Ok(())
        } else {
            Err(LascoError::CloudQuotaExceeded {
                msg: format!(
                    "{} bytes requested with {} of {} bytes already indicated",
                    checked.proposed_media_bytes,
                    checked.approximate_used_bytes,
                    checked.storage_quota_bytes
                ),
            })
        }
    }
    pub async fn configure_lasco_cloud_auth(&self, base_url: String) -> Result<(), LascoError> {
        let inner = self.inner.clone();
        self.rt
            .spawn(async move { inner.configure_lasco_cloud_auth(base_url).await })
            .await
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
    }

    pub async fn lasco_cloud_login(
        &self,
        email: String,
        password: String,
        platform: String,
        app_version: String,
    ) -> Result<(), LascoError> {
        let runtime = self.inner.cloud_runtime();
        let auth = runtime
            .lasco_cloud_auth()
            .ok_or_else(|| LascoError::Other {
                msg: "Lasco Cloud is not configured".to_string(),
            })?;
        self.rt
            .spawn(async move { auth.login(email, password, platform, app_version).await })
            .await
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
    }

    pub async fn lasco_cloud_list_remotes(&self) -> Result<Vec<FfiLascoCloudRemote>, LascoError> {
        let auth = self
            .inner
            .cloud_runtime()
            .lasco_cloud_auth()
            .ok_or_else(|| LascoError::Other {
                msg: "Lasco Cloud is not configured".to_string(),
            })?;
        self.rt
            .spawn(async move { auth.list_remotes().await })
            .await
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
            .map(|remotes| remotes.into_iter().map(FfiLascoCloudRemote::from).collect())
    }

    pub async fn lasco_cloud_assign_remotes_to_this_library(
        &self,
        remote_ids: Vec<String>,
    ) -> Result<(), LascoError> {
        let auth = self
            .inner
            .cloud_runtime()
            .lasco_cloud_auth()
            .ok_or_else(|| LascoError::Other {
                msg: "Lasco Cloud is not configured".to_string(),
            })?;
        let library_id = self.inner.library_id();
        self.rt
            .spawn(async move {
                for remote_id in remote_ids {
                    auth.assign_remote_to_library(&remote_id, library_id)
                        .await?;
                }
                Ok::<(), lasco_core::library::lasco_cloud_auth::LascoCloudAuthError>(())
            })
            .await
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
    }

    pub async fn lasco_cloud_subscription(&self) -> Result<FfiLascoCloudAccount, LascoError> {
        let auth = self
            .inner
            .cloud_runtime()
            .lasco_cloud_auth()
            .ok_or_else(|| LascoError::Other {
                msg: "Lasco Cloud is not configured".to_string(),
            })?;
        self.rt
            .spawn(async move { auth.subscription().await })
            .await
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
            .map(FfiLascoCloudAccount::from)
    }

    pub async fn lasco_cloud_revoke_session(&self) -> Result<(), LascoError> {
        let runtime = self.inner.cloud_runtime();
        let auth = runtime
            .lasco_cloud_auth()
            .ok_or_else(|| LascoError::Other {
                msg: "Lasco Cloud is not configured".to_string(),
            })?;
        self.rt
            .spawn(async move { auth.revoke().await })
            .await
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?;
        runtime
            .clear_persisted_credentials()
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
    }

    pub async fn clear_lasco_cloud_auth_and_credentials(&self) -> Result<(), LascoError> {
        let inner = self.inner.clone();
        self.rt
            .spawn(async move { inner.clear_lasco_cloud_auth_and_credentials().await })
            .await
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })?
            .map_err(|error| LascoError::Other {
                msg: error.to_string(),
            })
    }

    pub fn lasco_cloud_is_authenticated(&self) -> bool {
        self.inner
            .cloud_runtime()
            .lasco_cloud_auth()
            .is_some_and(|auth| auth.is_authenticated())
    }
}

impl From<lasco_core::library::lasco_cloud_auth::LascoCloudRemoteInfo> for FfiLascoCloudRemote {
    fn from(value: lasco_core::library::lasco_cloud_auth::LascoCloudRemoteInfo) -> Self {
        Self {
            id: value.id,
            library_id: value.library_id,
            name: value.name,
            endpoint: value.endpoint,
            bucket: value.bucket,
            region: value.region,
            path_prefix: value.path_prefix,
        }
    }
}

impl From<lasco_core::library::lasco_cloud_auth::LascoCloudAccount> for FfiLascoCloudAccount {
    fn from(value: lasco_core::library::lasco_cloud_auth::LascoCloudAccount) -> Self {
        Self {
            email: value.email,
            subscription: value.subscription.map(FfiLascoCloudSubscription::from),
        }
    }
}

impl From<lasco_core::library::lasco_cloud_auth::LascoCloudSubscription>
    for FfiLascoCloudSubscription
{
    fn from(value: lasco_core::library::lasco_cloud_auth::LascoCloudSubscription) -> Self {
        Self {
            plan_id: value.plan_id,
            plan_name: value.plan_name,
            status: value.status,
            storage_quota_bytes: value.storage_quota_bytes,
            renews_at: value.renews_at,
        }
    }
}
