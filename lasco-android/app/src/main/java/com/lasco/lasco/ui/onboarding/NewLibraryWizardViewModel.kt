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
import com.lasco.lasco.media.DeviceScan
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
    val deviceScan: DeviceScan? = null,
    val scanning: Boolean = false,
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
                    LibraryRepository(lib, nickname = name, username = username, appDir = repository.appDir, context = app, prefs = prefs)
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

    // Built lazily rather than at construction, since app.librarySession is
    // only set once create() has opened the library, the wizard steps before
    // the import one. Owned here and run on viewModelScope, safe because the
    // wizard screen blocks back navigation for the entire time an import is
    // in progress, so this ViewModel cannot be cleared mid run.
    private val initialImportController: InitialImportController by lazy {
        val session = app.librarySession ?: error("No library session")
        InitialImportController(
            lib = session.ffiLibraryForOnboardingImport(),
            context = app,
            prefs = prefs,
            sync = session.sync,
            onLibraryChanged = { session.notifyChanged() },
            scope = viewModelScope,
        )
    }

    // Exposed for the screen to collect directly, since it updates far more
    // often (once per imported item) than a plain uiState copy would want to.
    val deviceImportState: StateFlow<ImportState>
        get() = initialImportController.importState

    fun scanDeviceMedia() {
        _uiState.value = _uiState.value.copy(scanning = true)
        viewModelScope.launch {
            val scan = initialImportController.scan()
            _uiState.value = _uiState.value.copy(scanning = false, deviceScan = scan)
        }
    }

    fun startDeviceImport() {
        viewModelScope.launch { initialImportController.runInitialImport() }
    }

    // Both skip paths still lead to the auto-import question, so the user can
    // turn it on having imported nothing. Stamping the watermark here is what
    // keeps that from meaning the entire camera folder.
    fun skipDeviceImport() {
        val libraryId = _uiState.value.libraryId ?: return
        prefs.baselineImportWatermark(libraryId)
    }

    fun setAutoImportDeviceMedia(enabled: Boolean) {
        viewModelScope.launch { app.librarySession?.setAutoImportDeviceMedia(enabled) }
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
