package com.lasco.lasco.data

import android.os.SystemClock
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.lasco_ffi.FfiLibrary
import uniffi.lasco_ffi.FfiMediaId
import uniffi.lasco_ffi.LascoException
import uniffi.lasco_ffi.FfiRemoteUuid
import uniffi.lasco_ffi.PushProgressSink
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong

sealed interface PushResult {
    data object Success : PushResult
    data class MissingLocalMedia(val mediaIds: List<FfiMediaId>) : PushResult
    /**
     * Push preparation found no place to get some media from. The remedy is confirming one or
     * more remotes, so a manual push offers that instead of only reporting the failure. An
     * automatic push has no one to ask and reports Failed instead.
     */
    data class MissingMediaOnConfiguredSources(val mediaIds: List<FfiMediaId>) : PushResult
    data class Failed(val message: String) : PushResult
}

sealed interface ConfirmMediaResult {
    data class Confirmed(val newlyConfirmed: ULong) : ConfirmMediaResult
    data class Failed(val message: String) : ConfirmMediaResult
}

/**
 * Owned by LibraryRepository for the lifetime of the open library. Holds the
 * transient sync state pulled out of the Swift LibraryModel (busyRemotes,
 * fetchInProgress), operational state rather than session identity, so it
 * lives here and not on SessionState. Records push/fetch results into Prefs,
 * the Android equivalent of Swift's lastPushRecords/lastFetchRecords.
 *
 * Rust owns sync admission. This controller starts each request immediately and
 * tracks it only so the UI reflects the operations that are actually in flight.
 */
