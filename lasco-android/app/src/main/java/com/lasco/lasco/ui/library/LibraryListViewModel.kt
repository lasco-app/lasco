package com.lasco.lasco.ui.library

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.LascoRepository
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import java.util.UUID
import uniffi.lasco_ffi.FfiLibraryEntry

/** Immutable details needed by the credential screen after a cache miss. */
data class LibraryOpenRequest(
    val attemptId: String,
    val libraryId: String,
    val nickname: String,
    val username: String?,
)

sealed interface LibraryOpenDestination {
    data object Opened : LibraryOpenDestination
    data class CredentialsRequired(val request: LibraryOpenRequest) : LibraryOpenDestination
}

/**
 * UI state for the library list screen. The first real slice of the port, it
 * mirrors the Swift LibraryModel.libraries plus its loading and error fields.
 */
data class LibraryListUiState(
    val loading: Boolean = true,
    val libraries: List<FfiLibraryEntry> = emptyList(),
    val error: String? = null,
    val openingLibraryId: String? = null,
)

/**
 * Watches the list of libraries through the repository and exposes it as a
 * StateFlow the Compose screen collects. Reloads whenever the repository
 * signals a library was created, deleted or added, so the list never goes
 * stale after a mutation made elsewhere in the app.
 */
class LibraryListViewModel(
    private val app: LascoApp,
    private val repository: LascoRepository,
    private val prefs: Prefs,
) : ViewModel() {
    private val openingLibraryId = MutableStateFlow<String?>(null)
    private val _openDestinations = MutableSharedFlow<LibraryOpenDestination>(extraBufferCapacity = 1)
    val openDestinations = _openDestinations.asSharedFlow()

    val uiState: StateFlow<LibraryListUiState> =
        repository.watchLibraries()
            .map { libraries -> LibraryListUiState(loading = false, libraries = libraries) }
            .catch { e -> emit(LibraryListUiState(loading = false, error = e.message ?: "unknown error")) }
            .combine(openingLibraryId) { state, openingId -> state.copy(openingLibraryId = openingId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), LibraryListUiState())

    /**
     * Resolves the cache before choosing a destination. A credential screen is
     * only reachable after a miss, so it can never flash for an already-open
     * session.
     */
    fun open(entry: FfiLibraryEntry) {
        if (openingLibraryId.value != null) return

        val request = LibraryOpenRequest(
            attemptId = UUID.randomUUID().toString(),
            libraryId = entry.libraryId.value,
            nickname = entry.nickname,
            username = entry.username,
        )
        openingLibraryId.value = request.libraryId
        viewModelScope.launch {
            val destination = try {
                val username = request.username
                val library = if (username == null) null else repository.openCached(request.nickname, username)
                if (library == null || username == null) {
                    LibraryOpenDestination.CredentialsRequired(request)
                } else {
                    app.librarySession = LibraryRepository(
                        library,
                        nickname = request.nickname,
                        username = username,
                        appDir = repository.appDir,
                        context = app,
                        prefs = prefs,
                    )
                    LibraryOpenDestination.Opened
                }
            } catch (_: Throwable) {
                // Match the previous behavior: an unavailable cache falls back
                // to the password flow, which can surface a useful open error.
                LibraryOpenDestination.CredentialsRequired(request)
            }

            openingLibraryId.value = null
            _openDestinations.emit(destination)
        }
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY] as LascoApp
                LibraryListViewModel(app, LascoRepository.from(app), Prefs.from(app))
            }
        }
    }
}
