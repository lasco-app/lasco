use lasco_core::identifiers::LibraryId;
use lasco_core::operations::LibraryUsername;

use crate::error::LascoError;

#[uniffi::export(default(app_dir = None))]
pub fn session_clear(
    library_id: String,
    username: String,
    app_dir: Option<String>,
) -> Result<(), LascoError> {
    let app_dir = crate::resolve_app_dir(app_dir)?;
    let sessions = app_dir.join("sessions");
    let uuid = uuid::Uuid::parse_str(&library_id)
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;
    let id = LibraryId(uuid);
    let user = LibraryUsername(username);
    lasco_core::session::session_clear(id, &user, Some(&sessions))
        .map_err(|e| LascoError::Other { msg: e.to_string() })
}
