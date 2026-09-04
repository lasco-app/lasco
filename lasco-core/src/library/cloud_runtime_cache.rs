//! Encrypted local cache for resolved Lasco Cloud S3 credentials.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, AeadCore, Payload},
};
use chrono::{DateTime, Utc};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::atomic_file;
use crate::encryption::master_key::MasterKey;
use crate::identifiers::LibraryId;

const MAGIC: &[u8; 4] = b"LCR1";
const NONCE_SIZE: usize = 12;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CachedCloudS3Credentials {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub path_prefix: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
struct PersistedCloudRuntime {
    format_version: u8,
    credentials_by_storage_id: HashMap<String, CachedCloudS3Credentials>,
}

pub(crate) struct CloudRuntimeCache {
    path: PathBuf,
    master_key: MasterKey,
    aad: Vec<u8>,
}

impl CloudRuntimeCache {
    pub(crate) fn new(path: PathBuf, library_id: LibraryId, master_key: &MasterKey) -> Self {
        Self {
            path,
            master_key: master_key.clone(),
            aad: format!("lasco-cloud-runtime-v1:{library_id}").into_bytes(),
        }
    }

    pub(crate) fn load(&self) -> Result<HashMap<String, CachedCloudS3Credentials>, CacheError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(error) => return Err(CacheError::Io(error)),
        };
        if bytes.len() < MAGIC.len() + NONCE_SIZE || &bytes[..MAGIC.len()] != MAGIC {
            return Err(CacheError::InvalidEnvelope);
        }
        let nonce = aes_gcm::Nonce::from_slice(&bytes[MAGIC.len()..MAGIC.len() + NONCE_SIZE]);
        let cipher = Aes256Gcm::new_from_slice(self.master_key.as_ref())
            .expect("master keys always have the AES-256 length");
        let plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &bytes[MAGIC.len() + NONCE_SIZE..],
                    aad: &self.aad,
                },
            )
            .map_err(|_| CacheError::Authentication)?;
        let persisted: PersistedCloudRuntime =
            serde_json::from_slice(&plaintext).map_err(CacheError::Deserialize)?;
        if persisted.format_version != 1 {
            return Err(CacheError::UnsupportedVersion(persisted.format_version));
        }
        Ok(persisted.credentials_by_storage_id)
    }

    pub(crate) fn save(
        &self,
        credentials_by_storage_id: &HashMap<String, CachedCloudS3Credentials>,
    ) -> Result<(), CacheError> {
        let plaintext = serde_json::to_vec(&PersistedCloudRuntime {
            format_version: 1,
            credentials_by_storage_id: credentials_by_storage_id.clone(),
        })
        .map_err(CacheError::Serialize)?;
        let cipher = Aes256Gcm::new_from_slice(self.master_key.as_ref())
            .expect("master keys always have the AES-256 length");
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: &self.aad,
                },
            )
            .expect("AES-256-GCM encryption with a valid key cannot fail");
        let mut bytes = Vec::with_capacity(MAGIC.len() + NONCE_SIZE + ciphertext.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&ciphertext);
        let parent = self
            .path
            .parent()
            .expect("cloud runtime cache path has a parent");
        std::fs::create_dir_all(parent).map_err(CacheError::Io)?;
        atomic_file::write(&self.path, &bytes).map_err(CacheError::Io)
    }

    /// Atomically replaces the cache with an empty encrypted credential map.
    pub(crate) fn clear(&self) -> Result<(), CacheError> {
        self.save(&HashMap::new())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CacheError {
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),
    #[error("invalid encrypted cache envelope")]
    InvalidEnvelope,
    #[error("cache authentication failed")]
    Authentication,
    #[error("unsupported cache format version {0}")]
    UnsupportedVersion(u8),
    #[error("cache serialization failed: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("cache deserialization failed: {0}")]
    Deserialize(#[source] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::master_key::generate_master_key;

    #[test]
    fn cache_round_trips_and_is_bound_to_its_library() {
        let directory = tempfile::tempdir().unwrap();
        let library_id = LibraryId::new();
        let master_key = generate_master_key();
        let cache = CloudRuntimeCache::new(
            directory.path().join("cloud-runtime.enc"),
            library_id,
            &master_key,
        );
        let mut credentials = HashMap::new();
        credentials.insert(
            "storage-1".to_string(),
            CachedCloudS3Credentials {
                endpoint: "https://s3.example".to_string(),
                bucket: "bucket".to_string(),
                region: "region".to_string(),
                path_prefix: "photos".to_string(),
                access_key_id: "access".to_string(),
                secret_access_key: "secret".to_string(),
                session_token: Some("session".to_string()),
                expires_at: Utc::now(),
            },
        );
        cache.save(&credentials).unwrap();
        assert_eq!(
            cache
                .load()
                .unwrap()
                .get("storage-1")
                .unwrap()
                .secret_access_key,
            "secret"
        );

        let other =
            CloudRuntimeCache::new(cache.path().to_path_buf(), LibraryId::new(), &master_key);
        assert!(matches!(other.load(), Err(CacheError::Authentication)));
    }
}
