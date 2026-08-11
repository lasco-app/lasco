mod albums;
mod groups;
mod media;
mod remotes;
mod types;

pub use types::*;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lasco_core::config_json::{ConfigJson, LibraryNickname};
use lasco_core::library_json::LibraryJson;
use lasco_core::operations::{LibraryPassword, LibraryUsername};
use lasco_core::session::session_load_master_key;

use remotes::remote_config_to_ffi;

use crate::error::LascoError;
use crate::ids::FfiLibraryId;

pub(super) fn ffi_count(value: usize) -> u64 {
    u64::try_from(value).expect("usize fits in u64 on supported UniFFI targets")
}

fn sessions_dir(app_dir: &std::path::Path) -> std::path::PathBuf {
    app_dir.join("sessions")
}

#[uniffi::export(default(app_dir = None))]
/// # Errors
///
/// Returns an error if the app directory/runtime cannot be created or library state, config, or session key cannot be initialized.
///
/// # Panics
///
/// Panics if Tokio cannot construct the runtime used to initialize the library.
pub fn ffi_create_library(
    nickname: String,
    username: String,
    password: String,
    app_dir: Option<String>,
) -> Result<FfiCreateLibraryResult, LascoError> {
    let app_dir = crate::resolve_app_dir(app_dir)?;
    let sessions = sessions_dir(&app_dir);
    let (library_id, master_key) =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(lasco_core::client::create_library(
                &app_dir,
                nickname,
                lasco_core::operations::LibraryUsername(username),
                lasco_core::operations::LibraryPassword(password),
                Some(&sessions),
            ))?;
    let mut master_key_hex = String::with_capacity(master_key.as_ref().len() * 2);
    for byte in master_key.as_ref() {
        write!(master_key_hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(FfiCreateLibraryResult {
        library_id: library_id.into(),
        master_key_hex,
    })
}

#[uniffi::export(default(app_dir = None))]
/// # Errors
///
/// Returns an error if the library ID is invalid or local data, session state, or app configuration cannot be removed or updated.
pub fn ffi_delete_library(
    library_id: FfiLibraryId,
    app_dir: Option<String>,
) -> Result<(), LascoError> {
    let app_dir = crate::resolve_app_dir(app_dir)?;
    let sessions = sessions_dir(&app_dir);
    let lib_id = library_id.try_into()?;
    lasco_core::client::delete_library(&app_dir, &lib_id, Some(&sessions))
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;
    Ok(())
}

#[uniffi::export(default(app_dir = None))]
/// # Errors
///
/// Returns an error if the application configuration cannot be read; per-library load failures are returned in each entry.
pub fn list_libraries(app_dir: Option<String>) -> Result<Vec<FfiLibraryEntry>, LascoError> {
    let app_dir = crate::resolve_app_dir(app_dir)?;
    let Some(config) = ConfigJson::load(&app_dir)? else {
        return Ok(vec![]);
    };
    Ok(config
        .libraries
        .iter()
        .map(|(id, entry)| {
            // The nickname lives in the index. The username lives in library.json.
            match LibraryJson::load(&app_dir, id) {
                Ok(Some(lib)) => FfiLibraryEntry {
                    library_id: (*id).into(),
                    nickname: lib.nickname.0,
                    username: lib.default_username.map(|u| u.0),
                    load_error: None,
                },
                Ok(None) => FfiLibraryEntry {
                    library_id: (*id).into(),
                    nickname: entry.nickname.0.clone(),
                    username: None,
                    load_error: Some("library.json not found".to_string()),
                },
                Err(e) => FfiLibraryEntry {
                    library_id: (*id).into(),
                    nickname: entry.nickname.0.clone(),
                    username: None,
                    load_error: Some(e.to_string()),
                },
            }
        })
        .collect())
}

/// Test connectivity to an S3 remote using the given credentials, without
/// saving anything. Builds an ephemeral client and lists the bucket root.
///
/// # Errors
///
/// Returns an error if the S3 client or runtime cannot be created, or the bucket cannot be listed.
#[uniffi::export]
pub fn ffi_test_s3_remote(
    endpoint: String,
    bucket: String,
    region: String,
    path_prefix: String,
    access_key: String,
    secret_key: String,
) -> Result<(), LascoError> {
    let path_prefix = if path_prefix.is_empty() {
        None
    } else {
        Some(path_prefix)
    };
    let storage = lasco_core::storage::StorageS3::new(
        endpoint,
        bucket,
        region,
        path_prefix,
        access_key,
        secret_key,
    )
    .map_err(|e| LascoError::Other { msg: e.to_string() })?;
    let rt =
        tokio::runtime::Runtime::new().map_err(|e| LascoError::Other { msg: e.to_string() })?;
    rt.block_on(lasco_core::storage::Storage::list(&storage, ""))
        .map_err(|e| LascoError::Other {
            msg: format!("remote unreachable: {e}"),
        })?;
    Ok(())
}

/// Try to open a library using a cached session (OS keychain), without a password.
/// Returns `None` if no session is cached — the caller should then prompt for credentials.
///
/// # Errors
///
/// Returns an error if configuration or the session key cannot be read, or opening a cached library fails.
#[uniffi::export(default(app_dir = None))]
pub fn ffi_open_cached(
    nickname: Option<String>,
    username: String,
    app_dir: Option<String>,
) -> Result<Option<Arc<FfiLibrary>>, LascoError> {
    let app_dir = crate::resolve_app_dir(app_dir)?;

    let Some(config) = ConfigJson::load(&app_dir)? else {
        return Ok(None);
    };

    let Ok(resolved) = config.resolve_nickname(nickname.map(LibraryNickname::from)) else {
        return Ok(None);
    };

    let library_id = match config.get_library_id_by_nickname(&resolved.0) {
        Some(id) => *id,
        None => return Ok(None),
    };
    let Some(library_config) = LibraryJson::load(&app_dir, &library_id)? else {
        return Ok(None);
    };

    let lib_username = LibraryUsername(username);

    let sessions = sessions_dir(&app_dir);
    let has_session = session_load_master_key(library_id, &lib_username, Some(&sessions))
        .map_err(|e| LascoError::Other { msg: e.to_string() })?
        .is_some();

    if !has_session {
        return Ok(None);
    }

    let rt =
        tokio::runtime::Runtime::new().map_err(|e| LascoError::Other { msg: e.to_string() })?;

    let remotes = library_config
        .remotes
        .iter()
        .map(remote_config_to_ffi)
        .collect();

    let library = rt.block_on(lasco_core::client::open_library(
        &app_dir,
        resolved,
        lib_username,
        None,
        Some(&sessions),
    ))?;

    Ok(Some(Arc::new(FfiLibrary {
        inner: library,
        rt,
        app_dir,
        remotes: Mutex::new(remotes),
    })))
}

/// Add a library that already exists on an S3 remote, downloading its crypto
/// metadata and operations and opening it locally. `username`/`password` must be
/// an existing user on the remote. When `new_username`/`new_password` are both
/// provided, a new user is registered and used as the effective device user.
///
/// # Errors
///
/// Returns an error if runtime/app setup, remote connection or authentication, local persistence, or initial synchronization fails.
#[uniffi::export(default(app_dir = None))]
#[allow(
    clippy::too_many_arguments,
    reason = "The FFI contract exposes S3 connection settings as explicit scalar parameters."
)]
pub fn ffi_add_existing_library_s3(
    nickname: String,
    username: String,
    password: String,
    new_username: Option<String>,
    new_password: Option<String>,
    remote_name: String,
    endpoint: String,
    bucket: String,
    region: String,
    path_prefix: String,
    access_key: String,
    secret_key: String,
    app_dir: Option<String>,
) -> Result<Arc<FfiLibrary>, LascoError> {
    let rt =
        tokio::runtime::Runtime::new().map_err(|e| LascoError::Other { msg: e.to_string() })?;

    let app_dir = crate::resolve_app_dir(app_dir)?;
    let sessions = sessions_dir(&app_dir);

    let new_user = match (new_username, new_password) {
        (Some(u), Some(p)) if !u.is_empty() => Some((LibraryUsername(u), LibraryPassword(p))),
        _ => None,
    };

    let path_prefix = if path_prefix.is_empty() {
        None
    } else {
        Some(path_prefix)
    };

    let (_library_id, library) = rt
        .block_on(lasco_core::client::add_existing_library_s3(
            &app_dir,
            nickname,
            LibraryUsername(username),
            LibraryPassword(password),
            new_user,
            remote_name.clone(),
            endpoint,
            bucket,
            region,
            path_prefix,
            access_key,
            secret_key,
            Some(&sessions),
        ))
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;

    let library_config =
        LibraryJson::load(&app_dir, &library.library_id())?.ok_or(LascoError::NotFound)?;
    let remotes = library_config
        .remotes
        .iter()
        .map(remote_config_to_ffi)
        .collect();

    Ok(Arc::new(FfiLibrary {
        inner: library,
        rt,
        app_dir,
        remotes: Mutex::new(remotes),
    }))
}

