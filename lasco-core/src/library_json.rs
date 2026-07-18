use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::config_json::{library_data_dir, ConfigJson, LibraryNickname};
use crate::identifiers::{AlbumUuid, LibraryId, RemoteUuid};
use crate::operations::LibraryUsername;

pub const LIBRARY_JSON_VERSION: u32 = 1;

// Remote storage configuration types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub remote_uuid: RemoteUuid,
    pub name: String,
    pub kind: RemoteKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteKind {
    S3(S3Config),
    FixedPath(FixedPathConfig),
    DebugLocalApple(DebugLocalAppleConfig),
    // future: UsbVolume(UsbVolumeConfig)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    /// Secret key encrypted with the library master key (AES-256-GCM), base64 encoded.
    pub secret_key_encrypted: String,
    /// Human-readable description of the encryption scheme.
    pub secret_key_encryption_description: String,
}

/// Trusts a stored absolute path, e.g. a USB or external drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedPathConfig {
    pub root_dir: PathBuf,
}

/// Persists only a name and re-resolves the path against the current app-support
/// directory on every use, so it survives the sandboxed app container's UUID
/// segment changing across relaunches/reinstalls on iOS/macOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugLocalAppleConfig {
    pub local_dir_name: String,
}

/// Per-library configuration stored at `{app_dir}/libraries/{library_id}/library.json`.
/// It holds the library preferences and the ordered list of remotes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryJson {
    /// Configuration version
    #[serde(default = "default_library_version")]
    pub version: u32,
    /// User-friendly nickname for the library
    pub nickname: LibraryNickname,
    /// Default username for this library
    #[serde(default)]
    pub default_username: Option<LibraryUsername>,
    /// UUID of the active password file for the default user (`mk_{username}_{uuid}.enc`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_password_uuid: Option<Uuid>,
    /// Default remote for fetching operations
    pub default_fetch_remote: Option<RemoteUuid>,
    /// Device-local default upload album
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_upload_album: Option<AlbumUuid>,
    /// Whether to automatically import new device media into this library
    #[serde(default)]
    pub auto_import_device_media: bool,
    /// Configured remote storage locations
    pub remotes: Vec<RemoteConfig>,
}

fn default_library_version() -> u32 {
    LIBRARY_JSON_VERSION
}

/// Path to a library's `library.json`
pub fn library_json_path(app_dir: &Path, library_id: &LibraryId) -> PathBuf {
    library_data_dir(app_dir, library_id).join("library.json")
}

impl LibraryJson {
    /// Load a library's configuration from disk.
    pub fn load(app_dir: &Path, library_id: &LibraryId) -> Result<Option<Self>> {
        let path = library_json_path(app_dir, library_id);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path)
            .with_context(|| format!("failed to read library config from {}", path.display()))?;
        let config: LibraryJson =
            serde_json::from_slice(&data).context("failed to parse library.json")?;
        if config.version != LIBRARY_JSON_VERSION {
            bail!(
                "unsupported library config version {} (expected {})",
                config.version,
                LIBRARY_JSON_VERSION
            );
        }
        Ok(Some(config))
    }

    /// Save a library's configuration to disk.
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

/// Persist a library's configuration and register it in the global index.
pub fn save_library(app_dir: &Path, library_id: &LibraryId, library: &LibraryJson) -> Result<()> {
    library.save(app_dir, library_id)?;
    let mut config = ConfigJson::load(app_dir)?.unwrap_or_default();
    config.add_or_update_library(*library_id, library.nickname.clone());
    config.save(app_dir)?;
    Ok(())
}

/// Load the configuration for the library with the given nickname.
pub fn load_library_by_nickname(
    app_dir: &Path,
    nickname: &str,
) -> Result<(LibraryId, LibraryJson)> {
    let config =
        ConfigJson::load(app_dir)?.ok_or_else(|| anyhow::anyhow!("no libraries configured"))?;
    let library_id = *config
        .get_library_id_by_nickname(nickname)
        .ok_or_else(|| anyhow::anyhow!("library '{nickname}' not found"))?;
    let library = LibraryJson::load(app_dir, &library_id)?
        .ok_or_else(|| anyhow::anyhow!("library.json not found for '{nickname}'"))?;
    Ok((library_id, library))
}

/// Load the configuration for the default library.
pub fn load_default_library(app_dir: &Path) -> Result<(LibraryId, LibraryJson)> {
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
pub fn get_remote_kind(library: &LibraryJson, remote_uuid: &str) -> Option<RemoteKind> {
    library
        .remotes
        .iter()
        .find(|r| r.remote_uuid.to_string() == remote_uuid)
        .map(|r| r.kind.clone())
}

/// Validate that a remote UUID (as a string) exists in the library config.
pub fn validate_remote_exists(library: &LibraryJson, remote_uuid: &str) -> Result<()> {
    if library.remotes.iter().any(|r| r.remote_uuid.to_string() == remote_uuid) {
        return Ok(());
    }
    anyhow::bail!(
        "Remote '{}' not found. Available remotes: {}",
        remote_uuid,
        list_remote_ids(library).join(", ")
    );
}

/// List all remote UUIDs (as strings) from the library config.
pub fn list_remote_ids(library: &LibraryJson) -> Vec<String> {
    library.remotes.iter().map(|r| r.remote_uuid.to_string()).collect()
}

/// Find the single remote with the given human-readable name.
/// Errors if zero or more than one remote share that name.
pub fn find_remote_by_name<'a>(library: &'a LibraryJson, name: &str) -> Result<&'a RemoteConfig> {
    let matches: Vec<&RemoteConfig> = library.remotes.iter().filter(|r| r.name == name).collect();
    match matches.len() {
        0 => anyhow::bail!(
            "Remote '{}' not found. Available remotes: {}",
            name,
            library.remotes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ")
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
pub fn find_remote_by_uuid<'a>(library: &'a LibraryJson, uuid: &RemoteUuid) -> Option<&'a RemoteConfig> {
    library.remotes.iter().find(|r| &r.remote_uuid == uuid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_library(remote_path: PathBuf) -> LibraryJson {
        LibraryJson {
            version: LIBRARY_JSON_VERSION,
            nickname: LibraryNickname("test".to_string()),
            default_username: None,
            active_password_uuid: None,
            default_fetch_remote: None,
            default_upload_album: None,
            auto_import_device_media: false,
            remotes: vec![RemoteConfig {
                remote_uuid: RemoteUuid::new(),
                name: "local".to_string(),
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
        assert_eq!(loaded.nickname.0, "test");
        assert_eq!(loaded.remotes[0].name, "local");
    }

    #[test]
    fn library_json_serialization() {
        let library = make_test_library(PathBuf::from("/tmp/remote"));
        let json = serde_json::to_string(&library).unwrap();
        let deserialized: LibraryJson = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.nickname.0, "test");
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
            kind: RemoteKind::FixedPath(FixedPathConfig {
                root_dir: PathBuf::from("/tmp/remote2"),
            }),
        });

        let err = find_remote_by_name(&library, "local").unwrap_err();
        assert!(err.to_string().contains("ambiguous"));
    }
}
