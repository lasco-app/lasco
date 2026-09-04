use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Result, bail};

use anyhow::Context;

use crate::config_json::{ConfigJson, LibraryNickname, library_data_dir};
use crate::encryption::master_key::{MasterKey, find_master_key};
use crate::identifiers::LibraryId;
use crate::library::Library;
use crate::library::local_dirs::LocalDirs;
use crate::library_json::{
    CloudS3Config, LibraryJson, RemoteConfig, RemoteKind, S3Config, find_library_id_by_nickname,
    save_library,
};
use crate::operations::{LibraryPassword, LibraryUsername};
use crate::s3_secret::{encrypt_s3_secret_key, resolve_s3_credentials};
use crate::session::{session_load_master_key, session_store_master_key};
use crate::storage::{Storage, StorageLocalFs, StorageS3};

/// Constructs the storage backend for an already-selected remote configuration.
///
/// This factory does not select a remote, modify library configuration, initialize remote
/// storage, or perform synchronization. Callers choose and validate the [`RemoteConfig`] first.
/// `app_dir` is used by the debug Android backend, while `app_support_dir` is required by the
/// debug Apple backend.
///
/// # Errors
///
/// Returns an error for an unsupported platform, missing required directory or master key,
/// invalid encrypted S3 credentials, or failure to initialize the selected storage backend.
pub fn build_storage(
    app_dir: &Path,
    remote: &RemoteConfig,
    master_key: Option<&MasterKey>,
    app_support_dir: Option<&Path>,
) -> Result<Box<dyn Storage + Send + Sync>> {
    match &remote.kind {
        RemoteKind::FixedPath(cfg) => Ok(Box::new(StorageLocalFs::new(&cfg.root_dir))),
        RemoteKind::UsbAndroid(cfg) => build_usb_android_storage(&cfg.tree_uri),
        RemoteKind::UsbApple(cfg) => build_usb_apple_storage(&cfg.bookmark_base64),
        RemoteKind::DebugLocalApple(cfg) => {
            let app_support_dir = app_support_dir.ok_or_else(|| {
                anyhow::anyhow!("app_support_dir required for debug_local_apple remote")
            })?;
            let path = app_support_dir
                .join("lasco")
                .join("local_fs_test")
                .join(&cfg.local_dir_name);
            Ok(Box::new(StorageLocalFs::new(&path)))
        }
        RemoteKind::DebugLocalAndroid(cfg) => {
            let path = app_dir.join("local_fs_test").join(&cfg.local_dir_name);
            Ok(Box::new(StorageLocalFs::new(&path)))
        }
        RemoteKind::S3(s3_cfg) => {
            let master_key = master_key
                .ok_or_else(|| anyhow::anyhow!("master key required to decrypt S3 credentials"))?;
            let (access_key, secret_key) = resolve_s3_credentials(s3_cfg, master_key)
                .map_err(|e| anyhow::anyhow!("failed to resolve S3 credentials: {e}"))?;
            Ok(Box::new(StorageS3::new(
                &s3_cfg.endpoint,
                &s3_cfg.bucket,
                &s3_cfg.region,
                s3_cfg.path_prefix.as_deref(),
                &access_key,
                &secret_key,
            )?))
        }
        RemoteKind::CloudS3(_) => bail!("Lasco Cloud credentials must be supplied at runtime"),
    }
}

#[cfg(target_os = "android")]
fn build_usb_android_storage(tree_uri: &str) -> Result<Box<dyn Storage + Send + Sync>> {
    Ok(Box::new(crate::storage::StorageUsbAndroid::new(tree_uri)?))
}

#[cfg(not(target_os = "android"))]
fn build_usb_android_storage(_tree_uri: &str) -> Result<Box<dyn Storage + Send + Sync>> {
    bail!("usb_android remotes are supported only on Android")
}

#[cfg(target_vendor = "apple")]
fn build_usb_apple_storage(bookmark_base64: &str) -> Result<Box<dyn Storage + Send + Sync>> {
    Ok(Box::new(crate::storage::StorageUsbApple::new(
        bookmark_base64,
    )?))
}

#[cfg(not(target_vendor = "apple"))]
fn build_usb_apple_storage(_bookmark_base64: &str) -> Result<Box<dyn Storage + Send + Sync>> {
    bail!("usb_apple remotes are supported only on Apple platforms")
}

