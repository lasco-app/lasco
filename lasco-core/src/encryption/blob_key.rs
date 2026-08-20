use std::fmt;

use chacha20poly1305::Key as XChaChaKey;
use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::master_key::MasterKey;

const BLOB_KEY_SIZE: usize = size_of::<XChaChaKey>();
const _: () = assert!(BLOB_KEY_SIZE == 32); // XChaCha20-Poly1305 key is fixed at 256 bits by the spec

/// Per-file encryption key derived from the `MasterKey` and the file's UUID via HKDF-SHA256.
///
/// Encrypts and decrypts individual file blobs with XChaCha20-Poly1305.
/// Never stored on disk. Rederived on every access.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct BlobKey([u8; BLOB_KEY_SIZE]);

impl BlobKey {
    #[must_use]
    pub fn from_raw(bytes: [u8; BLOB_KEY_SIZE]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for BlobKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobKey(...)")
    }
}

impl AsRef<[u8; BLOB_KEY_SIZE]> for BlobKey {
    fn as_ref(&self) -> &[u8; BLOB_KEY_SIZE] {
        &self.0
    }
}

/// Derive a `BlobKey` from `master_key` and `uuid` using HKDF-SHA256.
///
/// # Panics
///
/// Panics if HKDF cannot expand into the fixed `BLOB_KEY_SIZE` output buffer, which is impossible
/// for the selected SHA-256 parameters.
#[must_use]
pub fn derive_blob_key(master_key: &MasterKey, uuid: &Uuid) -> BlobKey {
    let hk = Hkdf::<Sha256>::new(None, master_key.as_ref());
    let mut fk_bytes = [0; BLOB_KEY_SIZE];
    hk.expand(uuid.as_bytes(), &mut fk_bytes)
        .expect("HKDF expand failed");
    BlobKey::from_raw(fk_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::master_key::generate_master_key;

    fn _assert_send<T: Send>() {}
    const _: () = {
        let _ = _assert_send::<BlobKey>;
    };

    #[test]
    fn derive_blob_key_is_deterministic() {
        let mk = generate_master_key();
        let uuid = Uuid::new_v4();
        let fk1 = derive_blob_key(&mk, &uuid);
        let fk2 = derive_blob_key(&mk, &uuid);
        assert_eq!(fk1.as_ref(), fk2.as_ref());
    }

    #[test]
    fn different_uuids_yield_different_file_keys() {
        let mk = generate_master_key();
        let fk1 = derive_blob_key(&mk, &Uuid::new_v4());
        let fk2 = derive_blob_key(&mk, &Uuid::new_v4());
        assert_ne!(fk1.as_ref(), fk2.as_ref());
    }
}
