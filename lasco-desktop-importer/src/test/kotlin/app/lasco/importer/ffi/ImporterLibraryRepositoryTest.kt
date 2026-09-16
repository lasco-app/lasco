package app.lasco.importer.ffi

import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class ImporterLibraryRepositoryTest {
    @Test
    fun `failed initialization rolls back only its new remote`() = runBlocking {
        var removed = 0

        val failure = assertFailsWith<IllegalStateException> {
            initializeRemoteOrRollback(
                initialize = { error("initialization failed") },
                remove = { removed += 1 },
            )
        }

        assertEquals("initialization failed", failure.message)
        assertEquals(1, removed)
    }
}
