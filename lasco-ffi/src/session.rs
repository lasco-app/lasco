use lasco_core::identifiers::LibraryId;
use lasco_core::operations::LibraryUsername;

use crate::error::LascoError;

#[uniffi::export]
pub fn session_clear(library_id: String, username: String) -> Result<(), LascoError> {
    let app_dir = lasco_core::config_json::default_app_dir()?;
    let sessions = app_dir.join("sessions");
    let uuid = uuid::Uuid::parse_str(&library_id)
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;
    let id = LibraryId(uuid);
    let user = LibraryUsername(username);
    lasco_core::session::session_clear(id, &user, Some(&sessions))
        .map_err(|e| LascoError::Other { msg: e.to_string() })
}
