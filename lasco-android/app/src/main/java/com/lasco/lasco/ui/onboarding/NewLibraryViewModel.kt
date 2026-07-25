package com.lasco.lasco.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.LascoRepository
import com.lasco.lasco.data.LibraryRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class NewLibraryUiState(
    val loading: Boolean = false,
    val error: String? = null,
    val opened: Boolean = false,
)

/**
 * Creates a library, then opens it, since ffiCreateLibrary only returns ids,
 * not a usable FfiLibrary handle. Mirrors the create step of the Swift
 * NewLibraryWizard followed by LibraryModel.open.
 */
class NewLibraryViewModel(
    private val app: LascoApp,
    private val repository: LascoRepository,
) : ViewModel() {
    private val _uiState = MutableStateFlow(NewLibraryUiState())
    val uiState: StateFlow<NewLibraryUiState> = _uiState.asStateFlow()

    fun create(name: String, username: String, password: String) {
        _uiState.value = NewLibraryUiState(loading = true)
        viewModelScope.launch {
            try {
                repository.createLibrary(nickname = name, username = username, password = password)
                val lib = repository.openLibrary(nickname = name, username = username, password = password)
                app.librarySession =
                    LibraryRepository(lib, nickname = name, username = username, appDir = repository.appDir)
                _uiState.value = NewLibraryUiState(opened = true)
            } catch (e: Throwable) {
                _uiState.value = NewLibraryUiState(error = e.message ?: "Could not create library")
            }
        }
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY] as LascoApp
                NewLibraryViewModel(app, LascoRepository.from(app))
            }
        }
    }
}
