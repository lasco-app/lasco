use chacha20poly1305::{aead::Aead, KeyInit, XChaCha20Poly1305, XNonce};
use rand::{rngs::OsRng, RngCore};

use crate::encryption::blob_key::BlobKey;
use crate::encryption::error::{BlobError, CryptoError};

pub const BLOB_FORMAT_VERSION: u8 = 1;

pub const XCHACHA_NONCE_SIZE: usize = size_of::<XNonce>();
const _: () = assert!(XCHACHA_NONCE_SIZE == 24); // XChaCha20 nonce is fixed at 192 bits by the spec
const VERSION_SIZE: usize = size_of::<u8>();

const HEADER_LEN: usize = VERSION_SIZE + XCHACHA_NONCE_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nonce(pub [u8; XCHACHA_NONCE_SIZE]);

/// Encrypted blob with its format version, nonce, and ciphertext.
#[derive(Clone, Debug)]
pub struct BlobEncrypted {
    pub format_version: u8,
    pub nonce: Nonce,
    pub ciphertext: Vec<u8>,
}

impl BlobEncrypted {
    /// Serializes the blob to `[version | nonce | ciphertext]` bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.ciphertext.len());
        out.push(self.format_version);
        out.extend_from_slice(&self.nonce.0);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// Deserializes a blob from `[version | nonce | ciphertext]` bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BlobError> {
        if bytes.len() < HEADER_LEN {
            return Err(BlobError::Truncated {
                expected: HEADER_LEN,
                got: bytes.len(),
            });
        }
        let format_version = bytes[0];
        if format_version != BLOB_FORMAT_VERSION {
            return Err(BlobError::UnknownVersion(format_version));
        }
        let nonce = Nonce(bytes[VERSION_SIZE..HEADER_LEN].try_into().unwrap());
        let ciphertext = bytes[HEADER_LEN..].to_vec();
        Ok(Self {
            format_version,
            nonce,
            ciphertext,
        })
    }
}

/// Encrypt `plaintext` with XChaCha20-Poly1305 using a random 24-byte nonce sourced from the OS RNG.
pub fn encrypt_blob(file_key: &BlobKey, plaintext: &[u8]) -> BlobEncrypted {
    let key = chacha20poly1305::Key::from_slice(file_key.as_ref());
    let cipher = XChaCha20Poly1305::new(key);
    let mut nonce_bytes = [0; XCHACHA_NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let xnonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(xnonce, plaintext)
        .expect("XChaCha20-Poly1305 encryption failed");
    BlobEncrypted {
        format_version: BLOB_FORMAT_VERSION,
        nonce: Nonce(nonce_bytes),
        ciphertext,
    }
}

/// Decrypt a `BlobEncrypted` with XChaCha20-Poly1305. Returns an error on authentication failure.
pub fn decrypt_blob(file_key: &BlobKey, blob: &BlobEncrypted) -> Result<Vec<u8>, CryptoError> {
    let key = chacha20poly1305::Key::from_slice(file_key.as_ref());
    let cipher = XChaCha20Poly1305::new(key);
    let xnonce = XNonce::from_slice(&blob.nonce.0);
    cipher
        .decrypt(xnonce, blob.ciphertext.as_ref())
        .map_err(|_| CryptoError::AuthenticationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::blob_key::derive_blob_key;
    use crate::encryption::master_key::generate_master_key;
    use uuid::Uuid;

    fn sample() -> BlobEncrypted {
        BlobEncrypted {
            format_version: BLOB_FORMAT_VERSION,
            nonce: Nonce([1; XCHACHA_NONCE_SIZE]),
            ciphertext: vec![0, 1, 2, 3],
        }
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let original = sample();
        let bytes = original.to_bytes();
        let decoded = BlobEncrypted::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.format_version, original.format_version);
        assert_eq!(decoded.nonce, original.nonce);
        assert_eq!(decoded.ciphertext, original.ciphertext);
    }

    #[test]
    fn unknown_version_returns_error() {
        let mut bytes = sample().to_bytes();
        bytes[0] = 0;
        assert!(matches!(
            BlobEncrypted::from_bytes(&bytes),
            Err(BlobError::UnknownVersion(0))
        ));

        bytes[0] = 255;
        assert!(matches!(
            BlobEncrypted::from_bytes(&bytes),
            Err(BlobError::UnknownVersion(255))
        ));
    }

    #[test]
    fn truncated_input_returns_error() {
        let short = &[1, 2, 3, 4, 5];
        assert!(matches!(
            BlobEncrypted::from_bytes(short),
            Err(BlobError::Truncated { .. })
        ));
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let mk = generate_master_key();
        let uuid = Uuid::new_v4();
        let fk = derive_blob_key(&mk, &uuid);
        let plaintext = b"hello world, this is a secret";
        let blob = encrypt_blob(&fk, plaintext);
        let recovered = decrypt_blob(&fk, &blob).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_file_key_fails() {
        let mk = generate_master_key();
        let uuid = Uuid::new_v4();
        let fk = derive_blob_key(&mk, &uuid);
        let blob = encrypt_blob(&fk, b"secret data");
        let wrong_mk = generate_master_key();
        let wrong_fk = derive_blob_key(&wrong_mk, &uuid);
        assert!(matches!(
            decrypt_blob(&wrong_fk, &blob),
            Err(CryptoError::AuthenticationFailed)
        ));
    }

    #[test]
    fn decrypt_with_flipped_byte_fails() {
        let mk = generate_master_key();
        let uuid = Uuid::new_v4();
        let fk = derive_blob_key(&mk, &uuid);
        let mut blob = encrypt_blob(&fk, b"secret data");
        blob.ciphertext[0] ^= 0x01;
        assert!(matches!(
            decrypt_blob(&fk, &blob),
            Err(CryptoError::AuthenticationFailed)
        ));
    }
}
