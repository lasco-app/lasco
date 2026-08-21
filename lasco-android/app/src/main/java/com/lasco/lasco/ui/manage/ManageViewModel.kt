package com.lasco.lasco.ui.manage

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.CloudAccount
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.data.SessionState
import com.lasco.lasco.data.SyncState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.async
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.launch

/**
 * Backs ManageScreen and its Remotes/Users/Settings children. Sign-out clears
 * LascoApp.librarySession; library deletion is coordinated by LascoRoot after
 * the opened-library UI has been removed from composition.
 */
class ManageViewModel(
    private val repo: LibraryRepository,
    private val app: LascoApp,
    val prefs: Prefs,
) : ViewModel() {
    val sessionState: StateFlow<SessionState> = repo.sessionState

    val syncState: StateFlow<SyncState> = repo.sync.syncState

    // Close first, it waits for a push in flight. Signing out or deleting
    // under a running push would pull the library out from under it.
    suspend fun signOut() {
        val state = sessionState.value
        repo.close()
        app.repository.signOut(state.libraryId, state.username ?: "")
        app.librarySession = null
    }

    fun setAutoImportDeviceMedia(enabled: Boolean) {
        viewModelScope.launch {
            repo.setAutoImportDeviceMedia(enabled)
        }
    }

    suspend fun lascoCloudSubscription(): CloudAccount = repo.lascoCloudSubscription()

    fun setRemoteAutoPush(remoteId: uniffi.lasco_ffi.FfiRemoteUuid, enabled: Boolean) {
        viewModelScope.launch {
            repo.setRemoteAutoPush(remoteId, enabled)
        }
    }

    fun setMediaSourceOrder(remoteIds: List<uniffi.lasco_ffi.FfiRemoteUuid>): Deferred<Unit> =
        viewModelScope.async {
            repo.setMediaSourceOrder(remoteIds)
        }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY] as LascoApp
                ManageViewModel(LibraryRepository.from(app), app, Prefs.from(app))
            }
        }
    }
}
