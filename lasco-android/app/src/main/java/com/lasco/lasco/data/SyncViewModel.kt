package com.lasco.lasco.data

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Holds the transient sync and import state pulled out of the Swift
 * LibraryModel (busyRemotes, fetchInProgress, bulkImportProgress). This is
 * operational state, not session identity, so it lives here rather than on
 * SessionState. Also records push/fetch results into Prefs, the Android
 * equivalent of Swift's lastPushRecords/lastFetchRecords.
 */
class SyncViewModel(
    private val repository: LibraryRepository,
    private val prefs: Prefs,
) : ViewModel() {
    private val _syncState = MutableStateFlow(SyncState())
    val syncState: StateFlow<SyncState> = _syncState.asStateFlow()

    fun sync(appSupportDir: String? = null) {
        viewModelScope.launch {
            _syncState.value = _syncState.value.copy(fetchInProgress = true)
            try {
                val result = repository.sync(appSupportDir)
                _syncState.value = _syncState.value.copy(fetchInProgress = false, lastSyncResult = result)
            } catch (e: Throwable) {
                _syncState.value = _syncState.value.copy(fetchInProgress = false)
            }
        }
    }

    /**
     * Pushes one remote, returning an error message on failure or null on
     * success, mirroring Swift's LibraryModel.pushRemote.
     */
    suspend fun pushRemote(remoteId: String, appSupportDir: String? = null): String? {
        _syncState.value = _syncState.value.copy(busyRemoteIds = _syncState.value.busyRemoteIds + remoteId)
        return try {
            repository.pushRemote(remoteId, appSupportDir)
            prefs.recordPush(remoteId, success = true)
            null
        } catch (e: Exception) {
            prefs.recordPush(remoteId, success = false)
            e.message?.ifBlank { null } ?: "Push failed"
        } finally {
            _syncState.value = _syncState.value.copy(busyRemoteIds = _syncState.value.busyRemoteIds - remoteId)
        }
    }

    /**
     * Fetches one remote, returning an error message on failure or null on
     * success, mirroring Swift's LibraryModel.fetchRemote.
     */
    suspend fun fetchRemoteWithResult(remoteId: String, appSupportDir: String? = null): String? {
        _syncState.value = _syncState.value.copy(
            busyRemoteIds = _syncState.value.busyRemoteIds + remoteId,
            fetchInProgress = true,
        )
        return try {
            repository.fetchRemote(remoteId, appSupportDir)
            prefs.recordFetch(remoteId, success = true)
            null
        } catch (e: Exception) {
            prefs.recordFetch(remoteId, success = false)
            e.message?.ifBlank { null } ?: "Fetch failed"
        } finally {
            _syncState.value = _syncState.value.copy(
                busyRemoteIds = _syncState.value.busyRemoteIds - remoteId,
                fetchInProgress = false,
            )
        }
    }

    fun fetchRemote(remoteId: String, appSupportDir: String? = null) {
        viewModelScope.launch { fetchRemoteWithResult(remoteId, appSupportDir) }
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                SyncViewModel(LibraryRepository.from(app), Prefs.from(app))
            }
        }
    }
}
