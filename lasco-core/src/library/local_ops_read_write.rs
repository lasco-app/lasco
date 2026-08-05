use std::collections::HashSet;

use crate::encryption::master_key::MasterKey;
use crate::identifiers::OpUuid;
use crate::library::local_dirs::LocalDirs;
use crate::library::{Library, Result};
use crate::operations::local_ops as op_log;
use crate::operations::OperationGroup;

/// Exclusive capability to read and write the local operation files.
///
/// This is deliberately held only across synchronous filesystem calls, never across
/// an `.await`. It does not make multiple calls atomic as a group.
pub(crate) struct LocalOpsReadWrite<'a> {
    local_dirs: &'a LocalDirs,
    master_key: &'a MasterKey,
    _guard: parking_lot::MutexGuard<'a, ()>,
}

impl LocalOpsReadWrite<'_> {
    pub(crate) fn read_log_ids(&self) -> Result<HashSet<OpUuid>> {
        Ok(op_log::read_op_ids(&self.local_dirs.operations_log_path())?)
    }

    pub(crate) fn read_log_groups(&self) -> Result<Vec<OperationGroup>> {
        Ok(op_log::read_op_groups(
            &self.local_dirs.operations_log_path(),
            self.master_key,
        )?)
    }

    pub(crate) fn append_log(&mut self, group: &OperationGroup) -> Result<()> {
        Ok(op_log::append_op_group(
            &self.local_dirs.operations_log_path(),
            self.master_key,
            group,
        )?)
    }

    pub(crate) fn read_pending(&self) -> Result<Option<OperationGroup>> {
        Ok(op_log::read_pending_op_group(
            &self.local_dirs.pending_op_path(),
            self.master_key,
        )?)
    }

    pub(crate) fn write_pending(&mut self, group: &OperationGroup) -> Result<()> {
        Ok(op_log::write_pending_op_group(
            &self.local_dirs.pending_op_path(),
            self.master_key,
            group,
        )?)
    }

    pub(crate) fn take_pending(&mut self) -> Result<Option<OperationGroup>> {
        Ok(op_log::take_pending_op_group(
            &self.local_dirs.pending_op_path(),
            self.master_key,
        )?)
    }
}

impl Library {
    /// Acquires exclusive read/write access to `pending.op` and `operations.log`.
    /// The returned guard must not be held across an `.await`.
    pub(crate) fn local_ops_read_write(&self) -> LocalOpsReadWrite<'_> {
        LocalOpsReadWrite {
            local_dirs: &self.inner.local_dirs,
            master_key: &self.inner.master_key,
            _guard: self.inner.op_files_lock.lock(),
        }
    }
}
