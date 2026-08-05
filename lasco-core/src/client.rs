use std::path::Path;

use anyhow::Result;

use anyhow::Context;

use crate::config_json::{ConfigJson, LibraryNickname, library_data_dir};
use crate::encryption::master_key::{MasterKey, find_master_key};
use crate::identifiers::LibraryId;
use crate::library::Library;
use crate::library::local_dirs::LocalDirs;
use crate::library_json::{LibraryJson, RemoteConfig, RemoteKind, S3Config, save_library};
use crate::operations::{LibraryPassword, LibraryUsername};
use crate::s3_secret::{encrypt_s3_secret_key, resolve_s3_credentials};
use crate::session::{session_load_master_key, session_store_master_key};
use crate::storage::{Storage, StorageLocalFs, StorageS3};

pub fn local_storage_path(app_dir: &Path, library_id: &LibraryId) -> std::path::PathBuf {
    app_dir.join("local_storage").join(library_id.to_string())
}

pub fn build_storage(
    app_dir: &Path,
    library: &LibraryJson,
    library_id: &LibraryId,
    master_key: Option<&MasterKey>,
    app_support_dir: Option<&Path>,
) -> Result<Box<dyn Storage + Send + Sync>> {
    // Libraries with no remotes always use the convention local storage path.
    if library.remotes.is_empty() {
        let path = local_storage_path(app_dir, library_id);
        return Ok(Box::new(StorageLocalFs::new(&path)?));
    }

    let primary = &library.remotes[0];
    match &primary.kind {
        RemoteKind::FixedPath(cfg) => Ok(Box::new(StorageLocalFs::new(&cfg.root_dir)?)),
        RemoteKind::DebugLocalApple(cfg) => {
            let app_support_dir = app_support_dir.ok_or_else(|| {
                anyhow::anyhow!("app_support_dir required for debug_local_apple remote")
            })?;
            let path = app_support_dir
                .join("lasco")
                .join("local_fs_test")
                .join(&cfg.local_dir_name);
            Ok(Box::new(StorageLocalFs::new(&path)?))
        }
        RemoteKind::DebugLocalAndroid(cfg) => {
            let path = app_dir.join("local_fs_test").join(&cfg.local_dir_name);
            Ok(Box::new(StorageLocalFs::new(&path)?))
        }
        RemoteKind::S3(s3_cfg) => {
            let master_key = master_key
                .ok_or_else(|| anyhow::anyhow!("master key required to decrypt S3 credentials"))?;
            let (access_key, secret_key) = resolve_s3_credentials(s3_cfg, master_key)
                .map_err(|e| anyhow::anyhow!("failed to resolve S3 credentials: {e}"))?;
            Ok(Box::new(StorageS3::new(
                s3_cfg.endpoint.clone(),
                s3_cfg.bucket.clone(),
                s3_cfg.region.clone(),
                s3_cfg.path_prefix.clone(),
                access_key,
                secret_key,
            )?))
        }
    }
}

