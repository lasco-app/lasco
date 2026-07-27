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
import uniffi.lasco_ffi.FfiLibraryEntry

data class LibraryOpenUiState(
    val checkingCache: Boolean = true,
    val loading: Boolean = false,
    val error: String? = null,
    val opened: Boolean = false,
)

/**
 * Opens one library entry from the library list. Mirrors LibraryModel.openCached
 * followed by LibraryOpenSheet, tries the cached session first and only asks
 * for a password when that comes back empty. Takes the entry id rather than
 * an FfiLibraryEntry, since Nav3 keys must not carry FFI structs, and resolves
 * it back to the full entry here.
 */
class LibraryOpenViewModel(
    private val app: LascoApp,
    private val repository: LascoRepository,
    private val prefs: Prefs,
    entryId: String,
) : ViewModel() {
    private val _uiState = MutableStateFlow(LibraryOpenUiState())
    val uiState: StateFlow<LibraryOpenUiState> = _uiState.asStateFlow()

    private val _entry = MutableStateFlow<FfiLibraryEntry?>(null)
    val entry: StateFlow<FfiLibraryEntry?> = _entry.asStateFlow()

    init {
        viewModelScope.launch {
            val found = repository.listLibraries().firstOrNull { it.id == entryId }
            if (found == null) {
                _uiState.value = LibraryOpenUiState(checkingCache = false, error = "Library not found")
            } else {
                _entry.value = found
            }
        }
    }

    fun tryOpenCached(nickname: String, username: String?) {
        if (username == null) {
            _uiState.value = LibraryOpenUiState(checkingCache = false)
            return
        }
        viewModelScope.launch {
            try {
                val lib = repository.openCached(nickname = nickname, username = username)
                if (lib != null) {
                    app.librarySession =
                        LibraryRepository(lib, nickname = nickname, username = username, appDir = repository.appDir, prefs = prefs)
                    _uiState.value = LibraryOpenUiState(checkingCache = false, opened = true)
                } else {
                    _uiState.value = LibraryOpenUiState(checkingCache = false)
                }
            } catch (e: Throwable) {
                _uiState.value = LibraryOpenUiState(checkingCache = false)
            }
        }
    }

    fun open(nickname: String, username: String, password: String) {
        _uiState.value = _uiState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            try {
                val lib = repository.openLibrary(nickname = nickname, username = username, password = password)
                app.librarySession =
                    LibraryRepository(lib, nickname = nickname, username = username, appDir = repository.appDir, prefs = prefs)
                _uiState.value = _uiState.value.copy(loading = false, opened = true)
            } catch (e: Throwable) {
                _uiState.value = _uiState.value.copy(loading = false, error = e.message ?: "Could not open library")
            }
        }
    }

    companion object {
        fun factory(entryId: String): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY] as LascoApp
                LibraryOpenViewModel(app, LascoRepository.from(app), Prefs.from(app), entryId)
            }
        }
    }
}
