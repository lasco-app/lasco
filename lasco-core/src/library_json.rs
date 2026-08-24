use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::config_json::{ConfigJson, LibraryNickname, library_data_dir};
use crate::crdt::DeviceId;
use crate::identifiers::{LibraryId, RemoteUuid};
use crate::operations::LibraryUsername;

// Remote storage configuration types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub remote_uuid: RemoteUuid,
    pub name: String,
    #[serde(default)]
    pub auto_push: bool,
    /// Lower values are tried first when media is fetched on demand.
    pub media_fetch_priority: u32,
    /// Excludes this remote from on-demand media retrieval while retaining it for other operations.
    #[serde(default)]
    pub exclude_from_media_fetch: bool,
    pub kind: RemoteKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteKind {
    S3(S3Config),
    /// Lasco-managed S3 storage. Connection credentials are deliberately not
    /// persisted: the native client obtains a short-lived session for this
    /// stable storage identity and injects it into the FFI at runtime.
    #[serde(rename = "lasco_cloud_s3", alias = "cloud_s3")]
    CloudS3(CloudS3Config),
    FixedPath(FixedPathConfig),
    UsbAndroid(UsbAndroidConfig),
    UsbApple(UsbAppleConfig),
    DebugLocalApple(DebugLocalAppleConfig),
    DebugLocalAndroid(DebugLocalAndroidConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    /// Key prefix for the library storage. Lets several libraries share one bucket.
    pub path_prefix: Option<String>,
    pub access_key: String,
    /// Secret key encrypted with the library master key (AES-256-GCM), base64 encoded.
    pub secret_key_encrypted: String,
    /// Human-readable description of the encryption scheme.
    pub secret_key_encryption_description: String,
}

/// Persistent identity for one Lasco Cloud storage destination.
///
/// `cloud_storage_id` is opaque and stable even when the service moves the
/// underlying bucket, prefix, region, or provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudS3Config {
    pub cloud_storage_id: String,
}

/// Trusts a stored absolute path, e.g. a USB or external drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedPathConfig {
    pub root_dir: PathBuf,
}

/// Persistent Storage Access Framework grant for a wired USB folder selected
/// on Android. This is deliberately Android-specific: SAF providers have
/// different read/write guarantees from Apple security-scoped URLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbAndroidConfig {
    pub tree_uri: String,
}

/// Base64-encoded security-scoped bookmark for a wired USB folder selected on
/// Apple platforms. The bookmark is an opaque access capability, not a path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbAppleConfig {
    pub bookmark_base64: String,
}

/// Persists only a name and re-resolves the path against the current app-support
/// directory on every use, so it survives the sandboxed app container's UUID
/// segment changing across relaunches/reinstalls on iOS/macOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugLocalAppleConfig {
    pub local_dir_name: String,
}

/// Stores a name and resolves the path against the app's own data directory on
/// every use. Android's `app_dir` is not sandboxed the same way iOS/macOS is, so
/// unlike `DebugLocalAppleConfig` no separate app-support directory is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugLocalAndroidConfig {
    pub local_dir_name: String,
}

/// Per-library configuration stored at `{app_dir}/libraries/{library_id}/library.json`.
/// It holds the library preferences and the ordered list of remotes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryJson {
    /// User-friendly nickname for the library.
    pub library_nickname: LibraryNickname,
    /// Stable local CRDT author identity. This configuration is never uploaded to remotes.
    pub device_id: DeviceId,
    /// Default username for this library
    #[serde(default)]
    pub default_username: Option<LibraryUsername>,
    /// UUID of the active password file for the default user (`mk_{username}_{uuid}.enc`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_password_uuid: Option<Uuid>,
    /// Default remote for fetching operations
    pub default_fetch_remote: Option<RemoteUuid>,
    /// Whether to automatically import new device media into this library
    #[serde(default)]
    pub auto_import_device_media: bool,
    /// Configured remote storage locations
    pub remotes: Vec<RemoteConfig>,
    /// Ordered subset of remotes allowed to supply uncached originals and relay media.
    #[serde(default)]
    pub media_source_order: Vec<RemoteUuid>,
}