/// Open a library by nickname with optional session-key caching.
///
/// If a cached master key exists in the session, `password` is not needed and may be `None`.
/// If no cached key exists and `password` is `None`, returns an error.
pub async fn open_library(
    app_dir: &Path,
    library_nickname: LibraryNickname,
    username: LibraryUsername,
    password: Option<LibraryPassword>,
    session_dir: Option<&Path>,
) -> Result<Library> {
    let config_json = ConfigJson::load(app_dir)?
        .ok_or_else(|| anyhow::anyhow!("no libraries configured; use 'lasco new' to create one"))?;

    let library_id = config_json
        .get_library_id_by_nickname(&library_nickname.0)
        .ok_or_else(|| anyhow::anyhow!("library '{}' not found", library_nickname.0))?;

    let local_dirs = LocalDirs::new(app_dir.to_path_buf(), library_id);

    if let Some(master_key) = session_load_master_key(*library_id, &username, session_dir)? {
        let library = Library::open_with_master_key(local_dirs, master_key, *library_id, username)
            .await
            .map_err(|e| anyhow::anyhow!("failed to open library: {}", e))?;
        library.load_local_state().await?;
        return Ok(library);
    }

    let password =
        password.ok_or_else(|| anyhow::anyhow!("password required: no cached session found"))?;

    let library = Library::open(
        local_dirs,
        crate::library::Credentials {
            username: username.clone(),
            password,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to open library: {}", e))?;

    session_store_master_key(*library_id, &username, library.master_key(), session_dir)?;

    library.load_local_state().await?;

    Ok(library)
}

pub async fn create_library(
    app_dir: &Path,
    nickname: String,
    username: LibraryUsername,
    password: LibraryPassword,
    session_dir: Option<&Path>,
) -> Result<(LibraryId, MasterKey)> {
    let library_id = LibraryId::new();

    let local_dirs = LocalDirs::new(app_dir.to_path_buf(), &library_id);
    local_dirs
        .ensure_state_dirs()
        .context("failed to create local state directories")?;

    let (lib, password_uuid) = Library::init(
        local_dirs,
        library_id,
        crate::library::Credentials {
            username: username.clone(),
            password,
        },
    )
    .await
    .context("failed to initialise library")?;

    let library_config = LibraryJson {
        version: crate::library_json::LIBRARY_JSON_VERSION,
        nickname: LibraryNickname(nickname),
        default_fetch_remote: None,
        default_username: Some(username.clone()),
        active_password_uuid: Some(password_uuid),
        remotes: vec![],
        auto_import_device_media: false,
    };

    save_library(app_dir, &library_id, &library_config)?;

    let master_key = lib.master_key().clone();
    session_store_master_key(library_id, &username, &master_key, session_dir)
        .context("failed to store session key")?;

    Ok((library_id, master_key))
}

/// Add a library that already exists on an S3 remote.
///
/// Connects to the remote with the given plaintext credentials, downloads the
/// `library/` crypto dir (salt, sentinel, per user master key files) into a fresh
/// local library, derives the master key from an existing user's credentials, then
/// fetches the operations and rebuilds local state.
///
/// `new_user` optionally registers an additional user (same master key, encrypted
/// under a new password) and makes that user the effective one for this device.
#[allow(clippy::too_many_arguments)]
pub async fn add_existing_library_s3(
    app_dir: &Path,
    nickname: String,
    username: LibraryUsername,
    password: LibraryPassword,
    new_user: Option<(LibraryUsername, LibraryPassword)>,
    remote_id: String,
    endpoint: String,
    bucket: String,
    region: String,
    path_prefix: Option<String>,
    access_key: String,
    secret_key: String,
    session_dir: Option<&Path>,
) -> Result<(LibraryId, Library)> {
    let storage = StorageS3::new(
        endpoint.clone(),
        bucket.clone(),
        region.clone(),
        path_prefix.clone(),
        access_key.clone(),
        secret_key.clone(),
    )?;

    // Listing the crypto dir doubles as a connectivity and credential check.
    let remote_library_dir = storage
        .list("library/")
        .await
        .map_err(|e| anyhow::anyhow!("remote unreachable: {e}"))?;
    if remote_library_dir.is_empty() {
        anyhow::bail!("no library found at this remote");
    }

    // Parse library UUID from the library_id_{uuid} filename.
    let remote_library_uuid = remote_library_dir
        .iter()
        .find_map(|k| {
            let name = k.rsplit('/').next().unwrap_or(k);
            name.strip_prefix("library_id_")
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
        })
        .ok_or_else(|| anyhow::anyhow!("remote is missing library_id_{{uuid}} file"))?;
    let library_id = LibraryId(remote_library_uuid);

    let local_dirs = LocalDirs::new(app_dir.to_path_buf(), &library_id);
    local_dirs
        .ensure_state_dirs()
        .context("failed to create local state directories")?;
    local_dirs
        .ensure_sync_dirs()
        .context("failed to create local sync directories")?;

    // Download all crypto files into the local library dir.
    let lib_dir = local_dirs.local_library_dir();
    for key in &remote_library_dir {
        let basename = key.rsplit('/').next().unwrap_or(key);
        if basename.is_empty() {
            continue;
        }
        let data = storage
            .get(key)
            .await
            .map_err(|e| anyhow::anyhow!("failed to download {key}: {e}"))?;
        std::fs::write(lib_dir.join(basename), &data)
            .with_context(|| format!("failed to write crypto file {basename}"))?;
    }

    // Discover the active password UUID by trying all mk files for this user.
    let (master_key, active_password_uuid) = find_master_key(&lib_dir, &username.0, &password.0)
        .map_err(|_| anyhow::anyhow!("failed to open library — wrong username or password"))?;

    let library = Library::open_with_master_key(
        local_dirs.clone(),
        master_key.clone(),
        library_id,
        username.clone(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to open library: {}", e))?;

    // Optionally register and switch to a new user on this device.
    let (effective_username, active_password_uuid, library) = match new_user {
        Some((new_username, new_password)) => {
            let new_uuid = library
                .user_add(new_username.clone(), new_password)
                .await
                .map_err(|e| anyhow::anyhow!("failed to add user: {}", e))?;

            // Propagate the new user's master-key file to the remote so other
            // devices can authenticate as them.
            let mk_name = format!("mk_{}_{}.enc", new_username.0, new_uuid);
            let mk_bytes = std::fs::read(lib_dir.join(&mk_name))
                .with_context(|| format!("failed to read {mk_name}"))?;
            storage
                .put(&format!("library/{mk_name}"), &mk_bytes)
                .await
                .map_err(|e| anyhow::anyhow!("failed to upload new user key: {e}"))?;

            let library = Library::open_with_master_key(
                local_dirs,
                master_key.clone(),
                library_id,
                new_username.clone(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("failed to reopen library as new user: {}", e))?;
            (new_username, new_uuid, library)
        }
        None => (username, active_password_uuid, library),
    };

    // Encrypt the secret key with the master key and persist the remote config.
    let (secret_key_encrypted, secret_key_encryption_description) =
        encrypt_s3_secret_key(&master_key, &secret_key)
            .map_err(|e| anyhow::anyhow!("failed to encrypt S3 secret key: {e}"))?;

    let remote = crate::library::sync::remote_access::StorageRead::new(&storage);
    let remote_uuid = crate::library::sync::discover_remote_uuid(&remote)
        .await
        .map_err(|e| anyhow::anyhow!("failed to read remote id: {e}"))?;
    let remote_config = RemoteConfig {
        remote_uuid,
        name: remote_id.clone(),
        auto_push: true,
        kind: RemoteKind::S3(S3Config {
            endpoint,
            bucket,
            region,
            path_prefix,
            access_key,
            secret_key_encrypted,
            secret_key_encryption_description,
        }),
    };

    let library_config = LibraryJson {
        version: crate::library_json::LIBRARY_JSON_VERSION,
        nickname: LibraryNickname(nickname),
        default_username: Some(effective_username.clone()),
        active_password_uuid: Some(active_password_uuid),
        default_fetch_remote: Some(remote_uuid),
        auto_import_device_media: false,
        remotes: vec![remote_config],
    };

    save_library(app_dir, &library_id, &library_config)?;

    session_store_master_key(library_id, &effective_username, &master_key, session_dir)
        .context("failed to store session key")?;

    // Download operations from the remote and rebuild local state.
    library
        .fetch(&storage, &remote_uuid.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch from remote: {}", e))?;
    library
        .load_local_state()
        .await
        .map_err(|e| anyhow::anyhow!("failed to load local state: {}", e))?;

    Ok((library_id, library))
}

/// Delete a library by removing its local data directories, clearing its file-based
/// session keys, and removing its entry from the app config. Idempotent with respect to
/// missing files and dirs. Does not touch any configured remote storage and does
/// not clear OS keychain entries.
pub fn delete_library(
    app_dir: &Path,
    library_id: &LibraryId,
    session_dir: Option<&Path>,
) -> Result<()> {
    let lib_dir = library_data_dir(app_dir, library_id);
    if lib_dir.exists() {
        std::fs::remove_dir_all(&lib_dir)
            .with_context(|| format!("failed to remove {}", lib_dir.display()))?;
    }

    let local_storage = local_storage_path(app_dir, library_id);
    if local_storage.exists() {
        std::fs::remove_dir_all(&local_storage)
            .with_context(|| format!("failed to remove {}", local_storage.display()))?;
    }

    if let Some(dir) = session_dir {
        let session_lib_dir = dir.join(library_id.to_string());
        if session_lib_dir.exists() {
            std::fs::remove_dir_all(&session_lib_dir).ok();
        }
    }

    if let Some(mut config) = ConfigJson::load(app_dir)? {
        if config.libraries.contains_key(library_id) {
            config.remove_library(library_id)?;
            config.save(app_dir)?;
        }
    }

    Ok(())
}
