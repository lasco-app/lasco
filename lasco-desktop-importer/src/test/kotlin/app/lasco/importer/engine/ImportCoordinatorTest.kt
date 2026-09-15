package app.lasco.importer.engine

import app.lasco.importer.ffi.LascoGateway
import app.lasco.importer.ffi.LascoRemote
import app.lasco.importer.model.ApplePhotosAssetRevision
import app.lasco.importer.model.ApplePhotosResourceDescriptor
import app.lasco.importer.model.ApplePhotosResourceType
import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportRunState
import app.lasco.importer.model.ImportSource
import app.lasco.importer.model.ImportedMedia
import app.lasco.importer.model.RemoteBenchmark
import app.lasco.importer.model.ResourceRole
import app.lasco.importer.model.SourceMetadata
import app.lasco.importer.model.StagedAsset
import app.lasco.importer.source.ImportSourceReader
import kotlinx.coroutines.runBlocking
import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals

class ImportCoordinatorTest {
    @Test
    fun `pause is process local and final cleanup waits for every asset`() = runBlocking {
        val gateway = FakeGateway()
        var finalizations = 0
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")) {
            finalizations += 1
            null
        }

        coordinator.discover(Reader(listOf(asset("one"), asset("two"))), chunkSize = 1)
        coordinator.requestPause()
        coordinator.startOrResume(emptyList())

        assertEquals(1, gateway.imported.size)
        assertEquals(0, finalizations)
        assertEquals(ImportRunState.PAUSED, coordinator.progress.value.state)

        coordinator.startOrResume(emptyList())

        assertEquals(listOf("one", "two"), gateway.imported)
        assertEquals(1, finalizations)
        assertEquals(ImportRunState.COMPLETE, coordinator.progress.value.state)
        assertEquals(3, gateway.pushes)
    }

    @Test
    fun `cleanup failure does not turn a complete import into an import failure`() = runBlocking {
        val coordinator = ImportCoordinator(FakeGateway(), Files.createTempDirectory("lasco-stage")) {
            "Could not remove the temporary setup"
        }

        coordinator.discover(Reader(listOf(asset("one"))), chunkSize = 1)
        coordinator.startOrResume(emptyList())

        assertEquals(ImportRunState.COMPLETE_WITH_CLEANUP_WARNING, coordinator.progress.value.state)
        assertEquals("Could not remove the temporary setup", coordinator.progress.value.detail)
    }

    private fun asset(id: String) = ImportAsset(
        sourceId = id,
        source = ImportSource.GOOGLE_TAKEOUT,
        resourceRole = ResourceRole.PRIMARY,
        displayName = "$id.jpg",
        byteCount = 1,
        metadata = SourceMetadata("$id.jpg"),
        sourceLocator = id,
    )

    private class Reader(private val assets: List<ImportAsset>) : ImportSourceReader {
        override val source = ImportSource.GOOGLE_TAKEOUT

        override suspend fun discover() = assets

        override suspend fun stage(asset: ImportAsset, stagingDirectory: Path): StagedAsset {
            Files.createDirectories(stagingDirectory)
            val path = stagingDirectory.resolve(asset.displayName)
            Files.writeString(path, asset.sourceId)
            return StagedAsset(asset, path)
        }
    }

    private class FakeGateway : LascoGateway {
        override val libraryId = "library"
        val imported = mutableListOf<String>()
        var pushes = 0

        override fun remotes() = listOf(LascoRemote("remote", "Remote", "s3"))
        override fun createAlbum(name: String) = name
        override fun addMediaToAlbum(albumId: String, mediaId: String) = Unit
        override fun importMedia(path: Path, metadata: SourceMetadata, aaeMediaId: String?, liveVideoMediaId: String?): ImportedMedia {
            imported += path.fileName.toString().substringBeforeLast('.')
            return ImportedMedia("media-${imported.last()}", false)
        }
        override fun applePhotosAssetRevisionMediaIds(revision: ApplePhotosAssetRevision): Map<ApplePhotosResourceDescriptor, String>? = null
        override fun recordApplePhotosResourceOrigin(mediaId: String, revision: ApplePhotosAssetRevision, resourceType: ApplePhotosResourceType, filename: String) = Unit
        override suspend fun benchmark(remote: LascoRemote, bytesPerUpload: Long) = RemoteBenchmark(remote.id, remote.name, 1, bytesPerUpload)
        override suspend fun push(remote: LascoRemote, maxConcurrentMediaUploads: Int, onProgress: (Double) -> Unit) {
            pushes += 1
            onProgress(1.0)
        }
        override fun close() = Unit
    }
}
