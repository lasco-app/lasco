use std::fmt;
use std::path::{Path, PathBuf};

use aes_gcm::aead::generic_array::typenum::Unsigned;
use aes_gcm::{
    Aes256Gcm, Key as AesKey, KeyInit,
    aead::{Aead, AeadCore},
};
use chacha20poly1305::Key as XChaChaKey;
use rand::RngCore;
use rand::rngs::OsRng;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::encryption::error::KeychainError;
use crate::encryption::kek::{KEK_SIZE, derive_kek};
use crate::encryption::library_salt::{LibrarySalt, read_salt_file};
use crate::library::PROTOCOL_VERSION;

pub type Result<T> = std::result::Result<T, KeychainError>;

pub const MASTER_KEY_SIZE: usize = size_of::<XChaChaKey>();
const _: () = assert!(MASTER_KEY_SIZE == 32); // 256-bit root key

const AES_GCM_NONCE_SIZE: usize = <Aes256Gcm as AeadCore>::NonceSize::USIZE;
const _: () = assert!(AES_GCM_NONCE_SIZE == 12); // AES-GCM nonce is fixed at 96 bits by the spec

const VERSION_SIZE: usize = size_of::<u32>();
const FILE_HEADER_LEN: usize = VERSION_SIZE + AES_GCM_NONCE_SIZE;

/// Random 256-bit root key for a library, stored encrypted inside `library/mk_{username}_{uuid}.enc`.
///
/// Used to derive `BlobKeys` for each file via HKDF-SHA256. Never stored in plaintext on disk.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; MASTER_KEY_SIZE]);

impl MasterKey {
    pub fn from_raw(bytes: [u8; MASTER_KEY_SIZE]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MasterKey(...)")
    }
}

impl AsRef<[u8; MASTER_KEY_SIZE]> for MasterKey {
    fn as_ref(&self) -> &[u8; MASTER_KEY_SIZE] {
        &self.0
    }
}

/// Generate a fresh random `MasterKey`.
pub fn generate_master_key() -> MasterKey {
    let mut key_bytes = [0; MASTER_KEY_SIZE];
    OsRng.fill_bytes(&mut key_bytes);
    MasterKey::from_raw(key_bytes)
}

fn aes_gcm_encrypt(key_bytes: &[u8; KEK_SIZE], plaintext: &[u8]) -> Vec<u8> {
    let key = AesKey::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .expect("AES-256-GCM encryption failed");
    let mut out = Vec::with_capacity(FILE_HEADER_LEN + ciphertext.len());
    out.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    out.extend_from_slice(&nonce);
    out.extend(ciphertext);
    out
}

fn aes_gcm_decrypt(key_bytes: &[u8; KEK_SIZE], bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < FILE_HEADER_LEN {
        return Err(KeychainError::TooShort);
    }
    let version = u32::from_be_bytes(bytes[..VERSION_SIZE].try_into().unwrap());
    if version != PROTOCOL_VERSION {
        return Err(KeychainError::UnsupportedProtocolVersion(version));
    }
    let nonce_bytes = &bytes[VERSION_SIZE..FILE_HEADER_LEN];
    let ciphertext = &bytes[FILE_HEADER_LEN..];
    let key = AesKey::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = aes_gcm::Nonce::<_>::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| KeychainError::AuthenticationFailed)
}

fn serialize_mk(master_key: &MasterKey) -> [u8; MASTER_KEY_SIZE] {
    *master_key.as_ref()
}

fn deserialize_mk(bytes: &[u8]) -> Result<MasterKey> {
    if bytes.len() != MASTER_KEY_SIZE {
        return Err(KeychainError::InvalidLength {
            expected: MASTER_KEY_SIZE,
            got: bytes.len(),
        });
    }
    let mut arr = [0; MASTER_KEY_SIZE];
    arr.copy_from_slice(bytes);
    Ok(MasterKey(arr))
}

fn mk_path(lib_dir: &Path, username: &str, password_uuid: Uuid) -> PathBuf {
    lib_dir.join(format!("mk_{username}_{password_uuid}.enc"))
}

/// Length of a hyphenated UUID string: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
const UUID_STR_LEN: usize = 36;

/// Parse `mk_{username}_{uuid}.enc` filenames. Returns `(username, uuid)` on success.
pub fn parse_mk_filename(name: &str) -> Option<(String, Uuid)> {
    let rest = name.strip_prefix("mk_")?.strip_suffix(".enc")?;
    if rest.len() < UUID_STR_LEN + 2 {
        return None;
    }
    let (username_part, uuid_part) = rest.split_at(rest.len() - UUID_STR_LEN);
    let username = username_part.strip_suffix('_')?;
    let uuid = uuid_part.parse::<Uuid>().ok()?;
    Some((username.to_string(), uuid))
}

pub fn write_mk_file(
    lib_dir: &Path,
    username: &str,
    password_uuid: Uuid,
    master_key: &MasterKey,
    salt: LibrarySalt,
    password: &str,
) -> Result<()> {
    let kek = derive_kek(password, salt);
    let plaintext = serialize_mk(master_key);
    let encrypted = aes_gcm_encrypt(kek.as_ref(), &plaintext);
    std::fs::write(mk_path(lib_dir, username, password_uuid), &encrypted)
        .map_err(|e| KeychainError::Io(e.to_string()))
}

