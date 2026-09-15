package app.lasco.importer.engine

import app.lasco.importer.ffi.LascoGateway
import app.lasco.importer.ffi.LascoRemote
import app.lasco.importer.model.ApplePhotosAssetRevision
import app.lasco.importer.model.ApplePhotosCollectionDescriptor
import app.lasco.importer.model.ApplePhotosCollectionKind
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
import app.lasco.importer.source.ApplePhotosCollectionSourceReader
import kotlinx.coroutines.runBlocking
import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails

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

    @Test
    fun `a failed final remote push retains the temporary library`() = runBlocking {
        val gateway = FakeGateway().apply { failPushForRemoteId = "remote" }
        var finalizations = 0
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")) {
            finalizations += 1
            null
        }

        coordinator.discover(Reader(listOf(asset("one"))), chunkSize = 1)

        assertFails { coordinator.startOrResume(emptyList()) }
        assertEquals(listOf("one"), gateway.imported)
        assertEquals(0, finalizations)
    }

    @Test
    fun `self-referential companion metadata does not block a Photos scan`() = runBlocking {
        val gateway = FakeGateway()
        val sidecar = asset("sidecar").copy(
            source = ImportSource.APPLE_PHOTOS,
            resourceRole = ResourceRole.AAE_SIDECAR,
            aaeSourceId = "sidecar",
        )
        val primary = asset("primary").copy(
            source = ImportSource.APPLE_PHOTOS,
            aaeSourceId = "sidecar",
        )
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")) { null }

        coordinator.discover(Reader(listOf(primary, sidecar)), chunkSize = 2)
        coordinator.startOrResume(emptyList())

        assertEquals(listOf("sidecar", "primary"), gateway.imported)
    }

    @Test
    fun `Apple Photos summary separates confirmed remote media from companions to upload`() = runBlocking {
        val gateway = FakeGateway()
        val revision = ApplePhotosAssetRevision(
            "cloud-asset",
            null,
            listOf(
                ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic"),
                ApplePhotosResourceDescriptor(ApplePhotosResourceType.PAIRED_VIDEO, "motion.mov"),
                ApplePhotosResourceDescriptor(ApplePhotosResourceType.ADJUSTMENT_DATA, "edit.aae"),
                ApplePhotosResourceDescriptor(ApplePhotosResourceType.VIDEO, "movie.mov"),
            ),
        )
        gateway.revisionMedia[revision] = mapOf(
            ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic") to "still",
            ApplePhotosResourceDescriptor(ApplePhotosResourceType.PAIRED_VIDEO, "motion.mov") to "motion",
            ApplePhotosResourceDescriptor(ApplePhotosResourceType.ADJUSTMENT_DATA, "edit.aae") to "edit",
            ApplePhotosResourceDescriptor(ApplePhotosResourceType.VIDEO, "movie.mov") to "movie",
        )
        gateway.confirmedMedia += setOf("still", "movie")
        val assets = listOf(
            asset("still").copy(source = ImportSource.APPLE_PHOTOS, displayName = "still.heic", byteCount = 10, applePhotosRevision = revision, applePhotosResourceType = ApplePhotosResourceType.PHOTO),
            asset("motion").copy(source = ImportSource.APPLE_PHOTOS, resourceRole = ResourceRole.LIVE_PHOTO_VIDEO, displayName = "motion.mov", byteCount = 20, applePhotosRevision = revision, applePhotosResourceType = ApplePhotosResourceType.PAIRED_VIDEO),
            asset("edit").copy(source = ImportSource.APPLE_PHOTOS, resourceRole = ResourceRole.AAE_SIDECAR, displayName = "edit.aae", byteCount = 30, applePhotosRevision = revision, applePhotosResourceType = ApplePhotosResourceType.ADJUSTMENT_DATA),
            asset("movie").copy(source = ImportSource.APPLE_PHOTOS, displayName = "movie.mov", byteCount = 40, applePhotosRevision = revision, applePhotosResourceType = ApplePhotosResourceType.VIDEO),
        )

        val plan = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")) { null }
            .discover(Reader(assets))
        val remote = plan.remotes.single()

        assertEquals(1, plan.library.photos)
        assertEquals(1, plan.library.videos)
        assertEquals(1, plan.library.livePhotoVideos)
        assertEquals(1, plan.library.aaeFiles)
        assertEquals(2, remote.alreadyThere.photos + remote.alreadyThere.videos)
        assertEquals(1, remote.toUpload.livePhotoVideos)
        assertEquals(1, remote.toUpload.aaeFiles)
        assertEquals(50, remote.toUpload.bytes)
    }

    @Test
    fun `cloud linked collections reuse canonical albums and add primary media only`() = runBlocking {
        val gateway = FakeGateway().apply { collectionLinks["album-cloud"] = "existing-album" }
        val primary = asset("still").copy(
            source = ImportSource.APPLE_PHOTOS,
            assetSessionHandle = "asset-session",
            applePhotosRevision = ApplePhotosAssetRevision("asset-cloud", null, emptyList()),
        )
        val companion = asset("sidecar").copy(source = ImportSource.APPLE_PHOTOS, resourceRole = ResourceRole.AAE_SIDECAR)
        val reader = AppleReader(
            listOf(companion, primary),
            listOf(ApplePhotosCollectionDescriptor("album-cloud", ApplePhotosCollectionKind.ALBUM, "Do not rename", null, listOf("asset-cloud"), emptyList())),
        )
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")) { null }

        coordinator.discover(reader, chunkSize = 2)
        coordinator.startOrResume(emptyList())

        assertEquals(emptyList<String>(), gateway.createdAlbums)
        assertEquals(listOf("existing-album" to "media-still"), gateway.memberships)
    }

    @Test
    fun `new Apple collection hierarchy is created parent first and links only the primary`() = runBlocking {
        val gateway = FakeGateway()
        val primary = asset("still").copy(
            source = ImportSource.APPLE_PHOTOS,
            assetSessionHandle = "asset-session",
            applePhotosRevision = ApplePhotosAssetRevision("asset-cloud", null, emptyList()),
        )
        val sidecar = asset("sidecar").copy(source = ImportSource.APPLE_PHOTOS, resourceRole = ResourceRole.AAE_SIDECAR)
        val reader = AppleReader(
            listOf(sidecar, primary),
            listOf(
                ApplePhotosCollectionDescriptor("folder-cloud", ApplePhotosCollectionKind.FOLDER, "2019", null, emptyList(), emptyList()),
                ApplePhotosCollectionDescriptor("album-cloud", ApplePhotosCollectionKind.ALBUM, "Trip", "folder-cloud", listOf("asset-cloud"), emptyList()),
            ),
        )
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")) { null }

        coordinator.discover(reader, chunkSize = 2)
        coordinator.startOrResume(emptyList())

        assertEquals(listOf("2019" to null, "Trip" to "created-2019"), gateway.createdAlbumParents)
        assertEquals(listOf("created-Trip" to "media-still"), gateway.memberships)
        assertEquals("created-Trip", gateway.collectionLinks["album-cloud"])
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
        var failPushForRemoteId: String? = null

        override fun remotes() = listOf(LascoRemote("remote", "Remote", "s3"))
        val createdAlbums = mutableListOf<String>()
        val createdAlbumParents = mutableListOf<Pair<String, String?>>()
        val memberships = mutableListOf<Pair<String, String>>()
        val collectionLinks = mutableMapOf<String, String>()
        val revisionMedia = mutableMapOf<ApplePhotosAssetRevision, Map<ApplePhotosResourceDescriptor, String>>()
        val confirmedMedia = mutableSetOf<String>()
        override fun createAlbum(name: String, parentAlbumId: String?): String {
            createdAlbums += name
            createdAlbumParents += name to parentAlbumId
            return "created-$name"
        }
        override fun addMediaToAlbum(albumId: String, mediaId: String) { memberships += albumId to mediaId }
        override fun importMedia(path: Path, metadata: SourceMetadata, aaeMediaId: String?, liveVideoMediaId: String?): ImportedMedia {
            imported += path.fileName.toString().substringBeforeLast('.')
            return ImportedMedia("media-${imported.last()}", false)
        }
        override fun applePhotosAssetRevisionMediaIds(revision: ApplePhotosAssetRevision): Map<ApplePhotosResourceDescriptor, String>? = revisionMedia[revision]
        override fun recordApplePhotosResourceOrigin(mediaId: String, revision: ApplePhotosAssetRevision, resourceType: ApplePhotosResourceType, filename: String) = Unit
        override fun applePhotosCollectionLinks(collections: List<ApplePhotosCollectionDescriptor>) = collections.map { collectionLinks[it.cloudCollectionId] }
        override fun recordApplePhotosCollectionLink(albumId: String, collection: ApplePhotosCollectionDescriptor) { collectionLinks[collection.cloudCollectionId] = albumId }
        override suspend fun fetchRemoteOperations(remote: LascoRemote) = Unit
        override fun hasUnpushedOperations(remote: LascoRemote) = false
        override suspend fun confirmRemoteMedia(remote: LascoRemote) = Unit
        override fun confirmedRemoteMediaIds(remote: LascoRemote, mediaIds: Set<String>) = confirmedMedia.intersect(mediaIds)
        override suspend fun benchmark(remote: LascoRemote, bytesPerUpload: Long) = RemoteBenchmark(remote.id, remote.name, 1, bytesPerUpload)
        override suspend fun push(remote: LascoRemote, maxConcurrentMediaUploads: Int, onProgress: (Double) -> Unit) {
            if (remote.id == failPushForRemoteId) error("Remote ${remote.name} is unavailable")
            pushes += 1
            onProgress(1.0)
        }
        override fun close() = Unit
    }

    private class AppleReader(
        private val assets: List<ImportAsset>,
        private val collections: List<ApplePhotosCollectionDescriptor>,
    ) : ApplePhotosCollectionSourceReader {
        override val source = ImportSource.APPLE_PHOTOS
        override suspend fun discover() = assets
        override fun collectionDescriptors() = collections
        override suspend fun stage(asset: ImportAsset, stagingDirectory: Path): StagedAsset {
            Files.createDirectories(stagingDirectory)
            val path = stagingDirectory.resolve(asset.displayName)
            Files.writeString(path, asset.sourceId)
            return StagedAsset(asset, path)
        }
    }
}
