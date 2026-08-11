use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::identifiers::LibraryId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LibraryNickname(pub String);

impl From<String> for LibraryNickname {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for LibraryNickname {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for LibraryNickname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub const APP_CONFIG_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no libraries configured; use 'lasco new' to create one")]
    NoLibraries,
    #[error("no default library set; use 'lasco library default' to set one")]
    NoDefaultLibrary,
}

/// Returns the platform-default application data directory for lasco.
pub fn default_app_dir() -> Result<PathBuf> {
    directories::ProjectDirs::from("", "", "lasco")
        .map(|d| d.data_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("could not determine project directories"))
}

/// Path to the application configuration file
pub fn app_config_path(app_dir: &Path) -> PathBuf {
    app_dir.join("config.json")
}

/// Path to the library data directory
pub fn library_data_dir(app_dir: &Path, library_id: &LibraryId) -> PathBuf {
    app_dir.join("libraries").join(library_id.to_string())
}

// Application Configuration (config.json)

/// Index entry for a single library in the global config.
/// The full per-library configuration lives in `library.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryIndexEntry {
    /// User-friendly nickname for the library
    pub nickname: LibraryNickname,
}

/// Global index of libraries.
/// Stored at `{app_dir}/config.json`. It maps each library id to its nickname
/// and records the default library. It holds no remote or credential data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigJson {
    /// Configuration version
    #[serde(default = "default_version")]
    pub version: u32,
    /// Nickname of the default library
    pub default_library: Option<LibraryNickname>,
    /// All registered libraries, keyed by `library_id`
    pub libraries: HashMap<LibraryId, LibraryIndexEntry>,
}

impl Default for ConfigJson {
    fn default() -> Self {
        Self {
            version: APP_CONFIG_VERSION,
            default_library: None,
            libraries: HashMap::new(),
        }
    }
}

fn default_version() -> u32 {
    APP_CONFIG_VERSION
}

impl ConfigJson {
    /// Load the application config from disk
    pub fn load(app_dir: &Path) -> Result<Option<Self>> {
        let path = app_config_path(app_dir);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path)
            .with_context(|| format!("failed to read app config from {}", path.display()))?;
        let config: ConfigJson =
            serde_json::from_slice(&data).context("failed to parse config.json")?;
        if config.version != APP_CONFIG_VERSION {
            bail!(
                "unsupported config version {} (expected {})",
                config.version,
                APP_CONFIG_VERSION
            );
        }
        Ok(Some(config))
    }

    /// Save the application config to disk
    pub(crate) fn save(&self, app_dir: &Path) -> Result<()> {
        let path = app_config_path(app_dir);
        fs::create_dir_all(app_dir)
            .with_context(|| format!("Failed to create app directory {}", app_dir.display()))?;
        let data = serde_json::to_vec_pretty(self).context("failed to serialize app config")?;
        fs::write(&path, &data)
            .with_context(|| format!("failed to write app config to {}", path.display()))?;
        Ok(())
    }

    /// Get a library id by its nickname
    pub fn get_library_id_by_nickname(&self, nickname: &str) -> Option<&LibraryId> {
        self.libraries
            .iter()
            .find(|(_, lib)| lib.nickname.0 == nickname)
            .map(|(id, _)| id)
    }

    /// Get the default library id
    pub fn get_default_library_id(&self) -> Option<&LibraryId> {
        self.default_library
            .as_ref()
            .and_then(|nickname| self.get_library_id_by_nickname(&nickname.0))
    }

    /// Register a library or update its nickname.
    pub fn add_or_update_library(&mut self, library_id: LibraryId, nickname: LibraryNickname) {
        // If this is the first library, set it as default
        if self.libraries.is_empty() && self.default_library.is_none() {
            self.default_library = Some(nickname.clone());
        }
        self.libraries
            .insert(library_id, LibraryIndexEntry { nickname });
    }

    /// Remove a library by its ID
    pub(crate) fn remove_library(&mut self, library_id: &LibraryId) -> Result<()> {
        let nickname = self
            .libraries
            .get(library_id)
            .map(|lib| lib.nickname.clone())
            .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

        self.libraries.remove(library_id);

        // If the removed library was the default, clear the default
        if self.default_library.as_ref() == Some(&nickname) {
            self.default_library = None;
            // If there are remaining libraries, set the first as default
            if let Some((_, first_lib)) = self.libraries.iter().next() {
                self.default_library = Some(first_lib.nickname.clone());
            }
        }

        Ok(())
    }

    /// Set the default library by nickname
    #[allow(
        dead_code,
        reason = "Retained for the CLI command that sets the default library."
    )]
    fn set_default_library(&mut self, nickname: &str) -> Result<()> {
        if !self
            .libraries
            .values()
            .any(|lib| lib.nickname.0 == nickname)
        {
            anyhow::bail!("Library with nickname '{nickname}' not found");
        }
        self.default_library = Some(LibraryNickname(nickname.to_string()));
        Ok(())
    }

    /// Get all library IDs
    pub fn library_ids(&self) -> Vec<&LibraryId> {
        self.libraries.keys().collect()
    }

    /// Resolve an optional nickname to a concrete one.
    /// Returns `nickname` if `Some`, otherwise returns the default library nickname.
    pub fn resolve_nickname(
        &self,
        nickname: Option<LibraryNickname>,
    ) -> Result<LibraryNickname, ConfigError> {
        match nickname {
            Some(n) => Ok(n),
            None => self
                .default_library
                .clone()
                .ok_or(ConfigError::NoDefaultLibrary),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_empty_when_not_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = ConfigJson::load(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn app_config_load_and_save() {
        let dir = tempfile::TempDir::new().unwrap();
        let library_id = LibraryId::new();

        let mut config = ConfigJson::default();
        config.add_or_update_library(library_id, LibraryNickname("test".to_string()));
        config.save(dir.path()).unwrap();

        let loaded = ConfigJson::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.libraries.len(), 1);
        assert_eq!(
            loaded.default_library,
            Some(LibraryNickname("test".to_string()))
        );
    }
}
