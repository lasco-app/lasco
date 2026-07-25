uniffi::setup_scaffolding!();

pub mod error;
pub mod library;
pub mod session;

/// Resolve the main app-data directory. When the caller passes an explicit path
/// (Android supplies its app-private dir) use it, otherwise fall back to the
/// `directories` crate default used by iOS and macOS.
pub(crate) fn resolve_app_dir(
    app_dir: Option<String>,
) -> Result<std::path::PathBuf, crate::error::LascoError> {
    match app_dir {
        Some(p) => Ok(std::path::PathBuf::from(p)),
        None => Ok(lasco_core::config_json::default_app_dir()?),
    }
}
