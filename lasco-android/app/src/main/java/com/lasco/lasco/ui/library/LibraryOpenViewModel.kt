package com.lasco.lasco.ui.library

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.LascoRepository
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.LascoException

data class LibraryOpenUiState(
    val loading: Boolean = false,
    val error: String? = null,
    val opened: Boolean = false,
    val recoveryAvailable: Boolean = false,
)

/**
 * Opens a library after the list flow has already established that no cached
 * session is available. Cache resolution belongs to LibraryListViewModel so
 * this screen is only ever composed for a credential prompt.
 */
class LibraryOpenViewModel(
    private val app: LascoApp,
    private val repository: LascoRepository,
    private val prefs: Prefs,
) : ViewModel() {
    private val _uiState = MutableStateFlow(LibraryOpenUiState())
    val uiState: StateFlow<LibraryOpenUiState> = _uiState.asStateFlow()

    fun open(nickname: String, username: String, password: String) {
        _uiState.value = _uiState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            try {
                val lib = repository.openLibrary(nickname = nickname, username = username, password = password)
                app.librarySession =
                    LibraryRepository(lib, nickname = nickname, username = username, appDir = repository.appDir, context = app, prefs = prefs)
                _uiState.value = _uiState.value.copy(loading = false, opened = true)
            } catch (e: Throwable) {
                _uiState.value = _uiState.value.copy(
                    loading = false,
                    error = e.message ?: "Could not open library",
                    recoveryAvailable = e is LascoException.CrdtRecoveryAvailable,
                )
            }
        }
    }

    fun recover(nickname: String, username: String, password: String) {
        _uiState.value = _uiState.value.copy(loading = true, error = null, recoveryAvailable = false)
        viewModelScope.launch {
            try {
                repository.recoverLibraryState(nickname, username, password)
                open(nickname, username, password)
            } catch (e: Throwable) {
                _uiState.value = _uiState.value.copy(loading = false, error = e.message ?: "Could not recover library state")
            }
        }
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY] as LascoApp
                LibraryOpenViewModel(app, LascoRepository.from(app), Prefs.from(app))
            }
        }
    }
}
