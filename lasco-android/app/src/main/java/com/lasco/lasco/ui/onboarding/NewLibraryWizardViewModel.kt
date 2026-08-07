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
import com.lasco.lasco.data.WizardCheckpoint
import com.lasco.lasco.media.DeviceScan
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiLibraryId

sealed interface WizardStep {
    data object CreateLibrary : WizardStep
    data object SaveRecoveryKey : WizardStep
    data object AddRemote : WizardStep
    data object ChooseDeviceImport : WizardStep
    data object GrantMediaAccess : WizardStep
    data object GrantLocationAccess : WizardStep
    data object ImportDeviceMedia : WizardStep
    data object ChooseAutoImport : WizardStep
}

enum class BackResult { ExitWizard, Consumed, NoOp }

data class WizardUiState(
    val step: WizardStep = WizardStep.CreateLibrary,
    val slideForward: Boolean = true,
    val loading: Boolean = false,
    val error: String? = null,
    val libraryId: String? = null,
    val nickname: String? = null,
    val masterKeyHex: String? = null,
    val deviceScan: DeviceScan? = null,
    val scanning: Boolean = false,
    val importState: ImportState = ImportState.Idle,
    val isImporting: Boolean = false,
)

fun WizardStep.progressIndex() = when (this) {
    WizardStep.CreateLibrary -> 0
    WizardStep.SaveRecoveryKey -> 1
    WizardStep.AddRemote -> 2
    WizardStep.ChooseDeviceImport -> 3
    WizardStep.GrantMediaAccess -> 4
    WizardStep.GrantLocationAccess -> 5
    WizardStep.ImportDeviceMedia -> 6
    WizardStep.ChooseAutoImport -> 7
}

