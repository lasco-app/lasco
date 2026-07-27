package com.lasco.lasco.ui.manage

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.data.SessionState
import com.lasco.lasco.data.SyncState
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiAlbum

/**
 * Backs ManageScreen and its Remotes/Users/Settings children. Sign out and
 * delete both clear LascoApp.librarySession, mirroring Swift's LibraryModel
 * resetting lib to nil, the caller then navigates back to the library list.
 */
class ManageViewModel(
    private val repo: LibraryRepository,
    private val app: LascoApp,
    val prefs: Prefs,
) : ViewModel() {
    val sessionState: StateFlow<SessionState> = repo.sessionState

    val syncState: StateFlow<SyncState> = repo.sync.syncState

    val albums: StateFlow<List<FfiAlbum>> =
        repo.watch(Change.AlbumList) { repo.listAlbums() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun setDefaultUploadAlbum(albumId: String?) {
        viewModelScope.launch { repo.setDefaultUploadAlbum(albumId) }
    }

    // Close first, it waits for a push in flight. Signing out or deleting
    // under a running push would pull the library out from under it.
    suspend fun signOut() {
        val state = sessionState.value
        repo.close()
        app.repository.signOut(state.libraryId, state.username ?: "")
        app.librarySession = null
    }

    suspend fun deleteLibrary() {
        val state = sessionState.value
        repo.close()
        app.repository.deleteLibrary(state.libraryId)
        app.librarySession = null
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
