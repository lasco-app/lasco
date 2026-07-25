package com.lasco.lasco.data

import uniffi.lasco_ffi.FfiSyncResult

/**
 * Transient sync and import state, kept separate from SessionState so that
 * state stays lean. FfiSyncResult only carries pushed and pulled counts,
 * there is no per record sync history on the FFI surface.
 */
data class SyncState(
    val busyRemoteIds: Set<String> = emptySet(),
    val fetchInProgress: Boolean = false,
    val bulkImportProgress: Pair<Int, Int>? = null,
    val lastSyncResult: FfiSyncResult? = null,
)