class NewLibraryWizardViewModel(
    private val app: LascoApp,
    private val repository: LascoRepository,
    private val prefs: Prefs,
) : ViewModel() {
    private val _uiState = MutableStateFlow(WizardUiState())
    val uiState: StateFlow<WizardUiState> = _uiState.asStateFlow()

    private var sessionId: String? = null
    private var importStateJob: Job? = null
    private var initialImportController: InitialImportController? = null

    fun startFresh(sessionId: String) {
        if (this.sessionId == sessionId) return
        cancel()
        this.sessionId = sessionId
        app.librarySession = null
        _uiState.value = WizardUiState()
    }

    fun resume(sessionId: String, libraryId: String, nickname: String, checkpoint: WizardCheckpoint) {
        if (this.sessionId == sessionId) return
        reset(clearAppSession = false)
        this.sessionId = sessionId
        val step = checkpoint.toStep()
        _uiState.value = WizardUiState(step = step, libraryId = libraryId, nickname = nickname)
        observeImportState()
    }

    fun createLibrary(name: String, username: String, password: String) {
        if (_uiState.value.step != WizardStep.CreateLibrary || _uiState.value.loading) return
        _uiState.value = _uiState.value.copy(loading = true, error = null)
        viewModelScope.launch {
            try {
                val result = repository.createLibrary(nickname = name, username = username, password = password)
                val lib = repository.openLibrary(nickname = name, username = username, password = password)
                app.librarySession = LibraryRepository(
                    lib,
                    nickname = name,
                    username = username,
                    appDir = repository.appDir,
                    context = app,
                    prefs = prefs,
                )
                _uiState.value = WizardUiState(
                    step = WizardStep.SaveRecoveryKey,
                    libraryId = result.libraryId.value,
                    nickname = name,
                    masterKeyHex = result.masterKeyHex,
                )
                observeImportState()
            } catch (e: Throwable) {
                _uiState.value = _uiState.value.copy(loading = false, error = e.message ?: "Could not create library")
            }
        }
    }

    fun confirmRecoveryKey() {
        _uiState.value = _uiState.value.copy(masterKeyHex = null)
        moveTo(WizardStep.AddRemote)
    }
    fun remoteCompleted() = moveTo(WizardStep.ChooseDeviceImport)
    fun skipRemote() = moveTo(WizardStep.ChooseDeviceImport)
    fun chooseDeviceImport() = moveTo(WizardStep.GrantMediaAccess)
    fun mediaAccessGranted() = moveTo(WizardStep.GrantLocationAccess)
    fun locationAccessGranted() = moveTo(WizardStep.ImportDeviceMedia)

    fun skipDeviceImport() {
        _uiState.value.libraryId?.let { prefs.baselineImportWatermark(FfiLibraryId(it)) }
        moveTo(WizardStep.ChooseAutoImport)
    }

    fun importCompleted() = moveTo(WizardStep.ChooseAutoImport)

    fun setAutoImport(enabled: Boolean, onSuccess: () -> Unit) {
        viewModelScope.launch {
            try {
                app.librarySession?.setAutoImportDeviceMedia(enabled)
                _uiState.value.libraryId?.let { prefs.clearOnboardingIncomplete(FfiLibraryId(it)) }
                complete()
                onSuccess()
            } catch (e: Throwable) {
                _uiState.value = _uiState.value.copy(error = e.message ?: "Could not save auto-import setting")
            }
        }
    }

    fun scanDeviceMedia() {
        val controller = controllerOrNull() ?: return
        _uiState.value = _uiState.value.copy(scanning = true, error = null)
        viewModelScope.launch {
            try {
                val scan = controller.scan()
                _uiState.value = _uiState.value.copy(scanning = false, deviceScan = scan)
            } catch (e: Throwable) {
                _uiState.value = _uiState.value.copy(scanning = false, error = e.message ?: "Could not scan device media")
            }
        }
    }

    fun startDeviceImport() {
        controllerOrNull()?.let { controller -> viewModelScope.launch { controller.runInitialImport() } }
    }

    fun back(): BackResult {
        val current = _uiState.value
        if (current.step == WizardStep.CreateLibrary) return BackResult.ExitWizard
        return BackResult.NoOp
    }

    fun cancel() {
        reset(clearAppSession = true)
    }

    private fun reset(clearAppSession: Boolean) {
        initialImportController?.cancel()
        importStateJob?.cancel()
        importStateJob = null
        initialImportController = null
        if (clearAppSession) app.librarySession = null
        _uiState.value = WizardUiState()
    }

    fun complete() {
        _uiState.value.libraryId?.let { prefs.clearOnboardingIncomplete(FfiLibraryId(it)) }
        initialImportController?.cancel()
        importStateJob?.cancel()
        importStateJob = null
        initialImportController = null
        _uiState.value = WizardUiState()
        sessionId = null
    }

    private fun moveTo(next: WizardStep) {
        val current = _uiState.value
        _uiState.value = current.copy(
            step = next,
            slideForward = next.progressIndex() > current.step.progressIndex(),
            masterKeyHex = current.masterKeyHex.takeIf { next == WizardStep.SaveRecoveryKey },
        )
        val libraryId = current.libraryId ?: return
        next.toCheckpoint()?.let { prefs.setOnboardingCheckpoint(FfiLibraryId(libraryId), it) }
            ?: prefs.clearOnboardingIncomplete(FfiLibraryId(libraryId))
    }

    private fun controllerOrNull(): InitialImportController? {
        if (initialImportController == null) {
            val session = app.librarySession ?: return null
            initialImportController = InitialImportController(
                lib = session.ffiLibraryForOnboardingImport(),
                context = app,
                prefs = prefs,
                sync = session.sync,
                onLibraryChanged = { session.notifyChanged() },
                scope = viewModelScope,
            )
            observeImportState()
        }
        return initialImportController
    }

    private fun observeImportState() {
        val controller = initialImportController ?: return
        if (importStateJob?.isActive == true) return
        importStateJob = viewModelScope.launch {
            controller.importState.collect { importState ->
                _uiState.value = _uiState.value.copy(
                    importState = importState,
                    isImporting = importState is ImportState.Importing,
                )
            }
        }
    }

    private fun WizardCheckpoint.toStep() = when (this) {
        WizardCheckpoint.AddRemote -> WizardStep.AddRemote
        WizardCheckpoint.ChooseDeviceImport -> WizardStep.ChooseDeviceImport
        WizardCheckpoint.GrantMediaAccess -> WizardStep.GrantMediaAccess
        WizardCheckpoint.GrantLocationAccess -> WizardStep.GrantLocationAccess
        WizardCheckpoint.ImportDeviceMedia -> WizardStep.ImportDeviceMedia
        WizardCheckpoint.ChooseAutoImport -> WizardStep.ChooseAutoImport
    }

    private fun WizardStep.toCheckpoint() = when (this) {
        WizardStep.AddRemote -> WizardCheckpoint.AddRemote
        WizardStep.ChooseDeviceImport -> WizardCheckpoint.ChooseDeviceImport
        WizardStep.GrantMediaAccess -> WizardCheckpoint.GrantMediaAccess
        WizardStep.GrantLocationAccess -> WizardCheckpoint.GrantLocationAccess
        WizardStep.ImportDeviceMedia -> WizardCheckpoint.ImportDeviceMedia
        WizardStep.ChooseAutoImport -> WizardCheckpoint.ChooseAutoImport
        WizardStep.CreateLibrary, WizardStep.SaveRecoveryKey -> null
    }

    companion object {
        fun factory(): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY] as LascoApp
                NewLibraryWizardViewModel(app, LascoRepository.from(app), Prefs.from(app))
            }
        }
    }
}
