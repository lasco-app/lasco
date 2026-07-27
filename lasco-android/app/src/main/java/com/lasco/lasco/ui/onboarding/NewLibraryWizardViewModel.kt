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
import com.lasco.lasco.data.Prefs
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class NewLibraryWizardUiState(
    val loading: Boolean = false,
    val error: String? = null,
    val libraryId: String? = null,
    val nickname: String? = null,
    val masterKeyHex: String? = null,
)

/**
 * Backs NewLibraryWizardScreen, ported from the createStep/masterKeyStep of
 * the Swift NewLibraryWizard. When resuming an interrupted wizard, libraryId
 * and nickname are pre-populated from the caller instead of being produced
 * by create(), since the master key cannot be recovered after a process
 * restart, that step is always skipped on resume.
 */
class NewLibraryWizardViewModel(
    private val app: LascoApp,
    private val repository: LascoRepository,
    private val prefs: Prefs,
    resumeLibraryId: String?,
    resumeNickname: String?,
) : ViewModel() {
    private val _uiState = MutableStateFlow(
        NewLibraryWizardUiState(libraryId = resumeLibraryId, nickname = resumeNickname),
    )
    val uiState: StateFlow<NewLibraryWizardUiState> = _uiState.asStateFlow()

    fun create(name: String, username: String, password: String) {
        _uiState.value = _uiState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            try {
                val result = repository.createLibrary(nickname = name, username = username, password = password)
                val lib = repository.openLibrary(nickname = name, username = username, password = password)
                app.librarySession =
                    LibraryRepository(lib, nickname = name, username = username, appDir = repository.appDir, prefs = prefs)
                _uiState.value = NewLibraryWizardUiState(
                    libraryId = result.libraryId,
                    nickname = name,
                    masterKeyHex = result.masterKeyHex,
                )
            } catch (e: Throwable) {
                _uiState.value = _uiState.value.copy(loading = false, error = e.message ?: "Could not create library")
            }
        }
    }

    fun clearMasterKey() {
        _uiState.value = _uiState.value.copy(masterKeyHex = null)
    }

    fun recordStep(step: Int) {
        val libraryId = _uiState.value.libraryId ?: return
        prefs.setOnboardingStep(libraryId, step)
    }

    fun finish() {
        val libraryId = _uiState.value.libraryId ?: return
        prefs.clearOnboardingIncomplete(libraryId)
    }

    companion object {
        fun factory(resumeLibraryId: String?, resumeNickname: String?): ViewModelProvider.Factory =
            viewModelFactory {
                initializer {
                    val app = this[APPLICATION_KEY] as LascoApp
                    NewLibraryWizardViewModel(
                        app,
                        LascoRepository.from(app),
                        Prefs.from(app),
                        resumeLibraryId,
                        resumeNickname,
                    )
                }
            }
    }
}