/// Path to a library's `library.json`
#[must_use]
pub fn library_json_path(app_dir: &Path, library_id: &LibraryId) -> PathBuf {
    library_data_dir(app_dir, library_id).join("library.json")
}

impl LibraryJson {
    /// Load a library's configuration from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is invalid JSON.
    pub fn load(app_dir: &Path, library_id: &LibraryId) -> Result<Option<Self>> {
        let path = library_json_path(app_dir, library_id);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path)
            .with_context(|| format!("failed to read library config from {}", path.display()))?;
        let mut config: LibraryJson =
            serde_json::from_slice(&data).context("failed to parse library.json")?;
        // Old configurations did not carry the ordered subset. Preserve their established
        // priority semantics on first load; subsequent saves write only the new field.
        if config.media_source_order.is_empty() {
            let mut legacy: Vec<_> = config
                .remotes
                .iter()
                .enumerate()
                .filter(|(_, remote)| !remote.exclude_from_media_fetch)
                .collect();
            legacy.sort_by_key(|(index, remote)| (remote.media_fetch_priority, *index));
            config.media_source_order = legacy
                .into_iter()
                .map(|(_, remote)| remote.remote_uuid)
                .collect();
        } else {
            let known: std::collections::HashSet<_> =
                config.remotes.iter().map(|r| r.remote_uuid).collect();
            config.media_source_order.retain(|id| known.contains(id));
            let mut seen = std::collections::HashSet::new();
            config.media_source_order.retain(|id| seen.insert(*id));
        }
        Ok(Some(config))
    }

    /// Save a library's configuration to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if its directory cannot be created or its JSON cannot be serialized or written.
    pub fn save(&self, app_dir: &Path, library_id: &LibraryId) -> Result<()> {
        let dir = library_data_dir(app_dir, library_id);
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create library directory {}", dir.display()))?;
        let path = library_json_path(app_dir, library_id);
        let data = serde_json::to_vec_pretty(self).context("failed to serialize library config")?;
        fs::write(&path, &data)
            .with_context(|| format!("failed to write library config to {}", path.display()))?;
        Ok(())
    }
}

/// Owns the mutex that serializes access to one library's on-disk configuration.
pub struct LibraryJsonReadWriteLock {
    mutex: parking_lot::Mutex<()>,
}

impl LibraryJsonReadWriteLock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            mutex: parking_lot::Mutex::new(()),
        }
    }

    pub(crate) fn lock<'a>(
        &'a self,
        app_dir: &'a Path,
        library_id: LibraryId,
    ) -> LibraryJsonReadWrite<'a> {
        LibraryJsonReadWrite {
            app_dir,
            library_id,
            lock_guard: self.mutex.lock(),
        }
    }
}

/// Exclusive access to one library's on-disk configuration.
///
/// This object keeps the library configuration mutex locked from construction through its drop.
/// Do not hold it across an `.await`.
pub struct LibraryJsonReadWrite<'a> {
    app_dir: &'a Path,
    library_id: LibraryId,
    #[allow(
        dead_code,
        reason = "Keeping this RAII guard alive grants this object exclusive access until it is dropped."
    )]
    lock_guard: parking_lot::MutexGuard<'a, ()>,
}

impl<'a> LibraryJsonReadWrite<'a> {
    /// Read the current library configuration while holding exclusive access.
    pub fn read(&self) -> Result<LibraryJson> {
        LibraryJson::load(self.app_dir, &self.library_id)?
            .ok_or_else(|| anyhow::anyhow!("library.json not found"))
    }

    /// Persist a library configuration while holding exclusive access.
    pub fn write(&self, library_json: &LibraryJson) -> Result<()> {
        library_json.save(self.app_dir, &self.library_id)
    }
}

