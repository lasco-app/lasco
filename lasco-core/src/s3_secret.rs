use aes_gcm::{
    aead::{Aead, AeadCore},
    Aes256Gcm, KeyInit,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use rand::rngs::OsRng;

use crate::encryption::master_key::MasterKey;
use crate::error::CryptoError;
use crate::library_json::S3Config;

/// Encryption description string for S3 secret keys
pub const S3_SECRET_ENCRYPTION_DESCRIPTION: &str = "AES-256-GCM v1";

const S3_SECRET_NONCE_SIZE: usize = 12;

/// Encrypt an S3 secret key using the MasterKey (AES-256-GCM).
/// Returns the base64 ciphertext and its encryption description.
pub fn encrypt_s3_secret_key(
    master_key: &MasterKey,
    secret_key: &str,
) -> Result<(String, String), CryptoError> {
    let key = aes_gcm::Key::<Aes256Gcm>::from_slice(master_key.as_ref());
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, secret_key.as_bytes())
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    let mut encrypted_data = Vec::with_capacity(S3_SECRET_NONCE_SIZE + ciphertext.len());
    encrypted_data.extend_from_slice(&nonce);
    encrypted_data.extend(ciphertext);
    let encoded = BASE64_STANDARD.encode(encrypted_data);
    Ok((encoded, S3_SECRET_ENCRYPTION_DESCRIPTION.to_string()))
}

/// Decrypt an S3 secret key using the MasterKey (AES-256-GCM).
pub fn decrypt_s3_secret_key(
    master_key: &MasterKey,
    encrypted: &str,
    description: &str,
) -> Result<String, CryptoError> {
    if description != S3_SECRET_ENCRYPTION_DESCRIPTION {
        return Err(CryptoError::AuthenticationFailed);
    }
    let encrypted_data = BASE64_STANDARD
        .decode(encrypted)
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    if encrypted_data.len() < S3_SECRET_NONCE_SIZE {
        return Err(CryptoError::AuthenticationFailed);
    }
    let (nonce_bytes, ciphertext) = encrypted_data.split_at(S3_SECRET_NONCE_SIZE);
    let key = aes_gcm::Key::<Aes256Gcm>::from_slice(master_key.as_ref());
    let cipher = Aes256Gcm::new(key);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    String::from_utf8(plaintext).map_err(|_| CryptoError::AuthenticationFailed)
}

/// Resolve S3 credentials from an `S3Config`, decrypting the secret key with the master key.
pub fn resolve_s3_credentials(
    s3_config: &S3Config,
    master_key: &MasterKey,
) -> Result<(String, String), CryptoError> {
    let secret = decrypt_s3_secret_key(
        master_key,
        &s3_config.secret_key_encrypted,
        &s3_config.secret_key_encryption_description,
    )?;
    Ok((s3_config.access_key.clone(), secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::master_key::generate_master_key;

    #[test]
    fn encrypt_decrypt_s3_secret_round_trip() {
        let mk = generate_master_key();
        let secret = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        let (encrypted, desc) = encrypt_s3_secret_key(&mk, secret).unwrap();
        assert!(!encrypted.is_empty());
        assert_eq!(desc, S3_SECRET_ENCRYPTION_DESCRIPTION);
        let decrypted = decrypt_s3_secret_key(&mk, &encrypted, &desc).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn decrypt_with_wrong_master_key_fails() {
        let mk1 = generate_master_key();
        let mk2 = generate_master_key();
        let (encrypted, desc) = encrypt_s3_secret_key(&mk1, "my-secret-key").unwrap();
        assert!(decrypt_s3_secret_key(&mk2, &encrypted, &desc).is_err());
    }

    #[test]
    fn decrypt_with_wrong_description_fails() {
        let mk = generate_master_key();
        let (encrypted, _) = encrypt_s3_secret_key(&mk, "my-secret-key").unwrap();
        assert!(decrypt_s3_secret_key(&mk, &encrypted, "wrong-description").is_err());
    }

    #[test]
    fn decrypt_with_invalid_base64_fails() {
        let mk = generate_master_key();
        assert!(
            decrypt_s3_secret_key(&mk, "not-valid-base64!!!", S3_SECRET_ENCRYPTION_DESCRIPTION)
                .is_err()
        );
    }

    #[test]
    fn resolve_s3_credentials_decrypts() {
        let mk = generate_master_key();
        let secret = "encrypted-secret-value";
        let (encrypted, desc) = encrypt_s3_secret_key(&mk, secret).unwrap();
        let s3_config = S3Config {
            endpoint: "https://s3.example.com".to_string(),
            bucket: "my-bucket".to_string(),
            region: "us-east-1".to_string(),
            path_prefix: None,
            access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_key_encrypted: encrypted,
            secret_key_encryption_description: desc,
        };
        let (access_key, secret_key) = resolve_s3_credentials(&s3_config, &mk).unwrap();
        assert_eq!(access_key, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(secret_key, secret);
    }
}