class SyncController(
    private val lib: FfiLibrary,
    private val prefs: Prefs,
    private val onLibraryChanged: suspend () -> Unit,
    private val scope: CoroutineScope,
) {
    private val _syncState = MutableStateFlow(SyncState())
    val syncState: StateFlow<SyncState> = _syncState.asStateFlow()

    private enum class OperationKind { Push, Fetch, Confirm }

    private data class ActiveOperation(
        val remoteId: FfiRemoteUuid,
        val kind: OperationKind,
        val completion: kotlinx.coroutines.CompletableDeferred<Unit>,
    )

    private val operationMutex = Mutex()
    private val activeOperations = mutableMapOf<UUID, ActiveOperation>()
    private val scheduleCancellationEpoch = AtomicLong(0)
    private var scheduledAutoPushJob: Job? = null
    private var closed = false

    // Edits made while a push is already scheduled ride along with it rather
    // than pushing the deadline back, so a steady stream of edits still gets
    // pushed within the window instead of starving.
    fun schedulePush() {
        val cancellationEpoch = scheduleCancellationEpoch.get()
        scope.launch { schedulePushIfNeeded(cancellationEpoch) }
    }

    // No effect on a push already running.
    fun stopScheduledPush() {
        scope.launch { cancelScheduledPush() }
    }

    fun setIncrementalImportState(state: IncrementalImportState) {
        _syncState.update { it.copy(incrementalImportState = state) }
    }

    /**
     * Pushes one remote, returning an error message or null on success,
     * mirroring Swift's LibraryModel.pushRemote. Clears any pending countdown
     * and immediately asks Rust to start the operation.
     */
    suspend fun pushRemote(remoteId: FfiRemoteUuid): PushResult {
        cancelScheduledPush()
        return push(remoteId, isAutomatic = false)
    }

    /**
     * Updates one remote's media list without fetching, which is how a user resolves a push
     * blocked by an out-of-date media list.
     */
    suspend fun confirmRemoteMedia(remoteId: FfiRemoteUuid): ConfirmMediaResult {
        return confirmMedia(remoteId)
    }

    suspend fun fetchRemoteWithResult(remoteId: FfiRemoteUuid): String? {
        return fetch(remoteId)
    }

    // Stops the schedule and waits for operations already in flight to finish,
    // since the caller is usually about to delete the library files it is
    // still reading.
    suspend fun close() {
        val completions = operationMutex.withLock {
            closed = true
            scheduleCancellationEpoch.incrementAndGet()
            scheduledAutoPushJob?.cancel()
            scheduledAutoPushJob = null
            publishCountdown(null, emptySet())
            activeOperations.values.map { it.completion }
        }
        completions.awaitAll()
    }

    private suspend fun push(remoteId: FfiRemoteUuid, isAutomatic: Boolean): PushResult {
        val operationId = UUID.randomUUID()
        val operation = beginOperation(operationId, remoteId, OperationKind.Push)
            ?: return PushResult.Failed("Library closed")
        val progress = object : PushProgressSink {
            override fun uploadProgress(fraction: Double) {
                _syncState.update {
                    if (remoteId !in it.pushingRemoteIds) it
                    else it.copy(pushUploadProgress = it.pushUploadProgress + (remoteId to fraction.toFloat()))
                }
            }
        }
        return try {
            lib.pushRemoteUsingConfiguredMediaSourcesAsync(remoteId, null, progress)
            prefs.recordPush(remoteId, success = true)
            PushResult.Success
        } catch (e: LascoException.SyncBusy) {
            PushResult.Failed("A conflicting sync is already in progress")
        } catch (e: LascoException.MissingLocalMedia) {
            prefs.recordPush(remoteId, success = false)
            PushResult.MissingLocalMedia(e.mediaIds)
        } catch (e: LascoException.MissingMediaOnConfiguredSources) {
            prefs.recordPush(remoteId, success = false)
            // Only a manual push can offer the remedy, since it is the one a user is watching.
            if (isAutomatic) {
                PushResult.Failed("Some media have no known place to be copied from")
            } else {
                PushResult.MissingMediaOnConfiguredSources(e.mediaIds)
            }
        } catch (e: Exception) {
            prefs.recordPush(remoteId, success = false)
            PushResult.Failed(e.message?.ifBlank { null } ?: "Push failed")
        } finally {
            endOperation(operationId, operation)
        }
    }

    // Refreshes what this client knows of the media one remote holds, without fetching.
    private suspend fun confirmMedia(remoteId: FfiRemoteUuid): ConfirmMediaResult {
        val operationId = UUID.randomUUID()
        val operation = beginOperation(operationId, remoteId, OperationKind.Confirm)
            ?: return ConfirmMediaResult.Failed("Library closed")
        return try {
            ConfirmMediaResult.Confirmed(lib.confirmRemoteMediaAsync(remoteId, null))
        } catch (e: LascoException.SyncBusy) {
            ConfirmMediaResult.Failed("A conflicting sync is already in progress")
        } catch (e: Exception) {
            ConfirmMediaResult.Failed(e.message?.ifBlank { null } ?: "Could not update the media list")
        } finally {
            endOperation(operationId, operation)
        }
    }

    private suspend fun fetch(remoteId: FfiRemoteUuid): String? {
        val operationId = UUID.randomUUID()
        val operation = beginOperation(operationId, remoteId, OperationKind.Fetch)
            ?: return "Library closed"
        return try {
            lib.fetchRemoteAsync(remoteId, null)
            prefs.recordFetch(remoteId, success = true)
            onLibraryChanged()
            null
        } catch (e: LascoException.SyncBusy) {
            "A conflicting sync is already in progress"
        } catch (e: Exception) {
            prefs.recordFetch(remoteId, success = false)
            e.message?.ifBlank { null } ?: "Fetch failed"
        } finally {
            endOperation(operationId, operation)
        }
    }

    private suspend fun schedulePushIfNeeded(cancellationEpoch: Long) {
        val remoteIds = lib.listRemotes()
            .filter { it.autoPush }
            .map { it.remoteId }
            .toSet()
        if (remoteIds.isEmpty()) return
        operationMutex.withLock {
            if (closed || cancellationEpoch != scheduleCancellationEpoch.get() || scheduledAutoPushJob != null) return
            publishCountdown(SystemClock.elapsedRealtime() + PUSH_DELAY_MS, remoteIds)
            scheduledAutoPushJob = scope.launch {
                delay(PUSH_DELAY_MS)
                fireScheduledPushes(remoteIds, cancellationEpoch)
            }
        }
    }

    private suspend fun cancelScheduledPush() {
        scheduleCancellationEpoch.incrementAndGet()
        operationMutex.withLock {
            scheduledAutoPushJob?.cancel()
            scheduledAutoPushJob = null
            publishCountdown(null, emptySet())
        }
    }

    private suspend fun fireScheduledPushes(candidateRemoteIds: Set<FfiRemoteUuid>, cancellationEpoch: Long) {
        operationMutex.withLock {
            if (closed || cancellationEpoch != scheduleCancellationEpoch.get()) return
            scheduledAutoPushJob = null
            publishCountdown(null, emptySet())
        }
        val remotes = lib.listRemotes().filter { it.remoteId in candidateRemoteIds && it.autoPush }
        coroutineScope {
            for (remote in remotes) {
                launch { push(remote.remoteId, isAutomatic = true) }
            }
        }
    }

    private suspend fun beginOperation(
        operationId: UUID,
        remoteId: FfiRemoteUuid,
        kind: OperationKind,
    ): ActiveOperation? = operationMutex.withLock {
        if (closed) return@withLock null
        ActiveOperation(remoteId, kind, kotlinx.coroutines.CompletableDeferred<Unit>()).also { operation ->
            activeOperations[operationId] = operation
            publishOperationState()
        }
    }

    private suspend fun endOperation(operationId: UUID, operation: ActiveOperation) {
        operationMutex.withLock {
            activeOperations.remove(operationId)
            publishOperationState()
            operation.completion.complete(Unit)
        }
    }

    private fun publishOperationState() {
        val operations = activeOperations.values
        val busyRemoteIds = operations.mapTo(mutableSetOf()) { it.remoteId }
        val pushingRemoteIds = operations
            .filter { it.kind == OperationKind.Push }
            .mapTo(mutableSetOf()) { it.remoteId }
        val fetchInProgress = operations.any { it.kind == OperationKind.Fetch }
        _syncState.update {
            it.copy(
                busyRemoteIds = busyRemoteIds,
                pushingRemoteIds = pushingRemoteIds,
                pushUploadProgress = it.pushUploadProgress.filterKeys { remoteId -> remoteId in pushingRemoteIds },
                fetchInProgress = fetchInProgress,
            )
        }
    }

    private fun publishCountdown(deadline: Long?, remoteIds: Set<FfiRemoteUuid>) {
        _syncState.update {
            it.copy(
                pushDeadlineElapsedMs = deadline,
                scheduledAutoPushRemoteIds = remoteIds,
            )
        }
    }

    companion object {
        private const val PUSH_DELAY_MS = 30_000L
    }
}