/// Persist a library's configuration and register its ID in the global index.
///
/// # Errors
///
/// Returns an error if either library or global application configuration cannot be read or saved.
pub fn save_library(app_dir: &Path, library_id: &LibraryId, library: &LibraryJson) -> Result<()> {
    library.save(app_dir, library_id)?;
    let mut config = ConfigJson::load(app_dir)?.unwrap_or_default();
    config.add_library(*library_id);
    config.save(app_dir)?;
    Ok(())
}

/// Find a registered library by its nickname.
///
/// # Errors
///
/// Returns an error if the global index or a registered library configuration cannot be read,
/// or if no library has the requested nickname.
pub fn find_library_id_by_nickname(app_dir: &Path, nickname: &str) -> Result<LibraryId> {
    let config =
        ConfigJson::load(app_dir)?.ok_or_else(|| anyhow::anyhow!("no libraries configured"))?;
    for library_id in config.libraries {
        let library = LibraryJson::load(app_dir, &library_id)?
            .ok_or_else(|| anyhow::anyhow!("library.json not found for '{library_id}'"))?;
        if library.library_nickname.0 == nickname {
            return Ok(library_id);
        }
    }
    anyhow::bail!("library '{nickname}' not found")
}

/// Load the configuration for the library with the given nickname.
///
/// # Errors
///
/// Returns an error if configuration is unreadable, no library has that nickname, or its config is missing.
#[allow(
    dead_code,
    reason = "Retained for nickname-based library selection in the CLI."
)]
fn load_library_by_nickname(app_dir: &Path, nickname: &str) -> Result<(LibraryId, LibraryJson)> {
    let library_id = find_library_id_by_nickname(app_dir, nickname)?;
    let library = LibraryJson::load(app_dir, &library_id)?
        .ok_or_else(|| anyhow::anyhow!("library.json not found for '{nickname}'"))?;
    Ok((library_id, library))
}

/// Load the configuration for the default library.
///
/// # Errors
///
/// Returns an error if configuration is unreadable, no default exists, or its config is missing.
#[allow(
    dead_code,
    reason = "Retained for default-library selection in the CLI."
)]
fn load_default_library(app_dir: &Path) -> Result<(LibraryId, LibraryJson)> {
    let config =
        ConfigJson::load(app_dir)?.ok_or_else(|| anyhow::anyhow!("no libraries configured"))?;
    let library_id = *config
        .get_default_library_id()
        .ok_or_else(|| anyhow::anyhow!("no default library set"))?;
    let library = LibraryJson::load(app_dir, &library_id)?
        .ok_or_else(|| anyhow::anyhow!("library.json not found for default library"))?;
    Ok((library_id, library))
}

/// Get the remote kind for a given remote UUID (as a string).
#[must_use]
pub fn get_remote_kind(library: &LibraryJson, remote_uuid: &str) -> Option<RemoteKind> {
    library
        .remotes
        .iter()
        .find(|r| r.remote_uuid.to_string() == remote_uuid)
        .map(|r| r.kind.clone())
}

/// Validate that a remote UUID (as a string) exists in the library config.
///
/// # Errors
///
/// Returns an error if no configured remote has `remote_uuid`.
#[allow(
    dead_code,
    reason = "Retained for CLI validation of remote UUID arguments."
)]
fn validate_remote_exists(library: &LibraryJson, remote_uuid: &str) -> Result<()> {
    if library
        .remotes
        .iter()
        .any(|r| r.remote_uuid.to_string() == remote_uuid)
    {
        return Ok(());
    }
    anyhow::bail!(
        "Remote '{}' not found. Available remotes: {}",
        remote_uuid,
        list_remote_ids(library).join(", ")
    );
}

/// List all remote UUIDs (as strings) from the library config.
#[must_use]
pub fn list_remote_ids(library: &LibraryJson) -> Vec<String> {
    library
        .remotes
        .iter()
        .map(|r| r.remote_uuid.to_string())
        .collect()
}