/// Open a library by nickname with optional session-key caching.
///
/// If a cached master key exists in the session, `password` is not needed and may be `None`.
/// If no cached key exists and `password` is `None`, returns an error.
///
/// # Errors
///
/// Returns an error if configuration, session, credentials, crypto files, or local state cannot
/// be read; if the nickname is unknown; or if authentication fails without a cached key.
pub async fn open_library(
    app_dir: &Path,
    library_nickname: LibraryNickname,
    username: LibraryUsername,
    password: Option<LibraryPassword>,
    session_dir: Option<&Path>,
) -> Result<Library> {
    let library_id = find_library_id_by_nickname(app_dir, &library_nickname.0)
        .map_err(|_| anyhow::anyhow!("library '{}' not found", library_nickname.0))?;

    let local_dirs = LocalDirs::new(app_dir, &library_id);
    let library_config = LibraryJson::load(app_dir, &library_id)?
        .ok_or_else(|| anyhow::anyhow!("library.json not found for '{}'", library_nickname.0))?;
    let device_id = library_config.device_id;

    if let Some(master_key) = session_load_master_key(library_id, &username, session_dir)? {
        let library =
            Library::open_with_master_key(local_dirs, master_key, library_id, device_id, username)
                .context("failed to open library")?;
        return Ok(library);
    }

    let password =
        password.ok_or_else(|| anyhow::anyhow!("password required: no cached session found"))?;

    let library = Library::open(
        local_dirs,
        device_id,
        crate::library::Credentials {
            username: username.clone(),
            password,
        },
    )
    .context("failed to open library")?;

    session_store_master_key(library_id, &username, library.master_key(), session_dir)?;

    Ok(library)
}

/// # Errors
///
/// Returns an error if local state directories, crypto material, library configuration, or the
/// cached session key cannot be created or written.
#[allow(
    clippy::unused_async,
    reason = "Retains the public asynchronous client API used by FFI bindings."
)]
pub async fn create_library(
    app_dir: &Path,
    nickname: String,
    username: LibraryUsername,
    password: LibraryPassword,
    session_dir: Option<&Path>,
) -> Result<(LibraryId, MasterKey)> {
    let library_id = LibraryId::new();
    let device_id = crate::crdt::DeviceId::random();

    let local_dirs = LocalDirs::new(app_dir, &library_id);
    local_dirs
        .ensure_state_dirs()
        .context("failed to create local state directories")?;

    let (lib, password_uuid) = Library::init(
        local_dirs,
        library_id,
        device_id,
        crate::library::Credentials {
            username: username.clone(),
            password,
        },
    )
    .context("failed to initialise library")?;

    let library_config = LibraryJson {
        library_nickname: LibraryNickname(nickname),
        device_id,
        default_fetch_remote: None,
        default_username: Some(username.clone()),
        active_password_uuid: Some(password_uuid),
        remotes: vec![],
        media_source_order: vec![],
        auto_import_device_media: false,
    };

    save_library(app_dir, &library_id, &library_config)?;

    let master_key = lib.master_key().clone();
    session_store_master_key(library_id, &username, &master_key, session_dir)
        .context("failed to store session key")?;

    Ok((library_id, master_key))
}

