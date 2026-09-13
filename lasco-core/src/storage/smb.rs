use std::fmt;
use std::io;
use std::time::Duration;

use async_trait::async_trait;
use smb2::msg::create::{
    CreateDisposition, CreateRequest, CreateResponse, ImpersonationLevel, ShareAccess,
};
use smb2::msg::query_info::InfoType;
use smb2::msg::set_info::SetInfoRequest;
use smb2::pack::{ReadCursor, Unpack};
use smb2::types::flags::FileAccessMask;
use smb2::types::status::NtStatus;
use smb2::types::{Command, OplockLevel};
use smb2::{ClientConfig, Error as SmbError, ErrorKind, SmbClient, Tree};
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use super::{AtomicWriteMode, Result, Storage, StorageError};

const SMB_TIMEOUT: Duration = Duration::from_secs(15);
const FILE_RENAME_INFORMATION: u8 = 10;

/// Validated SMB connection details. Its `Debug` implementation deliberately
/// excludes the password so callers can safely include the value in diagnostics.
#[derive(Clone)]
pub struct SmbConnectionConfig {
    pub server: String,
    pub port: u16,
    pub share: String,
    pub path_prefix: Option<String>,
    pub username: String,
    pub password: String,
    pub domain: Option<String>,
}

impl fmt::Debug for SmbConnectionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SmbConnectionConfig")
            .field("server", &self.server)
            .field("port", &self.port)
            .field("share", &self.share)
            .field("path_prefix", &self.path_prefix)
            .field("username", &self.username)
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}

impl SmbConnectionConfig {
    /// Validate and normalize connection fields collected from a user or a persisted remote.
    ///
    /// # Errors
    ///
    /// Returns an error if a host, share, credential, port, or relative path is invalid.
    #[allow(
        clippy::too_many_arguments,
        reason = "SMB connection settings are explicit user inputs."
    )]
    pub fn new(
        server: &str,
        port: u16,
        share: &str,
        path_prefix: Option<&str>,
        username: &str,
        password: &str,
        domain: Option<&str>,
    ) -> Result<Self> {
        let mut server = server.trim().to_string();
        if server.starts_with('[') && server.ends_with(']') {
            server = server[1..server.len() - 1].to_string();
        }
        if server.is_empty()
            || server.contains("://")
            || server.contains('/')
            || server.contains('\\')
            || server.contains('@')
        {
            return Err(invalid_input(
                "SMB server must be a hostname or IP address without a scheme or port",
            ));
        }
        if server.contains(':') && server.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(invalid_input("SMB server must not include a port"));
        }
        if port == 0 {
            return Err(invalid_input("SMB port must be between 1 and 65535"));
        }

        let share = share.trim().to_string();
        if share.is_empty() || share.contains('/') || share.contains('\\') {
            return Err(invalid_input("SMB share must be one non-empty share name"));
        }

        let username = username.trim().to_string();
        if username.is_empty() {
            return Err(invalid_input("SMB username must not be empty"));
        }
        if password.is_empty() {
            return Err(invalid_input("SMB password must not be empty"));
        }

        let path_prefix = normalize_path_prefix(path_prefix)?;
        let domain = domain
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        Ok(Self {
            server,
            port,
            share,
            path_prefix,
            username,
            password: password.to_string(),
            domain,
        })
    }

    fn address(&self) -> String {
        if self.server.contains(':') {
            format!("[{}]:{}", self.server, self.port)
        } else {
            format!("{}:{}", self.server, self.port)
        }
    }
}

/// SMB 2/3 storage backend. One lazily connected session is serialized to keep
/// the high-level `smb2` client and share handle coherent across sync operations.
pub struct StorageSmb {
    config: SmbConnectionConfig,
    session: Mutex<Option<SmbSession>>,
}

impl fmt::Debug for StorageSmb {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StorageSmb")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

struct SmbSession {
    client: SmbClient,
    tree: Tree,
}

impl StorageSmb {
    #[must_use]
    pub fn new(config: SmbConnectionConfig) -> Self {
        Self {
            config,
            session: Mutex::new(None),
        }
    }