/// Find the single remote with the given human-readable name.
/// Errors if zero or more than one remote share that name.
#[allow(
    dead_code,
    reason = "Retained for name-based remote selection in the CLI."
)]
fn find_remote_by_name<'a>(library: &'a LibraryJson, name: &str) -> Result<&'a RemoteConfig> {
    let matches: Vec<&RemoteConfig> = library.remotes.iter().filter(|r| r.name == name).collect();
    match matches.len() {
        0 => anyhow::bail!(
            "Remote '{}' not found. Available remotes: {}",
            name,
            library
                .remotes
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        1 => Ok(matches[0]),
        _ => anyhow::bail!(
            "Remote name '{}' is ambiguous: {} remotes share this name",
            name,
            matches.len()
        ),
    }
}

/// Find a remote by its UUID.
#[must_use]
pub fn find_remote_by_uuid<'a>(
    library: &'a LibraryJson,
    uuid: &RemoteUuid,
) -> Option<&'a RemoteConfig> {
    library.remotes.iter().find(|r| &r.remote_uuid == uuid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_library(remote_path: PathBuf) -> LibraryJson {
        LibraryJson {
            library_nickname: LibraryNickname("test".to_string()),
            device_id: DeviceId(1),
            default_username: None,
            active_password_uuid: None,
            default_fetch_remote: None,
            auto_import_device_media: false,
            media_source_order: vec![],
            remotes: vec![RemoteConfig {
                remote_uuid: RemoteUuid::new(),
                name: "local".to_string(),
                auto_push: true,
                media_fetch_priority: 0,
                exclude_from_media_fetch: false,
                kind: RemoteKind::FixedPath(FixedPathConfig {
                    root_dir: remote_path,
                }),
            }],
        }
    }

    #[test]
    fn library_json_round_trip() {
        let dir = TempDir::new().unwrap();
        let library_id = LibraryId::new();
        let library = make_test_library(dir.path().join("remote"));

        save_library(dir.path(), &library_id, &library).unwrap();

        let (loaded_id, loaded) = load_library_by_nickname(dir.path(), "test").unwrap();
        assert_eq!(loaded_id, library_id);
        assert_eq!(loaded.library_nickname.0, "test");
        assert_eq!(loaded.remotes[0].name, "local");
    }

    #[test]
    fn library_json_serialization() {
        let library = make_test_library(PathBuf::from("/tmp/remote"));
        let json = serde_json::to_string(&library).unwrap();
        let deserialized: LibraryJson = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.library_nickname.0, "test");
        assert!(!json.contains("upload_album"));
    }

    #[test]
    fn cloud_s3_serializes_with_lasco_prefix_and_reads_legacy_name() {
        let kind = RemoteKind::CloudS3(CloudS3Config {
            cloud_storage_id: "cloud-remote".to_string(),
        });
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("lasco_cloud_s3"));

        let legacy: RemoteKind =
            serde_json::from_str(r#"{"kind":"cloud_s3","cloud_storage_id":"cloud-remote"}"#)
                .unwrap();
        assert!(matches!(legacy, RemoteKind::CloudS3(_)));
    }

    #[test]
    fn find_remote_by_name_returns_unique_match() {
        let library = make_test_library(PathBuf::from("/tmp/remote"));
        let found = find_remote_by_name(&library, "local").unwrap();
        assert_eq!(found.name, "local");
    }

    #[test]
    fn find_remote_by_name_errors_when_not_found() {
        let library = make_test_library(PathBuf::from("/tmp/remote"));
        assert!(find_remote_by_name(&library, "missing").is_err());
    }

    #[test]
    fn find_remote_by_name_errors_when_ambiguous() {
        let mut library = make_test_library(PathBuf::from("/tmp/remote"));
        library.remotes.push(RemoteConfig {
            remote_uuid: RemoteUuid::new(),
            name: "local".to_string(),
            auto_push: true,
            media_fetch_priority: 1,
            exclude_from_media_fetch: false,
            kind: RemoteKind::FixedPath(FixedPathConfig {
                root_dir: PathBuf::from("/tmp/remote2"),
            }),
        });

        let err = find_remote_by_name(&library, "local").unwrap_err();
        assert!(err.to_string().contains("ambiguous"));
    }
}
