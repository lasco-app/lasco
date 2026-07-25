package com.lasco.lasco.ui.library

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import com.lasco.lasco.data.LascoRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiLibraryEntry

/**
 * UI state for the library list screen. The first real slice of the port, it
 * mirrors the Swift LibraryModel.libraries plus its loading and error fields.
 */
data class LibraryListUiState(
    val loading: Boolean = true,
    val libraries: List<FfiLibraryEntry> = emptyList(),
    val error: String? = null,
)

/**
 * Loads the list of libraries through the repository and exposes it as a
 * StateFlow the Compose screen collects. This is the standard Compose state
 * holder, one screen one ViewModel, replacing the throwaway FfiStatus probe.
 */
class LibraryListViewModel(
    private val repository: LascoRepository,
) : ViewModel() {
    private val _uiState = MutableStateFlow(LibraryListUiState())
    val uiState: StateFlow<LibraryListUiState> = _uiState.asStateFlow()

    init {
        refresh()
    }

    fun refresh() {
        _uiState.value = _uiState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            try {
                val libraries = repository.listLibraries()
                _uiState.value = LibraryListUiState(loading = false, libraries = libraries)
            } catch (e: Throwable) {
                _uiState.value = LibraryListUiState(loading = false, error = e.message ?: "unknown error")
            }
        }
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                LibraryListViewModel(LascoRepository.from(app))
            }
        }
    }
}