    /// Verify that the remote prefix is writable using a short-lived probe file.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot authenticate, create, read, or delete the probe.
    pub async fn test_connection(&self) -> Result<()> {
        let probe_key = format!(".lasco-connection-test-{}", Uuid::new_v4());
        self.put_atomic(&probe_key, b"lasco-smb-probe", AtomicWriteMode::Replace)
            .await?;
        let read_back = self.get(&probe_key).await;
        let cleanup = self.delete(&probe_key).await;
        let read_back = read_back?;
        cleanup?;
        if read_back != b"lasco-smb-probe" {
            return Err(StorageError::Unavailable(
                "SMB server returned different data for the connection test".to_string(),
            ));
        }
        Ok(())
    }

    async fn session(&self) -> Result<MutexGuard<'_, Option<SmbSession>>> {
        let mut session = self.session.lock().await;
        if session.is_none() {
            let client_config = ClientConfig {
                addr: self.config.address(),
                timeout: SMB_TIMEOUT,
                username: self.config.username.clone(),
                password: self.config.password.clone(),
                domain: self.config.domain.clone().unwrap_or_default(),
                auto_reconnect: true,
                compression: true,
                dfs_enabled: false,
                dfs_target_overrides: Default::default(),
            };
            let mut client = SmbClient::connect(client_config)
                .await
                .map_err(map_smb_error)?;
            let tree = client
                .connect_share(&self.config.share)
                .await
                .map_err(map_smb_error)?;
            *session = Some(SmbSession { client, tree });
        }
        Ok(session)
    }

    async fn ensure_parent_directories(&self, path: &str) -> Result<()> {
        let Some((parent, _)) = path.rsplit_once('/') else {
            return Ok(());
        };
        let mut guard = self.session().await?;
        let mut current = String::new();
        for component in parent.split('/') {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(component);
            let result = {
                let session = guard.as_mut().expect("session initialized before use");
                session
                    .client
                    .create_directory(&mut session.tree, &current)
                    .await
            };
            match result {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => {
                    let clear = should_reset_session(&error);
                    let mapped = map_smb_error(error);
                    if clear {
                        *guard = None;
                    }
                    return Err(mapped);
                }
            }
        }
        Ok(())
    }

    fn remote_path(&self, key: &str) -> Result<String> {
        let key = normalize_key(key)?;
        Ok(match &self.config.path_prefix {
            Some(prefix) if key.is_empty() => prefix.clone(),
            Some(prefix) => format!("{prefix}/{key}"),
            None => key,
        })
    }

    fn logical_key(&self, prefix: &str, name: &str) -> Result<String> {
        let prefix = normalize_key(prefix)?;
        if prefix.is_empty() {
            Ok(name.to_string())
        } else {
            Ok(format!("{prefix}/{name}"))
        }
    }

    async fn delete_remote_path(&self, path: &str) -> Result<()> {
        let mut guard = self.session().await?;
        let result = {
            let session = guard.as_mut().expect("session initialized before use");
            session.client.delete_file(&mut session.tree, path).await
        };
        match result {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => {
                let clear = should_reset_session(&error);
                let mapped = map_smb_error(error);
                if clear {
                    *guard = None;
                }
                Err(mapped)
            }
        }
    }
}

