package com.lasco.lasco.ui.status

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.data.SessionState
import com.lasco.lasco.data.SyncState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiLocalStateStats
import uniffi.lasco_ffi.FfiMediaItem

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
        repo.watch(Change.MediaList) { repo.mediaByDate() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val sessionState: StateFlow<SessionState> = repo.sessionState

    val syncState: StateFlow<SyncState> = repo.sync.syncState

    private val _unpushed = MutableStateFlow<Map<String, Boolean>>(emptyMap())
    val unpushed: StateFlow<Map<String, Boolean>> = _unpushed.asStateFlow()

    private val _localStateStats = MutableStateFlow<FfiLocalStateStats?>(null)
    val localStateStats: StateFlow<FfiLocalStateStats?> = _localStateStats.asStateFlow()

    init {
        viewModelScope.launch {
            sessionState.collect { state -> refreshUnpushed(state.remotes.map { it.id }) }
        }
        viewModelScope.launch {
            repo.watch(Change.MediaList, Change.AlbumList) {}
                .collect { refreshUnpushed(sessionState.value.remotes.map { it.id }) }
        }
        viewModelScope.launch {
            prefs.lastPush.collect { records -> refreshUnpushed(records.keys.toList()) }
        }
        refreshLocalStateStats()
    }

    suspend fun pushRemote(remoteId: String): String? = repo.sync.pushRemote(remoteId)

    suspend fun fetchRemote(remoteId: String): String? {
        val result = repo.sync.fetchRemoteWithResult(remoteId)
        refreshUnpushed(listOf(remoteId))
        return result
    }

    private suspend fun refreshUnpushed(remoteIds: List<String>) {
        val updates = remoteIds.associateWith { repo.hasUnpushedChanges(it) }
        _unpushed.value = _unpushed.value + updates
    }

    fun refreshLocalStateStats() {
        viewModelScope.launch { _localStateStats.value = repo.localStateStats() }
    }

    suspend fun mediaCountWithoutRemoteBackup(): Int? =
        repo.mediaIdsWithoutRemoteBackup().size.takeIf { it > 0 }

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
