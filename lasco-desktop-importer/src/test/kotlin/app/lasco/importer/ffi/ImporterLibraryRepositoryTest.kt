package app.lasco.importer.ffi

import kotlinx.coroutines.runBlocking
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class ImporterLibraryRepositoryTest {
    @Test
    fun `bundled native library can list an empty importer directory`() = runBlocking {
        val directory = Files.createTempDirectory("lasco-native-loading-")
        try {
            UniffiImporterLibraryRepository(directory).use { repository ->
                assertTrue(repository.list().isEmpty())
            }
        } finally {
            directory.toFile().deleteRecursively()
        }
    }

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