#[async_trait]
impl Storage for StorageSmb {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.put_atomic(key, data, AtomicWriteMode::Replace)
            .await
            .map(|_| ())
    }

    async fn put_atomic(&self, key: &str, data: &[u8], mode: AtomicWriteMode) -> Result<bool> {
        match mode {
            AtomicWriteMode::Replace => {
                let path = self.remote_path(key)?;
                self.ensure_parent_directories(&path).await?;
                let (parent, name) = match path.rsplit_once('/') {
                    Some((parent, name)) => (format!("{parent}/"), name),
                    None => (String::new(), path.as_str()),
                };
                let temporary = format!("{parent}.lasco-{name}-{}.tmp", Uuid::new_v4());

                let write_result = {
                    let mut session = self.session().await?;
                    let session = session.as_mut().expect("session initialized before use");
                    session
                        .client
                        .write_file_pipelined(&mut session.tree, &temporary, data)
                        .await
                };
                if let Err(error) = write_result {
                    let clear = should_reset_session(&error);
                    if clear {
                        *self.session.lock().await = None;
                    }
                    let _ = self.delete_remote_path(&temporary).await;
                    return Err(map_smb_error(error));
                }

                let rename_result = {
                    let mut session = self.session().await?;
                    let session = session.as_mut().expect("session initialized before use");
                    rename_replace(session, &temporary, &path).await
                };
                if let Err(error) = rename_result {
                    let clear = should_reset_session(&error);
                    if clear {
                        *self.session.lock().await = None;
                    }
                    let _ = self.delete_remote_path(&temporary).await;
                    return Err(map_smb_error(error));
                }
                Ok(true)
            }
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.remote_path(key)?;
        let mut guard = self.session().await?;
        let result = {
            let session = guard.as_mut().expect("session initialized before use");
            session
                .client
                .read_file_pipelined(&mut session.tree, &path)
                .await
        };
        match result {
            Ok(data) => Ok(data),
            Err(error) => {
                let clear = should_reset_session(&error);
                let mapped = map_smb_error(error);
                if clear {
                    *guard = None;
                }
                Err(mapped)
            }
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.remote_path(key)?;
        self.delete_remote_path(&path).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let path = self.remote_path(prefix)?;
        let mut guard = self.session().await?;
        let result = {
            let session = guard.as_mut().expect("session initialized before use");
            session
                .client
                .list_directory(&mut session.tree, &path)
                .await
        };
        match result {
            Ok(entries) => entries
                .into_iter()
                .filter(|entry| !entry.is_directory && entry.name != "." && entry.name != "..")
                .map(|entry| self.logical_key(prefix, &entry.name))
                .collect(),
            Err(error) => {
                let clear = should_reset_session(&error);
                let mapped = map_smb_error(error);
                if clear {
                    *guard = None;
                }
                Err(mapped)
            }
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.remote_path(key)?;
        let mut guard = self.session().await?;
        let result = {
            let session = guard.as_mut().expect("session initialized before use");
            session.client.stat(&mut session.tree, &path).await
        };
        match result {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => {
                let clear = should_reset_session(&error);
                let mapped = map_smb_error(error);
                if clear {
                    *guard = None;
                }
                Err(mapped)
            }
        }
    }
}

/// Rename a file with SMB's `ReplaceIfExists` flag. `smb2` 0.21's public
/// convenience rename deliberately uses `false`, so this advanced operation is
/// necessary to honor Lasco's atomic replacement contract.
async fn rename_replace(
    session: &mut SmbSession,
    from: &str,
    to: &str,
) -> std::result::Result<(), SmbError> {
    if session.tree.is_dfs {
        return Err(SmbError::InvalidData {
            message: "SMB DFS shares are not supported for Lasco remotes".to_string(),
        });
    }
    let from = smb2::encode_path(from);
    let to = smb2::encode_path(to);
    let tree_id = session.tree.tree_id;
    let create = CreateRequest {
        requested_oplock_level: OplockLevel::None,
        impersonation_level: ImpersonationLevel::Impersonation,
        desired_access: FileAccessMask::new(
            FileAccessMask::DELETE | FileAccessMask::FILE_READ_ATTRIBUTES,
        ),
        file_attributes: 0,
        share_access: ShareAccess(
            ShareAccess::FILE_SHARE_READ
                | ShareAccess::FILE_SHARE_WRITE
                | ShareAccess::FILE_SHARE_DELETE,
        ),
        create_disposition: CreateDisposition::FileOpen,
        create_options: 0,
        name: from,
        create_contexts: vec![],
    };
    let frame = session
        .client
        .connection_mut()
        .execute(Command::Create, &create, Some(tree_id))
        .await?;
    if frame.header.status != NtStatus::SUCCESS {
        return Err(SmbError::Protocol {
            status: frame.header.status,
            command: Command::Create,
        });
    }
    let mut cursor = ReadCursor::new(&frame.body);
    let create_response = CreateResponse::unpack(&mut cursor)?;
    let file_id = create_response.file_id;

    let rename = SetInfoRequest {
        info_type: InfoType::File,
        file_info_class: FILE_RENAME_INFORMATION,
        additional_information: 0,
        file_id,
        buffer: rename_information(&to),
    };
    let rename_result = session
        .client
        .connection_mut()
        .execute(Command::SetInfo, &rename, Some(tree_id))
        .await;
    let close_result = session
        .tree
        .close_handle(session.client.connection_mut(), file_id)
        .await;

    let frame = rename_result?;
    if frame.header.status != NtStatus::SUCCESS {
        return Err(SmbError::Protocol {
            status: frame.header.status,
            command: Command::SetInfo,
        });
    }
    close_result?;
    Ok(())
}

fn rename_information(destination: &str) -> Vec<u8> {
    let destination: Vec<u16> = destination.encode_utf16().collect();
    let mut buffer = Vec::with_capacity(20 + destination.len() * 2);
    buffer.push(1); // ReplaceIfExists = true.
    buffer.extend_from_slice(&[0; 7]);
    buffer.extend_from_slice(&0_u64.to_le_bytes());
    buffer.extend_from_slice(
        &u32::try_from(destination.len() * 2)
            .expect("SMB path length fits u32")
            .to_le_bytes(),
    );
    for unit in destination {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }
    buffer
}

fn normalize_path_prefix(prefix: Option<&str>) -> Result<Option<String>> {
    prefix
        .map(normalize_key)
        .transpose()
        .map(|value| value.filter(|value| !value.is_empty()))
}

fn normalize_key(value: &str) -> Result<String> {
    let value = value.trim().replace('\\', "/");
    let components: Vec<&str> = value
        .trim_matches('/')
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    if components
        .iter()
        .any(|component| *component == "." || *component == "..")
    {
        return Err(invalid_input("SMB paths must not contain '.' or '..'"));
    }
    Ok(components.join("/"))
}

fn invalid_input(message: &str) -> StorageError {
    StorageError::Other(Box::new(io::Error::new(
        io::ErrorKind::InvalidInput,
        message,
    )))
}

fn should_reset_session(error: &SmbError) -> bool {
    matches!(
        error.kind(),
        ErrorKind::ConnectionLost | ErrorKind::TimedOut | ErrorKind::SessionExpired
    )
}

fn map_smb_error(error: SmbError) -> StorageError {
    match error.kind() {
        ErrorKind::NotFound => StorageError::NotFound,
        ErrorKind::AuthRequired | ErrorKind::SigningRequired => {
            StorageError::Unavailable("SMB authentication failed".to_string())
        }
        ErrorKind::AccessDenied => StorageError::Unavailable("SMB access was denied".to_string()),
        ErrorKind::DiskFull => StorageError::Unavailable("SMB share is full".to_string()),
        ErrorKind::ConnectionLost | ErrorKind::TimedOut | ErrorKind::SessionExpired => {
            StorageError::Unavailable("SMB server is unavailable".to_string())
        }
        // Do not surface protocol diagnostics through the application boundary:
        // they can include remote paths and server-specific details.
        _ => StorageError::Unavailable("SMB request failed".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_prefix_and_ipv6() {
        let config = SmbConnectionConfig::new(
            "[::1]",
            445,
            "photos",
            Some(" /lasco\\photos/ "),
            "alice",
            "password",
            Some(" WORKGROUP "),
        )
        .unwrap();
        assert_eq!(config.server, "::1");
        assert_eq!(config.address(), "[::1]:445");
        assert_eq!(config.path_prefix.as_deref(), Some("lasco/photos"));
        assert_eq!(config.domain.as_deref(), Some("WORKGROUP"));
    }

    #[test]
    fn rejects_server_urls_and_parent_paths() {
        assert!(
            SmbConnectionConfig::new("smb://nas", 445, "share", None, "user", "pass", None)
                .is_err()
        );
        assert!(
            SmbConnectionConfig::new("nas", 445, "share", Some("../escape"), "user", "pass", None)
                .is_err()
        );
    }

    #[test]
    fn replacement_buffer_sets_replace_if_exists() {
        let buffer = rename_information("destination");
        assert_eq!(buffer[0], 1);
    }
}
