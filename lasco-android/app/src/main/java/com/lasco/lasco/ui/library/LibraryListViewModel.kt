package com.lasco.lasco.ui.library

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import com.lasco.lasco.data.LascoRepository
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
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
 * Watches the list of libraries through the repository and exposes it as a
 * StateFlow the Compose screen collects. Reloads whenever the repository
 * signals a library was created, deleted or added, so the list never goes
 * stale after a mutation made elsewhere in the app.
 */
class LibraryListViewModel(
    repository: LascoRepository,
) : ViewModel() {
    val uiState: StateFlow<LibraryListUiState> =
        repository.watchLibraries()
            .map { libraries -> LibraryListUiState(loading = false, libraries = libraries) }
            .catch { e -> emit(LibraryListUiState(loading = false, error = e.message ?: "unknown error")) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), LibraryListUiState())

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                LibraryListViewModel(LascoRepository.from(app))
            }
        }
    }
}
