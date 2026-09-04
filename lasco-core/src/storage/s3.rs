use async_trait::async_trait;
use s3::bucket::Bucket;
use s3::creds::Credentials as S3Credentials;
use s3::error::S3Error;
use s3::region::Region;

use super::{AtomicWriteMode, Result, Storage, StorageError};

/// S3-compatible object storage backend.
#[derive(Debug, Clone)]
pub struct StorageS3 {
    bucket: Box<Bucket>,
    path_prefix: Option<String>,
}

impl StorageS3 {
    /// # Errors
    ///
    /// Returns an error if the supplied S3 credentials or bucket configuration cannot be constructed.
    pub fn new(
        endpoint: &str,
        bucket_name: &str,
        region: &str,
        path_prefix: Option<&str>,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self> {
        Self::new_with_session_token(
            endpoint,
            bucket_name,
            region,
            path_prefix,
            access_key,
            secret_key,
            None,
        )
    }

    /// Creates storage with an optional temporary STS/session token.
    pub fn new_with_session_token(
        endpoint: &str,
        bucket_name: &str,
        region: &str,
        path_prefix: Option<&str>,
        access_key: &str,
        secret_key: &str,
        session_token: Option<&str>,
    ) -> Result<Self> {
        let path_prefix = normalize_path_prefix(path_prefix.unwrap_or(""));
        // For S3-compatible providers the connection host
        // comes from the region endpoint, not from a host header. Use a custom
        // region pairing the region name with the provider endpoint.
        let endpoint = normalize_endpoint(endpoint);
        // The region is part of the SigV4 signing key and must match the
        // endpoint location (e.g. Hetzner nbg1). When left blank, derive it from
        // the endpoint host so providers like Hetzner work without guessing.
        let region = if region.trim().is_empty() {
            derive_region_from_endpoint(&endpoint)
        } else {
            region.trim().to_string()
        };
        let region = Region::Custom { region, endpoint };

        // Trim accidental whitespace/newlines from pasted credentials, which
        // would otherwise produce a SignatureDoesNotMatch error.
        let access_key = access_key.trim();
        let secret_key = secret_key.trim();

        let credentials = S3Credentials::new(
            Some(access_key),
            Some(secret_key),
            None,
            session_token.filter(|token| !token.trim().is_empty()),
            None,
        )
        .map_err(|e| StorageError::Other(Box::new(e)))?;

        // Path style is needed for MinIO and other S3-compatible services.
        let bucket = Bucket::new(bucket_name, region, credentials)
            .map_err(|e| StorageError::Other(Box::new(e)))?
            .with_path_style();

        Ok(Self {
            bucket,
            path_prefix,
        })
    }

    fn prefixed_key(&self, key: &str) -> String {
        match &self.path_prefix {
            Some(prefix) => format!("{prefix}{key}"),
            None => key.to_string(),
        }
    }

    async fn put_object(&self, key: &str, data: &[u8]) -> Result<()> {
        self.bucket
            .put_object(self.prefixed_key(key), data)
            .await
            .map_err(|e| StorageError::Other(Box::new(e)))?;
        Ok(())
    }
}

#[async_trait]
impl Storage for StorageS3 {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.put_object(key, data).await
    }

    /// S3 object replacement is atomic: readers observe either the old object
    /// or the complete replacement object.
    async fn put_atomic(&self, key: &str, data: &[u8], mode: AtomicWriteMode) -> Result<bool> {
        match mode {
            AtomicWriteMode::Replace => {
                self.put_object(key, data).await?;
                Ok(true)
            }
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        match self.bucket.get_object(self.prefixed_key(key)).await {
            Ok(obj) => {
                // ResponseData has a bytes() method that returns &Bytes
                // We can convert it to Vec<u8> using to_vec() or From trait
                Ok(Vec::<u8>::from(obj))
            }
            Err(e) if is_not_found_error(&e) => Err(StorageError::NotFound),
            Err(e) => Err(StorageError::Other(Box::new(e))),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.bucket
            .delete_object(self.prefixed_key(key))
            .await
            .map_err(|e| StorageError::Other(Box::new(e)))?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let results = self
            .bucket
            .list(self.prefixed_key(prefix), Some("/".to_string()))
            .await
            .map_err(|e| StorageError::Other(Box::new(e)))?;

        let mut keys = Vec::new();
        for result in results {
            for obj in result.contents {
                let key = match &self.path_prefix {
                    Some(prefix) => obj
                        .key
                        .strip_prefix(prefix)
                        .map(std::string::ToString::to_string)
                        .unwrap_or(obj.key),
                    None => obj.key,
                };
                keys.push(key);
            }
        }
        Ok(keys)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        match self.bucket.head_object(self.prefixed_key(key)).await {
            Ok(_) => Ok(true),
            Err(e) if is_not_found_error(&e) => Ok(false),
            Err(e) => Err(StorageError::Other(Box::new(e))),
        }
    }
}

/// Normalize a user-provided key prefix so it can be concatenated directly in
/// front of a key. Trims slashes off both ends then re-adds a single trailing
/// slash. Returns `None` if the prefix is empty (bucket root).
fn normalize_path_prefix(path_prefix: &str) -> Option<String> {
    let trimmed = path_prefix.trim().trim_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("{trimmed}/"))
    }
}

/// Normalize a user-provided endpoint for `Region::Custom`.
/// rust-s3 derives the scheme and host from the endpoint string itself, so we
/// keep any explicit scheme but drop trailing slashes that would corrupt URLs.
fn normalize_endpoint(endpoint: &str) -> String {
    endpoint.trim().trim_end_matches('/').to_string()
}

