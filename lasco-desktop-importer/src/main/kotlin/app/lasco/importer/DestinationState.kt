package app.lasco.importer

import app.lasco.importer.ffi.ImporterLibrarySummary

/** Renderable destination state. Handles and credentials deliberately stay in the repository. */
internal data class DestinationUiState(
    val libraries: List<ImporterLibrarySummary> = emptyList(),
    val loading: Boolean = true,
    val dialog: DestinationDialog? = null,
    val errorByLibraryId: Map<String, String> = emptyMap(),
    val listError: String? = null,
)

internal sealed interface DestinationDialog {
    data class Unlock(val library: ImporterLibrarySummary) : DestinationDialog
    data class AddRemote(val library: ImporterLibrarySummary) : DestinationDialog
    data class RemoveSetup(val library: ImporterLibrarySummary) : DestinationDialog
}

internal fun DestinationUiState.loaded(libraries: List<ImporterLibrarySummary>) = copy(
    libraries = libraries,
    loading = false,
    listError = null,
)

internal fun DestinationUiState.listFailed(message: String) = copy(loading = false, listError = message)

internal fun DestinationUiState.withLibraryError(libraryId: String, message: String) = copy(
    errorByLibraryId = errorByLibraryId + (libraryId to message),
)

internal fun DestinationUiState.clearLibraryError(libraryId: String) = copy(
    errorByLibraryId = errorByLibraryId - libraryId,
)
