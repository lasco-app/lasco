package app.lasco.importer.persistence

import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportSource
import app.lasco.importer.model.ResourceRole
import app.lasco.importer.model.SourceMetadata
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ImportManifestStoreTest {
    @Test
    fun `persists resumable import state in JSON`() {
        val directory = Files.createTempDirectory("lasco-importer-test")
        val asset = ImportAsset(
            sourceId = "photos://asset-1",
            source = ImportSource.APPLE_PHOTOS,
            resourceRole = ResourceRole.PRIMARY,
            displayName = "IMG_0001.HEIC",
            byteCount = 42,
            metadata = SourceMetadata("IMG_0001.HEIC"),
            sourceLocator = "asset-1",
        )

        ImportManifestStore(directory).use { store ->
            store.createJob("job", ImportSource.APPLE_PHOTOS, 32)
            store.recordDiscovery("job", listOf(asset))
            store.markImported("job", asset.sourceId, "media-1", false, 1)
            store.markRemoteChunkComplete("job", 1, "remote-1")
            store.requestPause("job")
            assertTrue(store.consumePauseRequest("job"))
        }

        ImportManifestStore(directory).use { restored ->
            assertEquals(1, restored.totalCount("job"))
            assertEquals(1, restored.completedCount("job"))
            assertEquals("media-1", restored.mediaIdFor("job", asset.sourceId))
            assertEquals(listOf(1), restored.importedChunks("job"))
            assertTrue(restored.remoteChunkCompleted("job", 1, "remote-1"))
            assertFalse(restored.consumePauseRequest("job"))
            assertTrue(Files.exists(directory.resolve("importer.json")))
        }
    }
}