/// Rebuild a library's materialized CRDT snapshot after an explicitly confirmed recovery.
/// Authentication is required because the operation log is encrypted with the master key.
pub async fn recover_library_state(
    app_dir: &Path,
    library_nickname: LibraryNickname,
    username: LibraryUsername,
    password: LibraryPassword,
) -> Result<()> {
    let library_id = find_library_id_by_nickname(app_dir, &library_nickname.0)?;
    let library_config = LibraryJson::load(app_dir, &library_id)?
        .ok_or_else(|| anyhow::anyhow!("library.json not found"))?;
    let local_dirs = LocalDirs::new(app_dir, &library_id);
    let master_key = find_master_key(
        local_dirs.local_state_library_dir().path(),
        &username.0,
        &password.0,
    )
    .map(|(key, _)| key)?;
    Library::recover_persisted_state(&local_dirs, &master_key, library_config.device_id)
        .context("failed to rebuild CRDT state")
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
///
/// # Errors
///
/// Returns an error for invalid S3 credentials or remote layout, download/upload failures,
/// failed authentication, local I/O, configuration persistence, or initial state synchronization.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "This public S3 bootstrap entry point keeps connection settings explicit and its ordered setup steps together."
)]
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
        &endpoint,
        &bucket,
        &region,
        path_prefix.as_deref(),
        &access_key,
        &secret_key,
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
    crate::library::sync::verify_remote_library_format_with_keys(&remote_library_dir)?;
    let library_id = LibraryId(remote_library_uuid);
    let device_id = crate::crdt::DeviceId::random();

    let local_dirs = LocalDirs::new(app_dir, &library_id);
    local_dirs
        .ensure_state_dirs()
        .context("failed to create local state directories")?;
    local_dirs
        .ensure_sync_dirs()
        .context("failed to create local sync directories")?;

    // Download all crypto files into the local library dir.
    let lib_dir = local_dirs.local_state_library_dir();
    for key in &remote_library_dir {
        let basename = key.rsplit('/').next().unwrap_or(key);
        if basename.is_empty() {
            continue;
        }
        let data = storage
            .get(key)
            .await
            .map_err(|e| anyhow::anyhow!("failed to download {key}: {e}"))?;
        std::fs::write(lib_dir.path().join(basename), &data)
            .with_context(|| format!("failed to write crypto file {basename}"))?;
    }

    // Discover the active password UUID by trying all mk files for this user.
    let (master_key, active_password_uuid) =
        find_master_key(lib_dir.path(), &username.0, &password.0).map_err(
            |_authentication_error| {
                anyhow::anyhow!("failed to open library — wrong username or password")
            },
        )?;

    let library = Library::open_with_master_key(
        local_dirs.clone(),
        master_key.clone(),
        library_id,
        device_id,
        username.clone(),
    )
    .map_err(|e| anyhow::anyhow!("failed to open library: {e}"))?;

    // Optionally register and switch to a new user on this device.
    let (effective_username, active_password_uuid, library) = match new_user {
        Some((new_username, new_password)) => {
            let new_uuid = library
                .user_add(new_username.clone(), new_password)
                .await
                .map_err(|e| anyhow::anyhow!("failed to add user: {e}"))?;

            // Propagate the new user's master-key file to the remote so other
            // devices can authenticate as them.
            let mk_name = format!("mk_{}_{}.enc", new_username.0, new_uuid);
            let mk_bytes = std::fs::read(lib_dir.path().join(&mk_name))
                .with_context(|| format!("failed to read {mk_name}"))?;
            let remote_key = format!("library/{mk_name}");
            if storage
                .exists(&remote_key)
                .await
                .map_err(|e| anyhow::anyhow!("failed to check new user key: {e}"))?
            {
                let remote_bytes = storage
                    .get(&remote_key)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to read existing new user key: {e}"))?;
                if remote_bytes != mk_bytes {
                    bail!("existing remote master-key file differs: {remote_key}");
                }
            } else {
                storage
                    .put_atomic(
                        &remote_key,
                        &mk_bytes,
                        crate::storage::AtomicWriteMode::Replace,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to upload new user key: {e}"))?;
            }

            let library = Library::open_with_master_key(
                local_dirs,
                master_key.clone(),
                library_id,
                device_id,
                new_username.clone(),
            )
            .map_err(|e| anyhow::anyhow!("failed to reopen library as new user: {e}"))?;
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
        media_fetch_priority: 0,
        exclude_from_media_fetch: false,
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
        library_nickname: LibraryNickname(nickname),
        device_id,
        default_username: Some(effective_username.clone()),
        active_password_uuid: Some(active_password_uuid),
        default_fetch_remote: Some(remote_uuid),
        auto_import_device_media: false,
        remotes: vec![remote_config],
        media_source_order: vec![remote_uuid],
    };

    save_library(app_dir, &library_id, &library_config)?;

    session_store_master_key(library_id, &effective_username, &master_key, session_dir)
        .context("failed to store session key")?;

    // Download operations from the remote and rebuild local state.
    library
        .fetch(&storage, remote_uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch from remote: {e}"))?;

    Ok((library_id, library))
}

/// Adds an existing library from the authenticated user's Lasco Cloud remotes.
///
/// The Cloud session used for discovery is deliberately ephemeral: only after
/// reading remote metadata do we know the library id that scopes persisted
/// Cloud authentication on this device.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn add_existing_library_lasco_cloud(
    app_dir: &Path,
    nickname: String,
    username: LibraryUsername,
    password: LibraryPassword,
    new_user: Option<(LibraryUsername, LibraryPassword)>,
    cloud_base_url: String,
    cloud_email: String,
    cloud_password: String,
    platform: String,
    app_version: String,
    session_dir: Option<&Path>,
) -> Result<(LibraryId, Library)> {
    let auth =
        crate::library::lasco_cloud_auth::LascoCloudAuthManager::new_ephemeral(cloud_base_url);
    auth.login(cloud_email, cloud_password, platform, app_version)
        .await?;
    let remote_infos = auth.list_remotes().await?;
    let associated_library_ids: HashSet<_> = remote_infos
        .iter()
        .filter_map(|remote| remote.library_id.clone())
        .collect();
    if associated_library_ids.is_empty() {
        bail!("no existing Lasco Cloud library was found for this account");
    }
    if associated_library_ids.len() != 1 {
        bail!("Lasco Cloud remotes are associated with different libraries");
    }
    let expected_library_uuid = associated_library_ids
        .into_iter()
        .next()
        .expect("non-empty set")
        .parse::<uuid::Uuid>()
        .context("Lasco Cloud returned an invalid library id")?;
    let expected_library_id = expected_library_uuid.to_string();
    let selected_remotes: Vec<_> = remote_infos
        .into_iter()
        .filter(|remote| remote.library_id.as_deref() == Some(expected_library_id.as_str()))
        .collect();
    if selected_remotes.is_empty() {
        bail!("no Lasco Cloud storage remotes are assigned to the existing library");
    }

    let credentials_by_id: HashMap<_, _> = auth
        .storage_credentials()
        .await?
        .into_iter()
        .map(|credential| (credential.id.clone(), credential))
        .collect();
    let mut cloud_remotes = Vec::with_capacity(selected_remotes.len());
    for remote in selected_remotes {
        let credentials = credentials_by_id.get(&remote.id).ok_or_else(|| {
            anyhow::anyhow!(
                "Lasco Cloud did not return credentials for remote '{}'",
                remote.id
            )
        })?;
        let storage = StorageS3::new_with_session_token(
            &remote.endpoint,
            &remote.bucket,
            &remote.region,
            (!remote.path_prefix.is_empty()).then_some(remote.path_prefix.as_str()),
            &credentials.access_key_id,
            &credentials.secret_access_key,
            credentials.session_token.as_deref(),
        )?;
        cloud_remotes.push((remote, storage));
    }
    let primary_storage = &cloud_remotes[0].1;
    let remote_library_dir = primary_storage
        .list("library/")
        .await
        .map_err(|e| anyhow::anyhow!("Lasco Cloud remote unreachable: {e}"))?;
    if remote_library_dir.is_empty() {
        bail!("no library found at this Lasco Cloud remote");
    }
    let remote_library_uuid = remote_library_dir
        .iter()
        .find_map(|key| {
            let name = key.rsplit('/').next().unwrap_or(key);
            name.strip_prefix("library_id_")
                .and_then(|value| value.parse::<uuid::Uuid>().ok())
        })
        .ok_or_else(|| anyhow::anyhow!("remote is missing library_id_{{uuid}} file"))?;
    if remote_library_uuid != expected_library_uuid {
        bail!("Lasco Cloud remote metadata does not match its assigned library");
    }
    crate::library::sync::verify_remote_library_format_with_keys(&remote_library_dir)?;
    let library_id = LibraryId(remote_library_uuid);
    let device_id = crate::crdt::DeviceId::random();
    let local_dirs = LocalDirs::new(app_dir, &library_id);
    local_dirs
        .ensure_state_dirs()
        .context("failed to create local state directories")?;
    local_dirs
        .ensure_sync_dirs()
        .context("failed to create local sync directories")?;
    let lib_dir = local_dirs.local_state_library_dir();
    for key in &remote_library_dir {
        let basename = key.rsplit('/').next().unwrap_or(key);
        if basename.is_empty() {
            continue;
        }
        let data = primary_storage
            .get(key)
            .await
            .map_err(|e| anyhow::anyhow!("failed to download {key}: {e}"))?;
        std::fs::write(lib_dir.path().join(basename), &data)
            .with_context(|| format!("failed to write crypto file {basename}"))?;
    }
    let (master_key, active_password_uuid) =
        find_master_key(lib_dir.path(), &username.0, &password.0)
            .map_err(|_| anyhow::anyhow!("failed to open library — wrong username or password"))?;
    let library = Library::open_with_master_key(
        local_dirs.clone(),
        master_key.clone(),
        library_id,
        device_id,
        username.clone(),
    )
    .map_err(|e| anyhow::anyhow!("failed to open library: {e}"))?;
    let (effective_username, active_password_uuid, library) =
        match new_user {
            Some((new_username, new_password)) => {
                let new_uuid = library
                    .user_add(new_username.clone(), new_password)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to add user: {e}"))?;
                let mk_name = format!("mk_{}_{}.enc", new_username.0, new_uuid);
                let mk_bytes = std::fs::read(lib_dir.path().join(&mk_name))
                    .with_context(|| format!("failed to read {mk_name}"))?;
                let remote_key = format!("library/{mk_name}");
                for (_, storage) in &cloud_remotes {
                    if storage
                        .exists(&remote_key)
                        .await
                        .map_err(|e| anyhow::anyhow!("failed to check new user key: {e}"))?
                    {
                        if storage.get(&remote_key).await.map_err(|e| {
                            anyhow::anyhow!("failed to read existing new user key: {e}")
                        })? != mk_bytes
                        {
                            bail!("existing remote master-key file differs: {remote_key}");
                        }
                    } else {
                        storage
                            .put_atomic(
                                &remote_key,
                                &mk_bytes,
                                crate::storage::AtomicWriteMode::Replace,
                            )
                            .await
                            .map_err(|e| anyhow::anyhow!("failed to upload new user key: {e}"))?;
                    }
                }
                let library = Library::open_with_master_key(
                    local_dirs,
                    master_key.clone(),
                    library_id,
                    device_id,
                    new_username.clone(),
                )
                .map_err(|e| anyhow::anyhow!("failed to reopen library as new user: {e}"))?;
                (new_username, new_uuid, library)
            }
            None => (username, active_password_uuid, library),
        };
    let mut remotes = Vec::with_capacity(cloud_remotes.len());
    for (priority, (remote_info, storage)) in cloud_remotes.iter().enumerate() {
        let remote = crate::library::sync::remote_access::StorageRead::new(storage);
        let remote_uuid = crate::library::sync::discover_remote_uuid(&remote)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read remote id: {e}"))?;
        remotes.push(RemoteConfig {
            remote_uuid,
            name: remote_info.name.clone(),
            auto_push: true,
            media_fetch_priority: u32::try_from(priority)
                .context("too many Lasco Cloud remotes")?,
            exclude_from_media_fetch: false,
            kind: RemoteKind::CloudS3(CloudS3Config {
                cloud_storage_id: remote_info.id.clone(),
            }),
        });
    }
    let primary_remote_uuid = remotes[0].remote_uuid;
    let media_source_order = remotes.iter().map(|remote| remote.remote_uuid).collect();
    save_library(
        app_dir,
        &library_id,
        &LibraryJson {
            library_nickname: LibraryNickname(nickname),
            device_id,
            default_username: Some(effective_username.clone()),
            active_password_uuid: Some(active_password_uuid),
            default_fetch_remote: Some(primary_remote_uuid),
            auto_import_device_media: false,
            remotes,
            media_source_order,
        },
    )?;
    session_store_master_key(library_id, &effective_username, &master_key, session_dir)
        .context("failed to store session key")?;
    library
        .fetch(primary_storage, primary_remote_uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch from Lasco Cloud: {e}"))?;
    Ok((library_id, library))
}

/// Delete a library by removing its local data directories, clearing its file-based
/// session keys, and removing its entry from the app config. Idempotent with respect to
/// missing files and dirs. Does not touch any configured remote storage and does
/// not clear OS keychain entries.
///
/// # Errors
///
/// Returns an error if local library data cannot be removed or the application configuration
/// cannot be read, updated, or saved.
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

    let local_storage = app_dir.join("local_storage").join(library_id.to_string());
    if local_storage.exists() {
        std::fs::remove_dir_all(&local_storage)
            .with_context(|| format!("failed to remove {}", local_storage.display()))?;
    }

    if let Some(dir) = session_dir {
        let session_lib_dir = dir.join(library_id.to_string());
        if session_lib_dir.exists() {
            let _ = std::fs::remove_dir_all(&session_lib_dir);
        }
    }

    if let Some(mut config) = ConfigJson::load(app_dir)?
        && config.libraries.contains(library_id)
    {
        config.remove_library(library_id)?;
        config.save(app_dir)?;
    }

    Ok(())
}
