use std::path::Path;

use keyring::Entry;

use crate::encryption::master_key::MASTER_KEY_SIZE;
use crate::encryption::master_key::MasterKey;
use crate::identifiers::LibraryId;
use crate::operations::LibraryUsername;

const KEYRING_SERVICE: &str = "lasco";

/// Keyring account key formatted as `{library_id}/{username}`.
fn user_mk_entry(library_id: LibraryId, username: &LibraryUsername) -> Result<Entry, SessionError> {
    let account = format!("{}/{}", library_id, username.0);
    Ok(Entry::new(KEYRING_SERVICE, &account)?)
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("keyring error: {0}")]
    Keychain(#[from] keyring::Error),
    #[error("stored master key has wrong length")]
    InvalidLength,
    #[error("session file I/O error: {0}")]
    Io(#[from] std::io::Error),
}

fn session_file(
    dir: &Path,
    library_id: LibraryId,
    username: &LibraryUsername,
) -> std::path::PathBuf {
    dir.join(library_id.to_string())
        .join(format!("{}.bin", username.0))
}

/// Store the `MasterKey` for a user.
/// Writes to a file when `session_dir` is `Some`, and to the OS keychain otherwise.
pub(crate) fn session_store_master_key(
    library_id: LibraryId,
    username: &LibraryUsername,
    master_key: &MasterKey,
    session_dir: Option<&Path>,
) -> Result<(), SessionError> {
    if let Some(dir) = session_dir {
        let path = session_file(dir, library_id, username);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, master_key.as_ref())?;
        return Ok(());
    }
    let hex = hex_encode(master_key.as_ref());
    user_mk_entry(library_id, username)?.set_password(&hex)?;
    Ok(())
}

/// Load the cached `MasterKey` for a user. Returns `None` if not present.
///
/// # Errors
///
/// Returns an error for keychain or session-file access failures, or malformed cached key material.
pub fn session_load_master_key(
    library_id: LibraryId,
    username: &LibraryUsername,
    session_dir: Option<&Path>,
) -> Result<Option<MasterKey>, SessionError> {
    if let Some(dir) = session_dir {
        let path = session_file(dir, library_id, username);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let arr: [u8; MASTER_KEY_SIZE] = bytes
                    .try_into()
                    .map_err(|_length_error| SessionError::InvalidLength)?;
                return Ok(Some(MasterKey::from_raw(arr)));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(SessionError::Io(e)),
        }
    }
    let e = user_mk_entry(library_id, username)?;
    match e.get_password() {
        Ok(hex) => {
            let bytes = hex_decode(&hex).map_err(|()| SessionError::InvalidLength)?;
            Ok(Some(MasterKey::from_raw(bytes)))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(SessionError::Keychain(err)),
    }
}

/// Clear the cached `MasterKey` for a library.
///
/// # Errors
///
/// Returns an error if the session directory or OS keychain cannot be accessed.
pub fn session_clear(
    library_id: LibraryId,
    username: &LibraryUsername,
    session_dir: Option<&Path>,
) -> Result<(), SessionError> {
    if let Some(dir) = session_dir {
        let lib_dir = dir.join(library_id.to_string());
        if lib_dir.exists() {
            let _ = std::fs::remove_dir_all(&lib_dir);
        }
        return Ok(());
    }
    let e = user_mk_entry(library_id, username)?;
    match e.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(SessionError::Keychain(err)),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Result<[u8; MASTER_KEY_SIZE], ()> {
    if s.len() != MASTER_KEY_SIZE * 2 {
        return Err(());
    }
    let mut out = [0u8; MASTER_KEY_SIZE];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8, ()> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::master_key::generate_master_key;

    fn test_username() -> LibraryUsername {
        LibraryUsername("alice".to_string())
    }

    #[test]
    fn store_load_clear_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let id = LibraryId::new();
        let username = test_username();
        let mk = generate_master_key();
        session_store_master_key(id, &username, &mk, Some(dir.path())).unwrap();
        let loaded = session_load_master_key(id, &username, Some(dir.path()))
            .unwrap()
            .expect("MasterKey present");
        assert_eq!(loaded.as_ref(), mk.as_ref());
        session_clear(id, &username, Some(dir.path())).unwrap();
        let after = session_load_master_key(id, &username, Some(dir.path())).unwrap();
        assert!(after.is_none());
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let id = LibraryId::new();
        let username = test_username();
        let result = session_load_master_key(id, &username, Some(dir.path())).unwrap();
        assert!(result.is_none());
    }
}
