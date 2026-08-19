use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
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

/// Returns the platform-default application data directory for lasco.
///
/// # Errors
///
/// Returns an error when the platform does not provide a suitable project data directory.
pub fn default_app_dir() -> Result<PathBuf> {
    directories::ProjectDirs::from("", "", "lasco")
        .map(|d| d.data_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("could not determine project directories"))
}

/// Path to the application configuration file
#[must_use]
pub fn app_config_path(app_dir: &Path) -> PathBuf {
    app_dir.join("config.json")
}

/// Path to the library data directory
#[must_use]
pub fn library_data_dir(app_dir: &Path, library_id: &LibraryId) -> PathBuf {
    app_dir.join("libraries").join(library_id.to_string())
}

// Application Configuration (config.json)

/// Global index of libraries.
/// Stored at `{app_dir}/config.json`. It records library IDs and the default library ID.
/// Library names, remotes, and credentials live in each library's `library.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigJson {
    /// Configuration version
    #[serde(default = "default_version")]
    pub version: u32,
    /// ID of the default library
    pub default_library_id: Option<LibraryId>,
    /// All registered library IDs
    pub libraries: Vec<LibraryId>,
}

impl Default for ConfigJson {
    fn default() -> Self {
        Self {
            version: APP_CONFIG_VERSION,
            default_library_id: None,
            libraries: vec![],
        }
    }
}

fn default_version() -> u32 {
    APP_CONFIG_VERSION
}

impl ConfigJson {
    /// Load the application config from disk
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, is invalid JSON, or has an unsupported version.
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

    /// Get the default library id
    #[must_use]
    pub fn get_default_library_id(&self) -> Option<&LibraryId> {
        self.default_library_id.as_ref()
    }

    /// Register a library.
    pub fn add_library(&mut self, library_id: LibraryId) {
        // If this is the first library, set it as default
        if self.libraries.is_empty() && self.default_library_id.is_none() {
            self.default_library_id = Some(library_id);
        }
        if !self.libraries.contains(&library_id) {
            self.libraries.push(library_id);
        }
    }

    /// Remove a library by its ID
    pub(crate) fn remove_library(&mut self, library_id: &LibraryId) -> Result<()> {
        let index = self
            .libraries
            .iter()
            .position(|id| id == library_id)
            .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

        self.libraries.remove(index);

        // If the removed library was the default, clear the default
        if self.default_library_id.as_ref() == Some(library_id) {
            self.default_library_id = None;
            // If there are remaining libraries, set the first as default
            if let Some(first_library_id) = self.libraries.first() {
                self.default_library_id = Some(*first_library_id);
            }
        }

        Ok(())
    }

    /// Set the default library by ID.
    #[allow(
        dead_code,
        reason = "Retained for the CLI command that sets the default library."
    )]
    fn set_default_library(&mut self, library_id: LibraryId) -> Result<()> {
        if !self.libraries.contains(&library_id) {
            anyhow::bail!("Library '{library_id}' not found");
        }
        self.default_library_id = Some(library_id);
        Ok(())
    }

    /// Get all library IDs
    #[must_use]
    pub fn library_ids(&self) -> Vec<&LibraryId> {
        self.libraries.iter().collect()
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
        config.add_library(library_id);
        config.save(dir.path()).unwrap();

        let loaded = ConfigJson::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.libraries.len(), 1);
        assert_eq!(loaded.default_library_id, Some(library_id));
    }
}
