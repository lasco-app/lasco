package com.lasco.lasco.data

import android.os.SystemClock
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.onTimeout
import kotlinx.coroutines.selects.select
import uniffi.lasco_ffi.FfiLibrary

/**
 * Owned by LibraryRepository for the lifetime of the open library. Holds the
 * transient sync state pulled out of the Swift LibraryModel (busyRemotes,
 * fetchInProgress), operational state rather than session identity, so it
 * lives here and not on SessionState. Records push/fetch results into Prefs,
 * the Android equivalent of Swift's lastPushRecords/lastFetchRecords.
 *
 * Pushes, fetches and the auto push countdown are serialized through one
 * command loop, so a manual push can never race a scheduled one and the
 * countdown can never disagree with what is actually running.
 */
class SyncController(
    private val lib: FfiLibrary,
    private val prefs: Prefs,
    private val onLibraryChanged: suspend () -> Unit,
    private val scope: CoroutineScope,
) {
    private val _syncState = MutableStateFlow(SyncState())
    val syncState: StateFlow<SyncState> = _syncState.asStateFlow()

    private sealed interface Cmd {
        data object Mutated : Cmd
        data object StopCountdown : Cmd
        data class Push(val remoteId: String, val ack: CompletableDeferred<String?>) : Cmd
        data class Fetch(val remoteId: String, val ack: CompletableDeferred<String?>) : Cmd
    }

    private val commands = Channel<Cmd>(Channel.UNLIMITED)

    @OptIn(ExperimentalCoroutinesApi::class)
    private val loop = scope.launch {
        // The schedule itself. Recomputing the timeout from it on every pass
        // means commands arriving mid wait cannot push the deadline back,
        // however often they arrive.
        var deadline: Long? = null
        var scheduledAutoPushRemoteIds = emptySet<String>()
        try {
            while (true) {
                val cmd = select<Cmd?> {
                    commands.onReceiveCatching { it.getOrNull() }
                    deadline?.let { onTimeout(it - SystemClock.elapsedRealtime()) { null } }
                }
                if (cmd == null && commands.isClosedForReceive) break
                if (cmd == null) {
                    val candidateRemoteIds = scheduledAutoPushRemoteIds
                    deadline = null
                    scheduledAutoPushRemoteIds = emptySet()
                    publishCountdown(null, emptySet())
                    pushScheduledRemotes(candidateRemoteIds)
                    continue
                }
                when (cmd) {
                    Cmd.Mutated -> if (deadline == null) {
                        val candidateRemoteIds = lib.listRemotes()
                            .filter { it.autoPush }
                            .map { it.id }
                            .toSet()
                        if (candidateRemoteIds.isNotEmpty()) {
                            deadline = SystemClock.elapsedRealtime() + PUSH_DELAY_MS
                            scheduledAutoPushRemoteIds = candidateRemoteIds
                            publishCountdown(deadline, candidateRemoteIds)
                        }
                    }
                    Cmd.StopCountdown -> {
                        deadline = null
                        scheduledAutoPushRemoteIds = emptySet()
                        publishCountdown(null, emptySet())
                    }
                    is Cmd.Push -> {
                        deadline = null
                        scheduledAutoPushRemoteIds = emptySet()
                        publishCountdown(null, emptySet())
                        cmd.ack.complete(push(cmd.remoteId))
                    }
                    is Cmd.Fetch -> cmd.ack.complete(fetch(cmd.remoteId))
                }
            }
        } finally {
            drainPendingAcks()
        }
    }

    // Edits made while a push is already scheduled ride along with it rather
    // than pushing the deadline back, so a steady stream of edits still gets
    // pushed within the window instead of starving.
    fun schedulePush() {
        commands.trySend(Cmd.Mutated)
    }

    // No effect on a push already running.
    fun stopScheduledPush() {
        commands.trySend(Cmd.StopCountdown)
    }

    fun setIncrementalImportState(state: IncrementalImportState) {
        _syncState.update { it.copy(incrementalImportState = state) }
    }

    /**
     * Pushes one remote, returning an error message or null on success,
     * mirroring Swift's LibraryModel.pushRemote. Clears any pending countdown
     * and queues behind a push or fetch already running.
     */
    suspend fun pushRemote(remoteId: String): String? {
        val ack = CompletableDeferred<String?>()
        commands.send(Cmd.Push(remoteId, ack))
        return ack.await()
    }

    /**
     * Fetches one remote, returning an error message or null on success,
     * mirroring Swift's LibraryModel.fetchRemote. Queues behind a push or
     * fetch already running.
     */
    suspend fun fetchRemoteWithResult(remoteId: String): String? {
        val ack = CompletableDeferred<String?>()
        commands.send(Cmd.Fetch(remoteId, ack))
        return ack.await()
    }

    // Stops the schedule and waits for a push or fetch in flight to finish,
    // since the caller is usually about to delete the library files it is
    // still reading.
    suspend fun close() {
        commands.close()
        loop.join()
    }

    private suspend fun pushScheduledRemotes(candidateRemoteIds: Set<String>) {
        for (remote in lib.listRemotes().filter { it.id in candidateRemoteIds && it.autoPush }) {
            push(remote.id)
        }
    }

    private suspend fun push(remoteId: String): String? {
        _syncState.update { it.copy(busyRemoteIds = it.busyRemoteIds + remoteId) }
        return try {
            lib.pushRemoteAsync(remoteId, null)
            prefs.recordPush(remoteId, success = true)
            null
        } catch (e: Exception) {
            prefs.recordPush(remoteId, success = false)
            e.message?.ifBlank { null } ?: "Push failed"
        } finally {
            _syncState.update { it.copy(busyRemoteIds = it.busyRemoteIds - remoteId) }
        }
    }

    private suspend fun fetch(remoteId: String): String? {
        _syncState.update { it.copy(busyRemoteIds = it.busyRemoteIds + remoteId, fetchInProgress = true) }
        return try {
            lib.fetchRemoteAsync(remoteId, null)
            prefs.recordFetch(remoteId, success = true)
            onLibraryChanged()
            null
        } catch (e: Exception) {
            prefs.recordFetch(remoteId, success = false)
            e.message?.ifBlank { null } ?: "Fetch failed"
        } finally {
            _syncState.update { it.copy(busyRemoteIds = it.busyRemoteIds - remoteId, fetchInProgress = false) }
        }
    }

    private fun publishCountdown(deadline: Long?, remoteIds: Set<String>) {
        _syncState.update {
            it.copy(
                pushDeadlineElapsedMs = deadline,
                scheduledAutoPushRemoteIds = remoteIds,
            )
        }
    }

    // A caller still awaiting an ack when the library closes under it must not
    // hang forever.
    private fun drainPendingAcks() {
        while (true) {
            val cmd = commands.tryReceive().getOrNull() ?: break
            when (cmd) {
                is Cmd.Push -> cmd.ack.complete("Library closed")
                is Cmd.Fetch -> cmd.ack.complete("Library closed")
                Cmd.Mutated, Cmd.StopCountdown -> {}
            }
        }
    }

    companion object {
        private const val PUSH_DELAY_MS = 30_000L
    }
}