#[derive(uniffi::Object, Debug)]
pub struct FfiLibrary {
    inner: lasco_core::library::Library,
    rt: tokio::runtime::Runtime,
    app_dir: PathBuf,
    remotes: Mutex<Vec<FfiRemote>>,
}

#[uniffi::export]
impl FfiLibrary {
    /// Open a library by nickname. Delegates config loading, storage
    /// construction, and session/master-key handling to `lasco_core::client`.
    ///
    /// # Errors
    ///
    /// Returns an error if setup/configuration fails, the nickname is unknown, or credentials cannot open the library.
    #[uniffi::constructor(default(app_dir = None))]
    pub fn open(
        nickname: Option<String>,
        username: String,
        password: String,
        app_dir: Option<String>,
    ) -> Result<Arc<Self>, LascoError> {
        let rt =
            tokio::runtime::Runtime::new().map_err(|e| LascoError::Other { msg: e.to_string() })?;

        let app_dir = crate::resolve_app_dir(app_dir)?;

        let config = ConfigJson::load(&app_dir)?.ok_or_else(|| LascoError::Other {
            msg: "no libraries configured".to_string(),
        })?;

        let resolved = config
            .resolve_nickname(nickname.map(LibraryNickname::from))
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        let library_id = config
            .get_library_id_by_nickname(&resolved.0)
            .ok_or(LascoError::NotFound)?;
        let library_config =
            LibraryJson::load(&app_dir, library_id)?.ok_or(LascoError::NotFound)?;
        let remotes = library_config
            .remotes
            .iter()
            .map(remote_config_to_ffi)
            .collect();

        let sessions = sessions_dir(&app_dir);
        let library = rt.block_on(lasco_core::client::open_library(
            &app_dir,
            resolved,
            LibraryUsername(username),
            Some(LibraryPassword(password)),
            Some(&sessions),
        ))?;

        Ok(Arc::new(Self {
            inner: library,
            rt,
            app_dir,
            remotes: Mutex::new(remotes),
        }))
    }
}
