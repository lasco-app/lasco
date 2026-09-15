package app.lasco.importer

import app.lasco.importer.ffi.ImporterLibrarySummary
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertIs

class DestinationStateTest {
    private val library = ImporterLibrarySummary("library-id", "Family", null, emptyList(), null)

    @Test
    fun `loading setups clears only the list error`() {
        val state = DestinationUiState(
            loading = true,
            listError = "stale error",
            errorByLibraryId = mapOf(library.libraryId to "unlock failed"),
        ).loaded(listOf(library))

        assertFalse(state.loading)
        assertNull(state.listError)
        assertEquals(listOf(library), state.libraries)
        assertEquals("unlock failed", state.errorByLibraryId[library.libraryId])
    }

    @Test
    fun `per card errors do not overwrite another setup`() {
        val second = library.copy(libraryId = "second-id")
        val state = DestinationUiState()
            .withLibraryError(library.libraryId, "unlock failed")
            .withLibraryError(second.libraryId, "remote failed")
            .clearLibraryError(library.libraryId)

        assertNull(state.errorByLibraryId[library.libraryId])
        assertEquals("remote failed", state.errorByLibraryId[second.libraryId])
    }

    @Test
    fun `locked setup targets only that library's unlock dialog`() {
        val state = DestinationUiState(libraries = listOf(library)).copy(
            dialog = DestinationDialog.Unlock(library),
        )

        assertEquals(library.libraryId, assertIs<DestinationDialog.Unlock>(state.dialog).library.libraryId)
    }

    @Test
    fun `removal confirmation retains the requested setup identity`() {
        val state = DestinationUiState(libraries = listOf(library)).copy(
            dialog = DestinationDialog.RemoveSetup(library),
        )

        assertEquals(library.libraryId, assertIs<DestinationDialog.RemoveSetup>(state.dialog).library.libraryId)
    }
}
