use lasco_core::operations::LibraryUsername;

use crate::error::LascoError;
use crate::ids::FfiLibraryId;

#[uniffi::export(default(app_dir = None))]
/// # Errors
///
/// Returns an error if the library ID is invalid or its file-based session cannot be cleared.
pub fn session_clear(
    library_id: FfiLibraryId,
    username: String,
    app_dir: Option<String>,
) -> Result<(), LascoError> {
    let app_dir = crate::resolve_app_dir(app_dir)?;
    let sessions = app_dir.join("sessions");
    let id = library_id.try_into()?;
    let user = LibraryUsername(username);
    lasco_core::session::session_clear(id, &user, Some(&sessions))
        .map_err(|e| LascoError::Other { msg: e.to_string() })
}
