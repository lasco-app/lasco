package com.lasco.lasco.ui.status

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.ConfirmMediaResult
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.data.SessionState
import com.lasco.lasco.data.SyncState
import com.lasco.lasco.data.PushResult
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiLocalStateStats
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiRemoteMediaShortfall
import uniffi.lasco_ffi.FfiRemoteUuid

private val VIDEO_EXTENSIONS = setOf(
    "mp4", "mov", "avi", "mkv", "m4v", "wmv", "flv", "webm", "mpg", "mpeg", "3gp", "ts", "mts", "m2ts",
)

data class MediaTypeCounts(val photos: Int, val videos: Int)

fun List<FfiMediaItem>.mediaTypeCounts(): MediaTypeCounts {
    val videos = count { it.filenameOriginal.substringAfterLast('.', "").lowercase() in VIDEO_EXTENSIONS }
    return MediaTypeCounts(photos = size - videos, videos = videos)
}

/**
 * Backs StatusScreen. Push and fetch live on LibraryRepository.sync, this
 * tracks the per remote unpushed state the pink/red banner needs plus local
 * cache stats.
 */
class StatusViewModel(
    private val repo: LibraryRepository,
    private val prefs: Prefs,
) : ViewModel() {
    val media: StateFlow<List<FfiMediaItem>> =
        repo.watch(Change.MediaList) { repo.mediaByDateAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val sessionState: StateFlow<SessionState> = repo.sessionState

    val syncState: StateFlow<SyncState> = repo.sync.syncState

    fun isLascoCloudConnected(): Boolean = repo.isLascoCloudConnected()

    private val _unpushed = MutableStateFlow<Map<FfiRemoteUuid, Boolean>>(emptyMap())
    val unpushed: StateFlow<Map<FfiRemoteUuid, Boolean>> = _unpushed.asStateFlow()

    private val _shortfall = MutableStateFlow<Map<FfiRemoteUuid, FfiRemoteMediaShortfall>>(emptyMap())
    val shortfall: StateFlow<Map<FfiRemoteUuid, FfiRemoteMediaShortfall>> = _shortfall.asStateFlow()

    private val _localStateStats = MutableStateFlow<FfiLocalStateStats?>(null)
    val localStateStats: StateFlow<FfiLocalStateStats?> = _localStateStats.asStateFlow()

    init {
        viewModelScope.launch {
            sessionState.collect { state -> refreshUnpushed(state.remotes.map { it.remoteId }) }
        }
        viewModelScope.launch {
            repo.watch(Change.MediaList, Change.AlbumList) {}
                .collect { refreshUnpushed(sessionState.value.remotes.map { it.remoteId }) }
        }
        viewModelScope.launch {
            prefs.lastPush.collect { records -> refreshUnpushed(records.keys.toList()) }
        }
        refreshLocalStateStats()
    }

    suspend fun pushRemote(remoteId: FfiRemoteUuid): PushResult = repo.sync.pushRemote(remoteId)

    suspend fun confirmRemoteMedia(remoteId: FfiRemoteUuid): ConfirmMediaResult =
        repo.sync.confirmRemoteMedia(remoteId)

    suspend fun fetchRemote(remoteId: FfiRemoteUuid): String? {
        val result = repo.sync.fetchRemoteWithResult(remoteId)
        refreshUnpushed(listOf(remoteId))
        return result
    }

    private suspend fun refreshUnpushed(remoteIds: List<FfiRemoteUuid>) {
        val updates = remoteIds.associateWith { repo.hasUnpushedChanges(it) }
        _unpushed.value = _unpushed.value + updates
        // Media a remote has never been told about cannot be expected on it, so the shortfall
        // is only worth reading once every operation has reached it. That also keeps this off
        // the hot path during an import, where nothing is pushed.
        val settled = updates.filterValues { !it }.keys
        if (settled.isEmpty()) return
        val shortfalls = settled.associateWith { repo.remoteMediaShortfall(it) }
        _shortfall.value = _shortfall.value + shortfalls
    }

    fun refreshLocalStateStats() {
        viewModelScope.launch { _localStateStats.value = repo.localStateStats() }
    }

    // Queried when the user reaches for the action rather than on every refresh, because it
    // stats a file per media and is only ever read to answer that one question.
    suspend fun mediaCountLostIfLocalMediaCleared(): Int = repo.mediaCountLostIfLocalMediaCleared()

    fun cleanLocalMedia() {
        viewModelScope.launch {
            repo.evictLocalData()
            refreshLocalStateStats()
        }
    }

    fun cleanLocalThumbnails() {
        viewModelScope.launch {
            repo.evictLocalThumbnails()
            refreshLocalStateStats()
        }
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                StatusViewModel(LibraryRepository.from(app), Prefs.from(app))
            }
        }
    }
}