/// Derive an S3 region from an endpoint host's first DNS label.
/// For `https://nbg1.your-objectstorage.com` this yields `nbg1`, which is the
/// region Hetzner and other Ceph-based providers expect in the signature.
fn derive_region_from_endpoint(endpoint: &str) -> String {
    let without_scheme = match endpoint.find("://") {
        Some(pos) => &endpoint[pos + 3..],
        None => endpoint,
    };
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    host.split('.').next().unwrap_or(host).to_string()
}

fn is_not_found_error(e: &S3Error) -> bool {
    let error_string = format!("{e:?}");
    error_string.contains("NoSuchKey") || error_string.contains("404")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_region_strips_scheme_and_takes_first_label() {
        assert_eq!(
            derive_region_from_endpoint("https://nbg1.your-objectstorage.com"),
            "nbg1"
        );
        assert_eq!(
            derive_region_from_endpoint("fsn1.your-objectstorage.com"),
            "fsn1"
        );
        assert_eq!(
            derive_region_from_endpoint("https://hel1.your-objectstorage.com/"),
            "hel1"
        );
    }

    #[test]
    fn normalize_path_prefix_adds_trailing_slash() {
        assert_eq!(normalize_path_prefix("photos"), Some("photos/".to_string()));
        assert_eq!(
            normalize_path_prefix("photos/"),
            Some("photos/".to_string())
        );
        assert_eq!(
            normalize_path_prefix("/photos/"),
            Some("photos/".to_string())
        );
        assert_eq!(normalize_path_prefix(""), None);
        assert_eq!(normalize_path_prefix("  "), None);
    }

    fn get_test_config() -> Option<(String, String, String, Option<String>, String, String)> {
        Some((
            std::env::var("S3_TEST_ENDPOINT").ok()?,
            std::env::var("S3_TEST_BUCKET").ok()?,
            std::env::var("S3_TEST_REGION").ok()?,
            std::env::var("S3_TEST_PATH_PREFIX").ok(),
            std::env::var("S3_TEST_ACCESS_KEY").ok()?,
            std::env::var("S3_TEST_SECRET_KEY").ok()?,
        ))
    }

    fn make_storage() -> Option<StorageS3> {
        let (endpoint, bucket, region, path_prefix, access_key, secret_key) = get_test_config()?;
        StorageS3::new(
            &endpoint,
            &bucket,
            &region,
            path_prefix.as_deref(),
            &access_key,
            &secret_key,
        )
        .ok()
    }

    #[tokio::test]
    #[ignore = "Requires S3 test environment"]
    async fn put_then_get_returns_identical_bytes() {
        let storage = make_storage().expect("S3 test config not set");
        storage
            .put_atomic("test/put_get", b"hello", AtomicWriteMode::Replace)
            .await
            .unwrap();
        assert_eq!(storage.get("test/put_get").await.unwrap(), b"hello");
        storage.delete("test/put_get").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires S3 test environment"]
    async fn get_missing_key_returns_not_found() {
        let storage = make_storage().expect("S3 test config not set");
        assert!(matches!(
            storage.get("missing").await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    #[ignore = "Requires S3 test environment"]
    async fn delete_removes_key() {
        let storage = make_storage().expect("S3 test config not set");
        storage
            .put_atomic("test/del", b"v", AtomicWriteMode::Replace)
            .await
            .unwrap();
        storage.delete("test/del").await.unwrap();
        assert!(matches!(
            storage.get("test/del").await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    #[ignore = "Requires S3 test environment"]
    async fn delete_missing_is_ok() {
        let storage = make_storage().expect("S3 test config not set");
        storage.delete("nope").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires S3 test environment"]
    async fn list_with_prefix() {
        let storage = make_storage().expect("S3 test config not set");
        storage
            .put_atomic("list/a", b"1", AtomicWriteMode::Replace)
            .await
            .unwrap();
        storage
            .put_atomic("list/b", b"2", AtomicWriteMode::Replace)
            .await
            .unwrap();
        storage
            .put_atomic("other/c", b"3", AtomicWriteMode::Replace)
            .await
            .unwrap();
        let keys = storage.list("list/").await.unwrap();
        assert!(keys.iter().any(|k| k == "list/a"));
        assert!(keys.iter().any(|k| k == "list/b"));
        for k in keys {
            storage.delete(&k).await.unwrap();
        }
        storage.delete("other/c").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires S3 test environment"]
    async fn path_prefix_scopes_keys_and_list_strips_it() {
        let (endpoint, bucket, region, base_prefix, access_key, secret_key) =
            get_test_config().expect("S3 test config not set");
        let base_prefix = base_prefix.unwrap_or_default();
        let prefixed_dir = format!("{}pfx-test", base_prefix.trim_end_matches('/'));
        let storage = StorageS3::new(
            &endpoint,
            &bucket,
            &region,
            Some(&prefixed_dir),
            &access_key,
            &secret_key,
        )
        .unwrap();

        storage
            .put_atomic("nested/a", b"1", AtomicWriteMode::Replace)
            .await
            .unwrap();
        let keys = storage.list("nested/").await.unwrap();
        assert!(keys.iter().any(|k| k == "nested/a"));
        assert_eq!(storage.get("nested/a").await.unwrap(), b"1");
        storage.delete("nested/a").await.unwrap();
        assert!(matches!(
            storage.get("nested/a").await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    #[ignore = "Requires S3 test environment"]
    async fn exists_behavior() {
        let storage = make_storage().expect("S3 test config not set");
        assert!(!storage.exists("test/ex").await.unwrap());
        storage
            .put_atomic("test/ex", b"v", AtomicWriteMode::Replace)
            .await
            .unwrap();
        assert!(storage.exists("test/ex").await.unwrap());
        storage.delete("test/ex").await.unwrap();
    }
}
