use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use super::{AtomicWriteMode, Result, Storage, StorageError, StorageMockMemory};

/// Storage operations that can be targeted by [`StorageMockMemoryFaulty`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageMockOperation {
    Put,
    PutAtomic,
    Get,
    Delete,
    List,
    Exists,
}

#[derive(Clone, Debug)]
struct FaultRule {
    operation: StorageMockOperation,
    key_prefix: String,
    matches_before_failure: usize,
    remaining_matches: usize,
    timing: FaultTiming,
}

#[derive(Clone, Copy, Debug)]
enum FaultTiming {
    Before,
    After,
}

/// An in-memory storage backend that can report targeted, one-shot failures.
///
/// A matching operation can return an error before it reaches the underlying
/// [`StorageMockMemory`], or after the underlying operation succeeds. The latter
/// models a response that is lost after a remote write is accepted.
#[derive(Clone, Debug, Default)]
pub struct StorageMockMemoryFaulty {
    inner: StorageMockMemory,
    faults: Arc<Mutex<Vec<FaultRule>>>,
}

impl StorageMockMemoryFaulty {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Make the next matching operation report an error without changing storage.
    pub fn fail_next(&self, operation: StorageMockOperation, key_prefix: impl Into<String>) {
        self.fail_on_match(operation, key_prefix, 1);
    }

    /// Make the selected matching operation report an error without changing storage.
    ///
    /// `match_number` is one-based: `1` fails the first matching operation, `2` the second,
    /// and so on.
    pub fn fail_on_match(
        &self,
        operation: StorageMockOperation,
        key_prefix: impl Into<String>,
        match_number: usize,
    ) {
        assert!(match_number > 0, "match_number must be at least one");
        self.faults.lock().push(FaultRule {
            operation,
            key_prefix: key_prefix.into(),
            matches_before_failure: match_number - 1,
            remaining_matches: 1,
            timing: FaultTiming::Before,
        });
    }

    /// Make the selected matching operation change storage, then report an error.
    ///
    /// This models a response that is lost after the remote has accepted a write.
    /// `match_number` is one-based: `1` targets the first matching operation.
    pub fn fail_after_on_match(
        &self,
        operation: StorageMockOperation,
        key_prefix: impl Into<String>,
        match_number: usize,
    ) {
        assert!(match_number > 0, "match_number must be at least one");
        self.faults.lock().push(FaultRule {
            operation,
            key_prefix: key_prefix.into(),
            matches_before_failure: match_number - 1,
            remaining_matches: 1,
            timing: FaultTiming::After,
        });
    }

    /// Force all operations to report an unavailable remote error.
    pub fn set_offline(&self, offline: bool) {
        self.inner.set_offline(offline);
    }

    #[must_use]
    pub fn pending_fault_count(&self) -> usize {
        self.faults.lock().len()
    }

    fn take_fault(&self, operation: StorageMockOperation, key: &str) -> Option<FaultTiming> {
        let mut faults = self.faults.lock();
        let Some(index) = faults
            .iter()
            .position(|rule| rule.operation == operation && key.starts_with(&rule.key_prefix))
        else {
            return None;
        };

        let rule = &mut faults[index];
        if rule.matches_before_failure > 0 {
            rule.matches_before_failure -= 1;
            return None;
        }
        let timing = rule.timing;
        rule.remaining_matches -= 1;
        if rule.remaining_matches == 0 {
            faults.remove(index);
        }
        Some(timing)
    }

    fn failure(operation: StorageMockOperation, key: &str) -> StorageError {
        StorageError::Unavailable(format!("injected failure for {operation:?} on key '{key}'"))
    }
}

#[async_trait]
impl Storage for StorageMockMemoryFaulty {
    #[allow(deprecated)]
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        let fault = self.take_fault(StorageMockOperation::Put, key);
        if matches!(fault, Some(FaultTiming::Before)) {
            return Err(Self::failure(StorageMockOperation::Put, key));
        }
        self.inner.put(key, data).await?;
        if matches!(fault, Some(FaultTiming::After)) {
            return Err(Self::failure(StorageMockOperation::Put, key));
        }
        Ok(())
    }

    async fn put_atomic(&self, key: &str, data: &[u8], mode: AtomicWriteMode) -> Result<bool> {
        let fault = self.take_fault(StorageMockOperation::PutAtomic, key);
        if matches!(fault, Some(FaultTiming::Before)) {
            return Err(Self::failure(StorageMockOperation::PutAtomic, key));
        }
        let result = self.inner.put_atomic(key, data, mode).await?;
        if matches!(fault, Some(FaultTiming::After)) {
            return Err(Self::failure(StorageMockOperation::PutAtomic, key));
        }
        Ok(result)
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let fault = self.take_fault(StorageMockOperation::Get, key);
        if matches!(fault, Some(FaultTiming::Before)) {
            return Err(Self::failure(StorageMockOperation::Get, key));
        }
        let result = self.inner.get(key).await?;
        if matches!(fault, Some(FaultTiming::After)) {
            return Err(Self::failure(StorageMockOperation::Get, key));
        }
        Ok(result)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let fault = self.take_fault(StorageMockOperation::Delete, key);
        if matches!(fault, Some(FaultTiming::Before)) {
            return Err(Self::failure(StorageMockOperation::Delete, key));
        }
        self.inner.delete(key).await?;
        if matches!(fault, Some(FaultTiming::After)) {
            return Err(Self::failure(StorageMockOperation::Delete, key));
        }
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let fault = self.take_fault(StorageMockOperation::List, prefix);
        if matches!(fault, Some(FaultTiming::Before)) {
            return Err(Self::failure(StorageMockOperation::List, prefix));
        }
        let result = self.inner.list(prefix).await?;
        if matches!(fault, Some(FaultTiming::After)) {
            return Err(Self::failure(StorageMockOperation::List, prefix));
        }
        Ok(result)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let fault = self.take_fault(StorageMockOperation::Exists, key);
        if matches!(fault, Some(FaultTiming::Before)) {
            return Err(Self::failure(StorageMockOperation::Exists, key));
        }
        let result = self.inner.exists(key).await?;
        if matches!(fault, Some(FaultTiming::After)) {
            return Err(Self::failure(StorageMockOperation::Exists, key));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn matching_failure_is_reported_without_writing() {
        let storage = StorageMockMemoryFaulty::new();
        storage.fail_next(StorageMockOperation::PutAtomic, "media/");

        assert!(
            storage
                .put_atomic("media/a.data", b"payload", AtomicWriteMode::Replace)
                .await
                .is_err()
        );
        assert!(matches!(
            storage.get("media/a.data").await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn failure_can_target_a_later_matching_operation() {
        let storage = StorageMockMemoryFaulty::new();
        storage.fail_on_match(StorageMockOperation::PutAtomic, "media/", 2);

        storage
            .put_atomic("media/a.data", b"a", AtomicWriteMode::Replace)
            .await
            .unwrap();
        assert!(
            storage
                .put_atomic("media/b.data", b"b", AtomicWriteMode::Replace)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn post_write_failure_reports_an_error_after_writing() {
        let storage = StorageMockMemoryFaulty::new();
        storage.fail_after_on_match(StorageMockOperation::PutAtomic, "media/", 1);

        assert!(
            storage
                .put_atomic("media/a.data", b"payload", AtomicWriteMode::CreateIfAbsent)
                .await
                .is_err()
        );
        assert_eq!(storage.get("media/a.data").await.unwrap(), b"payload");
    }
}