pub fn read_mk_file(
    lib_dir: &Path,
    username: &str,
    password_uuid: Uuid,
    salt: LibrarySalt,
    password: &str,
) -> Result<MasterKey> {
    let path = mk_path(lib_dir, username, password_uuid);
    let bytes = std::fs::read(&path).map_err(|_| {
        KeychainError::NotFound(format!("mk_{username}_{password_uuid}.enc not found"))
    })?;
    let kek = derive_kek(password, salt);
    let plaintext = aes_gcm_decrypt(kek.as_ref(), &bytes)?;
    deserialize_mk(&plaintext)
}

/// Try all `mk_{username}_*.enc` files in `lib_dir` until one decrypts successfully.
/// Returns the master key and the UUID of the file that matched.
pub fn find_master_key(
    lib_dir: &Path,
    username: &str,
    password: &str,
) -> Result<(MasterKey, Uuid)> {
    let salt = read_salt_file(lib_dir)?;
    let kek = derive_kek(password, salt);

    let entries = std::fs::read_dir(lib_dir).map_err(|e| KeychainError::Io(e.to_string()))?;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some((file_username, uuid)) = parse_mk_filename(&name) {
            if file_username != username {
                continue;
            }
            if let Ok(bytes) = std::fs::read(entry.path())
                && let Ok(plaintext) = aes_gcm_decrypt(kek.as_ref(), &bytes)
                && let Ok(mk) = deserialize_mk(&plaintext)
            {
                return Ok((mk, uuid));
            }
        }
    }

    Err(KeychainError::NotFound(format!(
        "no mk file found for user '{username}'"
    )))
}

/// Open with a known password UUID (fast path, avoids iterating all mk files).
pub fn open_master_key(
    lib_dir: &Path,
    username: &str,
    password_uuid: Uuid,
    password: &str,
) -> Result<MasterKey> {
    let salt = read_salt_file(lib_dir)?;
    read_mk_file(lib_dir, username, password_uuid, salt, password)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::encryption::library_salt::{generate_salt, write_salt_file};

    use super::*;

    fn _assert_send<T: Send>() {}
    const _: () = {
        let _ = _assert_send::<MasterKey>;
    };

    #[test]
    fn master_key_debug_does_not_leak_key_material() {
        let mk = MasterKey::from_raw([42; MASTER_KEY_SIZE]);
        assert_eq!(format!("{mk:?}"), "MasterKey(...)");
    }

    #[test]
    fn mk_file_round_trips() {
        let tmp = TempDir::new().unwrap();
        let lib_dir = tmp.path();
        let master_key = generate_master_key();
        let salt = generate_salt();
        let uuid = Uuid::new_v4();

        write_salt_file(lib_dir, salt).unwrap();
        write_mk_file(lib_dir, "alice", uuid, &master_key, salt, "password123").unwrap();

        let recovered = read_mk_file(lib_dir, "alice", uuid, salt, "password123").unwrap();
        assert_eq!(recovered.as_ref(), master_key.as_ref());
    }

    #[test]
    fn wrong_password_returns_auth_failed() {
        let tmp = TempDir::new().unwrap();
        let lib_dir = tmp.path();
        let master_key = generate_master_key();
        let salt = generate_salt();
        let uuid = Uuid::new_v4();

        write_salt_file(lib_dir, salt).unwrap();
        write_mk_file(lib_dir, "alice", uuid, &master_key, salt, "correct").unwrap();

        let result = read_mk_file(lib_dir, "alice", uuid, salt, "wrong");
        assert!(matches!(result, Err(KeychainError::AuthenticationFailed)));
    }

    #[test]
    fn find_master_key_discovers_correct_file() {
        let tmp = TempDir::new().unwrap();
        let lib_dir = tmp.path();
        let master_key = generate_master_key();
        let salt = generate_salt();
        let uuid = Uuid::new_v4();

        write_salt_file(lib_dir, salt).unwrap();
        write_mk_file(lib_dir, "alice", uuid, &master_key, salt, "pass").unwrap();

        let (recovered, found_uuid) = find_master_key(lib_dir, "alice", "pass").unwrap();
        assert_eq!(recovered.as_ref(), master_key.as_ref());
        assert_eq!(found_uuid, uuid);
    }

    #[test]
    fn find_master_key_wrong_password_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let lib_dir = tmp.path();
        let master_key = generate_master_key();
        let salt = generate_salt();
        let uuid = Uuid::new_v4();

        write_salt_file(lib_dir, salt).unwrap();
        write_mk_file(lib_dir, "alice", uuid, &master_key, salt, "correct").unwrap();

        let result = find_master_key(lib_dir, "alice", "wrong");
        assert!(matches!(result, Err(KeychainError::NotFound(_))));
    }

    #[test]
    fn parse_mk_filename_roundtrip() {
        let uuid = Uuid::new_v4();
        let name = format!("mk_alice_{uuid}.enc");
        let (username, parsed_uuid) = parse_mk_filename(&name).unwrap();
        assert_eq!(username, "alice");
        assert_eq!(parsed_uuid, uuid);
    }

    #[test]
    fn open_master_key_round_trips() {
        let tmp = TempDir::new().unwrap();
        let lib_dir = tmp.path();
        let master_key = generate_master_key();
        let salt = generate_salt();
        let uuid = Uuid::new_v4();

        write_salt_file(lib_dir, salt).unwrap();
        write_mk_file(lib_dir, "alice", uuid, &master_key, salt, "pass").unwrap();

        let recovered = open_master_key(lib_dir, "alice", uuid, "pass").unwrap();
        assert_eq!(recovered.as_ref(), master_key.as_ref());
    }
}
