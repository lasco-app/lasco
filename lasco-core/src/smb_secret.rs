use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, AeadCore},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rand::rngs::OsRng;

use crate::encryption::master_key::MasterKey;
use crate::error::CryptoError;
use crate::library_json::SmbConfig;

/// Encryption description for SMB passwords stored in library configuration.
pub const SMB_PASSWORD_ENCRYPTION_DESCRIPTION: &str = "AES-256-GCM v1";

const SMB_PASSWORD_NONCE_SIZE: usize = 12;

/// Encrypt an SMB password with the library master key.
///
/// # Errors
///
/// Returns an error if AES-GCM encryption fails.
pub fn encrypt_smb_password(
    master_key: &MasterKey,
    password: &str,
) -> Result<(String, String), CryptoError> {
    let key = aes_gcm::Key::<Aes256Gcm>::from_slice(master_key.as_ref());
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, password.as_bytes())
        .map_err(|_encryption_error| CryptoError::AuthenticationFailed)?;
    let mut encrypted_data = Vec::with_capacity(SMB_PASSWORD_NONCE_SIZE + ciphertext.len());
    encrypted_data.extend_from_slice(&nonce);
    encrypted_data.extend(ciphertext);
    Ok((
        BASE64_STANDARD.encode(encrypted_data),
        SMB_PASSWORD_ENCRYPTION_DESCRIPTION.to_string(),
    ))
}

/// Decrypt an SMB password with the library master key.
///
/// # Errors
///
/// Returns an authentication error for invalid ciphertext, descriptions, or keys.
pub fn decrypt_smb_password(
    master_key: &MasterKey,
    encrypted: &str,
    description: &str,
) -> Result<String, CryptoError> {
    if description != SMB_PASSWORD_ENCRYPTION_DESCRIPTION {
        return Err(CryptoError::AuthenticationFailed);
    }
    let encrypted_data = BASE64_STANDARD
        .decode(encrypted)
        .map_err(|_authentication_error| CryptoError::AuthenticationFailed)?;
    if encrypted_data.len() < SMB_PASSWORD_NONCE_SIZE {
        return Err(CryptoError::AuthenticationFailed);
    }
    let (nonce_bytes, ciphertext) = encrypted_data.split_at(SMB_PASSWORD_NONCE_SIZE);
    let key = aes_gcm::Key::<Aes256Gcm>::from_slice(master_key.as_ref());
    let cipher = Aes256Gcm::new(key);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_authentication_error| CryptoError::AuthenticationFailed)?;
    String::from_utf8(plaintext).map_err(|_authentication_error| CryptoError::AuthenticationFailed)
}

/// Resolve an SMB configuration's plaintext password with the library master key.
///
/// # Errors
///
/// Returns an authentication error if the saved credential cannot be decrypted.
pub fn resolve_smb_password(
    config: &SmbConfig,
    master_key: &MasterKey,
) -> Result<String, CryptoError> {
    decrypt_smb_password(
        master_key,
        &config.password_encrypted,
        &config.password_encryption_description,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::master_key::generate_master_key;

    #[test]
    fn password_round_trip() {
        let master_key = generate_master_key();
        let (encrypted, description) =
            encrypt_smb_password(&master_key, "correct horse battery staple").unwrap();
        assert_eq!(
            decrypt_smb_password(&master_key, &encrypted, &description).unwrap(),
            "correct horse battery staple"
        );
    }

    #[test]
    fn wrong_key_is_rejected() {
        let first = generate_master_key();
        let second = generate_master_key();
        let (encrypted, description) = encrypt_smb_password(&first, "secret").unwrap();
        assert!(decrypt_smb_password(&second, &encrypted, &description).is_err());
    }
}
