mod albums;
mod groups;
mod media;
mod remotes;
mod types;

pub use types::*;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lasco_core::config_json::{ConfigJson, LibraryNickname};
use lasco_core::library_json::LibraryJson;
use lasco_core::operations::{LibraryPassword, LibraryUsername};
use lasco_core::session::session_load_master_key;

use remotes::remote_config_to_ffi;

use crate::error::LascoError;

fn sessions_dir(app_dir: &std::path::Path) -> std::path::PathBuf {
    app_dir.join("sessions")
}

#[uniffi::export]
pub fn ffi_create_library(
    nickname: String,
    username: String,
    password: String,
) -> Result<FfiCreateLibraryResult, LascoError> {
    let app_dir = lasco_core::config_json::default_app_dir()?;
    let sessions = sessions_dir(&app_dir);
    let (library_id, master_key) = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(lasco_core::client::create_library(
            &app_dir,
            nickname,
            lasco_core::operations::LibraryUsername(username),
            lasco_core::operations::LibraryPassword(password),
            Some(&sessions),
        ))?;
    let master_key_hex = master_key.as_ref().iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(FfiCreateLibraryResult {
        library_id: library_id.to_string(),
        master_key_hex,
    })
}

#[uniffi::export]
pub fn ffi_delete_library(library_id: String) -> Result<(), LascoError> {
    let app_dir = lasco_core::config_json::default_app_dir()?;
    let sessions = sessions_dir(&app_dir);
    let uuid = uuid::Uuid::parse_str(&library_id)
        .map_err(|e| LascoError::Other { msg: format!("invalid library id: {e}") })?;
    let lib_id = lasco_core::identifiers::LibraryId(uuid);
    lasco_core::client::delete_library(&app_dir, &lib_id, Some(&sessions))
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;
    Ok(())
}

