package com.lasco.lasco.data

import uniffi.lasco_ffi.FfiRemoteUuid

sealed interface IncrementalImportState {
    data object Idle : IncrementalImportState
    data object Scanning : IncrementalImportState
    data class Importing(val done: Int, val total: Int) : IncrementalImportState
    data class Failed(val message: String) : IncrementalImportState
}

/**
 * Transient sync and import state, kept separate from SessionState so that
 * state stays lean and records only the per-remote operations the app performs.
 */
data class SyncState(
    val busyRemoteIds: Set<FfiRemoteUuid> = emptySet(),
    val fetchInProgress: Boolean = false,
    val bulkImportProgress: Pair<Int, Int>? = null,
    val incrementalImportState: IncrementalImportState = IncrementalImportState.Idle,
    // When the scheduled auto push fires, on SystemClock.elapsedRealtime's
    // monotonic clock, or null when none is scheduled. A deadline rather than
    // a remaining count, so this changes twice per schedule instead of once a
    // second, and the UI derives the displayed seconds from it.
    val pushDeadlineElapsedMs: Long? = null,
    // Immutable set of Auto push remotes eligible when the active countdown
    // began. Remotes are revalidated against their current setting at expiry.
    val scheduledAutoPushRemoteIds: Set<FfiRemoteUuid> = emptySet(),
)
