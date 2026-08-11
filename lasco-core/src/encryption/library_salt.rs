use std::path::Path;

use rand::RngCore;
use rand::rngs::OsRng;

use crate::encryption::error::KeychainError;

pub type Result<T> = std::result::Result<T, KeychainError>;

/// Salt size in bytes. Feeded into argon2id by kek.rs
pub const LIBRARY_SALT_SIZE: usize = 32;

const LIBRARY_SALT_FILE: &str = "library_salt";

/// Per-library random 256-bit value stored as `library_salt`.
#[derive(Debug, Clone, Copy)]
pub struct LibrarySalt(pub [u8; LIBRARY_SALT_SIZE]);

pub fn generate_salt() -> LibrarySalt {
    let mut bytes = [0; LIBRARY_SALT_SIZE];
    OsRng.fill_bytes(&mut bytes);
    LibrarySalt(bytes)
}

pub(crate) fn write_salt_file(lib_dir: &Path, salt: LibrarySalt) -> Result<()> {
    std::fs::write(lib_dir.join(LIBRARY_SALT_FILE), salt.0)
        .map_err(|e| KeychainError::Io(e.to_string()))
}

pub(crate) fn read_salt_file(lib_dir: &Path) -> Result<LibrarySalt> {
    let bytes = std::fs::read(lib_dir.join(LIBRARY_SALT_FILE))
        .map_err(|_| KeychainError::NotFound("library_salt not found".to_string()))?;
    if bytes.len() != LIBRARY_SALT_SIZE {
        return Err(KeychainError::InvalidLength {
            expected: LIBRARY_SALT_SIZE,
            got: bytes.len(),
        });
    }
    let mut arr = [0; LIBRARY_SALT_SIZE];
    arr.copy_from_slice(&bytes);
    Ok(LibrarySalt(arr))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn salt_file_round_trips() {
        let tmp = TempDir::new().unwrap();
        let lib_dir = tmp.path();
        let salt = generate_salt();
        write_salt_file(lib_dir, salt).unwrap();
        let recovered = read_salt_file(lib_dir).unwrap();
        assert_eq!(recovered.0, salt.0);
    }
}