#[uniffi::export]
pub fn list_libraries() -> Result<Vec<FfiLibraryEntry>, LascoError> {
    let app_dir = lasco_core::config_json::default_app_dir()?;
    let config = match ConfigJson::load(&app_dir)? {
        Some(c) => c,
        None => return Ok(vec![]),
    };
    Ok(config
        .libraries
        .iter()
        .map(|(id, entry)| {
            // The nickname lives in the index. The username lives in library.json.
            match LibraryJson::load(&app_dir, id) {
                Ok(Some(lib)) => FfiLibraryEntry {
                    id: id.to_string(),
                    nickname: lib.nickname.0,
                    username: lib.default_username.map(|u| u.0),
                    load_error: None,
                },
                Ok(None) => FfiLibraryEntry {
                    id: id.to_string(),
                    nickname: entry.nickname.0.clone(),
                    username: None,
                    load_error: Some("library.json not found".to_string()),
                },
                Err(e) => FfiLibraryEntry {
                    id: id.to_string(),
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
#[uniffi::export]
pub fn ffi_test_s3_remote(
    endpoint: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
) -> Result<(), LascoError> {
    let storage = lasco_core::storage::StorageS3::new(endpoint, bucket, region, access_key, secret_key)
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;
    rt.block_on(lasco_core::storage::Storage::list(&storage, ""))
        .map_err(|e| LascoError::Other { msg: format!("remote unreachable: {e}") })?;
    Ok(())
}

/// Try to open a library using a cached session (OS keychain), without a password.
/// Returns `None` if no session is cached — the caller should then prompt for credentials.
#[uniffi::export]
pub fn ffi_open_cached(
    nickname: Option<String>,
    username: String,
) -> Result<Option<Arc<FfiLibrary>>, LascoError> {
    let app_dir = lasco_core::config_json::default_app_dir()?;

    let config = match ConfigJson::load(&app_dir)? {
        Some(c) => c,
        None => return Ok(None),
    };

    let resolved = match config.resolve_nickname(nickname.map(LibraryNickname::from)) {
        Ok(n) => n,
        Err(_) => return Ok(None),
    };

    let library_id = match config.get_library_id_by_nickname(&resolved.0) {
        Some(id) => *id,
        None => return Ok(None),
    };
    let library_config = match LibraryJson::load(&app_dir, &library_id)? {
        Some(lc) => lc,
        None => return Ok(None),
    };

    let lib_username = LibraryUsername(username);

    let sessions = sessions_dir(&app_dir);
    let has_session =
        session_load_master_key(library_id, &lib_username, Some(&sessions))
            .map_err(|e| LascoError::Other { msg: e.to_string() })?
            .is_some();

    if !has_session {
        return Ok(None);
    }

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;

    let sync_remote_id = library_config.remotes.first().map(|r| r.remote_uuid.to_string());
    let remotes = library_config.remotes.iter().map(remote_config_to_ffi).collect();

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
        sync_remote_id,
        remotes: Mutex::new(remotes),
    })))
}

/// Add a library that already exists on an S3 remote, downloading its crypto
/// metadata and operations and opening it locally. `username`/`password` must be
/// an existing user on the remote. When `new_username`/`new_password` are both
/// provided, a new user is registered and used as the effective device user.
#[uniffi::export]
#[allow(clippy::too_many_arguments)]
pub fn ffi_add_existing_library_s3(
    nickname: String,
    username: String,
    password: String,
    new_username: Option<String>,
    new_password: Option<String>,
    remote_id: String,
    endpoint: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
) -> Result<Arc<FfiLibrary>, LascoError> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;

    let app_dir = lasco_core::config_json::default_app_dir()?;
    let sessions = sessions_dir(&app_dir);

    let new_user = match (new_username, new_password) {
        (Some(u), Some(p)) if !u.is_empty() => {
            Some((LibraryUsername(u), LibraryPassword(p)))
        }
        _ => None,
    };

    let (_library_id, library) = rt
        .block_on(lasco_core::client::add_existing_library_s3(
            &app_dir,
            nickname,
            LibraryUsername(username),
            LibraryPassword(password),
            new_user,
            remote_id.clone(),
            endpoint,
            bucket,
            region,
            access_key,
            secret_key,
            Some(&sessions),
        ))
        .map_err(|e| LascoError::Other { msg: e.to_string() })?;

    let library_config = LibraryJson::load(&app_dir, &library.library_id())?
        .ok_or(LascoError::NotFound)?;
    let sync_remote_id = library_config
        .remotes
        .iter()
        .find(|r| r.name == remote_id)
        .map(|r| r.remote_uuid.to_string());
    let remotes = library_config
        .remotes
        .iter()
        .map(remote_config_to_ffi)
        .collect();

    Ok(Arc::new(FfiLibrary {
        inner: library,
        rt,
        app_dir,
        sync_remote_id,
        remotes: Mutex::new(remotes),
    }))
}

#[derive(uniffi::Object, Debug)]
pub struct FfiLibrary {
    inner: lasco_core::library::Library,
    rt: tokio::runtime::Runtime,
    app_dir: PathBuf,
    sync_remote_id: Option<String>,
    remotes: Mutex<Vec<FfiRemote>>,
}

#[uniffi::export]
impl FfiLibrary {
    /// Open a library by nickname. Delegates config loading, storage
    /// construction, and session/master-key handling to `lasco_core::client`.
    #[uniffi::constructor]
    pub fn open(
        nickname: Option<String>,
        username: String,
        password: String,
    ) -> Result<Arc<Self>, LascoError> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| LascoError::Other { msg: e.to_string() })?;

        let app_dir = lasco_core::config_json::default_app_dir()?;

        let config = ConfigJson::load(&app_dir)?
            .ok_or_else(|| LascoError::Other {
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
        let sync_remote_id = library_config.remotes.first().map(|r| r.remote_uuid.to_string());

        let remotes = library_config.remotes.iter().map(remote_config_to_ffi).collect();

        let sessions = sessions_dir(&app_dir);
        let library = rt.block_on(lasco_core::client::open_library(
            &app_dir,
            resolved,
            LibraryUsername(username),
            Some(LibraryPassword(password)),
            Some(&sessions),
        ))?;

        Ok(Arc::new(Self { inner: library, rt, app_dir, sync_remote_id, remotes: Mutex::new(remotes) }))
    }
}
