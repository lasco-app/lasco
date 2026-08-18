use crate::crdt::{CrdtOperation, Dot};
use crate::encryption::master_key::MasterKey;
use crate::library::local_dirs::LocalStateOperations;
use crate::library::{Library, Result};
use crate::operations::local_ops as op_log;

/// Owns exclusive access to the local CRDT operation log.
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

/// This is deliberately held only across synchronous filesystem calls, never across an `.await`.
pub(crate) struct LocalOpsReadWrite<'a> {
    local_state_operations: LocalStateOperations,
    master_key: &'a MasterKey,
    _guard: parking_lot::MutexGuard<'a, ()>,
}

impl LocalOpsReadWrite<'_> {
    pub(crate) fn read_operations(&self) -> Result<Vec<CrdtOperation>> {
        Ok(op_log::read_crdt_operations(
            &self.local_state_operations.operations_log_path(),
            self.master_key,
        )?)
    }

    pub(crate) fn read_operations_range(
        &self,
        start_pos: u64,
        end_pos_exclusive: u64,
    ) -> Result<Vec<CrdtOperation>> {
        Ok(op_log::read_crdt_operations_range(
            &self.local_state_operations.operations_log_path(),
            self.master_key,
            start_pos,
            end_pos_exclusive,
        )?)
    }

    pub(crate) fn append_operation(&mut self, operation: &CrdtOperation) -> Result<()> {
        Ok(op_log::append_crdt_operation(
            &self.local_state_operations.operations_log_path(),
            self.master_key,
            operation,
        )?)
    }

    pub(crate) fn known_dots(&self) -> Result<std::collections::HashSet<Dot>> {
        Ok(op_log::read_known_dots(
            &self.local_state_operations.operations_log_path(),
            self.master_key,
        )?)
    }
}

impl Library {
    /// Acquires exclusive read/write access to `operations.log`.
    /// The returned guard must not be held across an `.await`.
    pub(crate) fn local_ops_read_write(&self) -> LocalOpsReadWrite<'_> {
        self.inner
            .local_ops_read_write_lock
            .lock(&self.inner.master_key)
    }
}
