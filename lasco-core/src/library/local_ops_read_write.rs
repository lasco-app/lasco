use crate::encryption::master_key::MasterKey;
use crate::library::local_dirs::LocalStateOperations;
use crate::library::{Library, Result};
use crate::operations::OperationGroup;
use crate::operations::local_ops as op_log;

/// Owns the exclusive lock that grants access to both local operation files:
/// `operations.log` and `pending.op`.
pub(crate) struct LocalOpsReadWriteLock {
    local_state_operations: LocalStateOperations,
    mutex: parking_lot::Mutex<()>,
}

impl LocalOpsReadWriteLock {
    pub(crate) fn new(local_state_operations: LocalStateOperations) -> Self {
        Self {
            local_state_operations,
            mutex: parking_lot::Mutex::new(()),
        }
    }

    /// The returned guard must not be held across an `.await`.
    pub(crate) fn lock<'a>(&'a self, master_key: &'a MasterKey) -> LocalOpsReadWrite<'a> {
        LocalOpsReadWrite {
            local_state_operations: self.local_state_operations.clone(),
            master_key,
            _guard: self.mutex.lock(),
        }
    }
}

/// Exclusive capability to read and write both local operation files:
/// `operations.log` and `pending.op`.
///
/// This is deliberately held only across synchronous filesystem calls, never across
/// an `.await`. It does not make multiple calls atomic as a group.
pub(crate) struct LocalOpsReadWrite<'a> {
    local_state_operations: LocalStateOperations,
    master_key: &'a MasterKey,
    _guard: parking_lot::MutexGuard<'a, ()>,
}

impl LocalOpsReadWrite<'_> {
    pub(crate) fn read_log_groups(&self) -> Result<Vec<OperationGroup>> {
        Ok(op_log::read_op_groups(
            &self.local_state_operations.operations_log_path(),
            self.master_key,
        )?)
    }

    pub(crate) fn append_log(&mut self, group: &OperationGroup) -> Result<()> {
        Ok(op_log::append_op_group(
            &self.local_state_operations.operations_log_path(),
            self.master_key,
            group,
        )?)
    }

    pub(crate) fn read_pending(&self) -> Result<Option<OperationGroup>> {
        Ok(op_log::read_pending_op_group(
            &self.local_state_operations.pending_op_path(),
            self.master_key,
        )?)
    }

    pub(crate) fn write_pending(&mut self, group: &OperationGroup) -> Result<()> {
        Ok(op_log::write_pending_op_group(
            &self.local_state_operations.pending_op_path(),
            self.master_key,
            group,
        )?)
    }

    pub(crate) fn remove_pending(&mut self) -> Result<()> {
        Ok(op_log::remove_pending_op_group(
            &self.local_state_operations.pending_op_path(),
        )?)
    }
}

impl Library {
    /// Acquires exclusive read/write access to `pending.op` and `operations.log`.
    /// The returned guard must not be held across an `.await`.
    pub(crate) fn local_ops_read_write(&self) -> LocalOpsReadWrite<'_> {
        self.inner
            .local_ops_read_write_lock
            .lock(&self.inner.master_key)
    }
}
