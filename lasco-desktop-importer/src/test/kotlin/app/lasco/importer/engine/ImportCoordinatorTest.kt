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
import kotlin.test.assertContains
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class ImportCoordinatorTest {
    @Test
    fun `pause is process local and completion waits for every asset`() = runBlocking {
        val gateway = FakeGateway()
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage"))

        coordinator.discover(Reader(listOf(asset("one"), asset("two"))), chunkSize = 1)
        coordinator.requestPause()
        coordinator.startOrResume(emptyList())

        assertEquals(1, gateway.imported.size)
        assertEquals(ImportRunState.PAUSED, coordinator.progress.value.state)

        coordinator.startOrResume(emptyList())

        assertEquals(listOf("one", "two"), gateway.imported)
        assertEquals(ImportRunState.COMPLETE, coordinator.progress.value.state)
        assertEquals(3, gateway.pushes)
    }

    @Test
    fun `primary resources get thumbnails while companion resources do not`() = runBlocking {
        val gateway = FakeGateway()
        val sidecar = asset("sidecar").copy(resourceRole = ResourceRole.AAE_SIDECAR)
        val primary = asset("primary")
        val coordinator = ImportCoordinator(
            gateway,
            Files.createTempDirectory("lasco-stage"),
            thumbnailGenerator = { "thumbnail".encodeToByteArray() },
        )

        coordinator.discover(Reader(listOf(sidecar, primary)), chunkSize = 2)
        coordinator.startOrResume(emptyList())

        assertEquals(mapOf("media-primary" to "thumbnail".encodeToByteArray().toList()), gateway.thumbnails.mapValues { it.value.toList() })
    }

    @Test
    fun `a failed final remote push retains the local library setup`() = runBlocking {
        val gateway = FakeGateway().apply { failPushForRemoteId = "remote" }
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage"))

        coordinator.discover(Reader(listOf(asset("one"))), chunkSize = 1)

        val failure = assertFailsWith<RemotePushFailure> { coordinator.startOrResume(emptyList()) }
        assertContains(failure.message.orEmpty(), "Could not upload to remote \"Remote\" (s3).")
        assertContains(failure.message.orEmpty(), "Remote Remote is unavailable")
        assertEquals(listOf("one"), gateway.imported)
    }

    @Test
    fun `a forbidden remote push explains why benchmark success is insufficient`() {
        val failure = RemotePushFailure(
            LascoRemote("remote", "Archive", "s3"),
            IllegalStateException("remote unreachable: storage error: Got HTTP 403 with content \"\""),
        )

        assertContains(failure.message.orEmpty(), "Archive")
        assertContains(failure.message.orEmpty(), "HTTP 403 means the storage service denied a request")
        assertContains(failure.message.orEmpty(), "read/list/write/delete permissions")
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
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage"))

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

        val plan = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage"))
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
    fun `PhotoKit cloud identifier reuses a matching import`() = runBlocking {
        val gateway = FakeGateway()
        val revision = ApplePhotosAssetRevision(
            cloudAssetId = "cloud-id",
            modificationDate = null,
            resources = listOf(ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic")),
        )
        gateway.revisionMedia[revision] = mapOf(
            ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic") to "still",
        )
        gateway.confirmedMedia += "still"
        val asset = asset("still").copy(
            source = ImportSource.APPLE_PHOTOS,
            displayName = "still.heic",
            applePhotosRevision = revision,
            applePhotosResourceType = ApplePhotosResourceType.PHOTO,
        )

        val plan = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage"))
            .discover(Reader(listOf(asset)))

        assertEquals(1, plan.alreadyCompleted)
        assertEquals(1, plan.remotes.single().alreadyThere.photos)
        assertEquals(0, plan.remotes.single().toUpload.photos)
    }

    @Test
    fun `fully present Apple Photos resources need no import when collection metadata matches`() = runBlocking {
        val gateway = FakeGateway()
        val revision = ApplePhotosAssetRevision(
            "cloud-asset",
            null,
            listOf(ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic")),
        )
        gateway.revisionMedia[revision] = mapOf(
            ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic") to "still",
        )
        gateway.confirmedMedia += "still"
        gateway.collectionLinks["album-cloud"] = "existing-album"
        gateway.mediaAlbums["still"] = setOf("existing-album")
        val asset = asset("still").copy(
            source = ImportSource.APPLE_PHOTOS,
            displayName = "still.heic",
            applePhotosRevision = revision,
            applePhotosResourceType = ApplePhotosResourceType.PHOTO,
        )
        val reader = AppleReader(
            listOf(asset),
            listOf(ApplePhotosCollectionDescriptor("album-cloud", ApplePhotosCollectionKind.ALBUM, "Trip", null, listOf("cloud-asset"), emptyList())),
        )

        val plan = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")).discover(reader)

        assertEquals(false, plan.hasMediaToUpload)
        assertEquals(false, plan.metadataToAdd)
    }

    @Test
    fun `collection metadata remains planned when a remote is missing media blobs`() = runBlocking {
        val gateway = FakeGateway()
        val revision = ApplePhotosAssetRevision(
            "cloud-asset",
            null,
            listOf(ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic")),
        )
        gateway.revisionMedia[revision] = mapOf(
            ApplePhotosResourceDescriptor(ApplePhotosResourceType.PHOTO, "still.heic") to "still",
        )
        val asset = asset("still").copy(
            source = ImportSource.APPLE_PHOTOS,
            displayName = "still.heic",
            applePhotosRevision = revision,
            applePhotosResourceType = ApplePhotosResourceType.PHOTO,
        )
        val reader = AppleReader(
            listOf(asset),
            listOf(ApplePhotosCollectionDescriptor("album-cloud", ApplePhotosCollectionKind.ALBUM, "Trip", null, listOf("cloud-asset"), emptyList())),
        )

        val plan = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")).discover(reader)

        assertEquals(true, plan.hasMediaToUpload)
        assertEquals(true, plan.metadataToAdd)
    }

    @Test
    fun `new media in an existing collection is planned as metadata work`() = runBlocking {
        val gateway = FakeGateway().apply { collectionLinks["album-cloud"] = "existing-album" }
        val asset = asset("still").copy(
            source = ImportSource.APPLE_PHOTOS,
            assetSessionHandle = "asset-session",
            applePhotosRevision = ApplePhotosAssetRevision("cloud-asset", null, emptyList()),
        )
        val reader = AppleReader(
            listOf(asset),
            listOf(ApplePhotosCollectionDescriptor("album-cloud", ApplePhotosCollectionKind.ALBUM, "Trip", null, emptyList(), listOf("asset-session"))),
        )

        val plan = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage")).discover(reader)

        assertEquals(true, plan.metadataToAdd)
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
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage"))

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
        val coordinator = ImportCoordinator(gateway, Files.createTempDirectory("lasco-stage"))

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
        val thumbnails = mutableMapOf<String, ByteArray>()
        val collectionLinks = mutableMapOf<String, String>()
        val mediaAlbums = mutableMapOf<String, Set<String>>()
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
        override fun setMediaThumbnail(mediaId: String, data: ByteArray) { thumbnails[mediaId] = data }
        override fun applePhotosAssetRevisionMediaIds(revision: ApplePhotosAssetRevision): Map<ApplePhotosResourceDescriptor, String>? = revisionMedia[revision]
        override fun recordApplePhotosResourceOrigin(mediaId: String, revision: ApplePhotosAssetRevision, resourceType: ApplePhotosResourceType, filename: String) = Unit
        override fun applePhotosCollectionLinks(collections: List<ApplePhotosCollectionDescriptor>) = collections.map { collectionLinks[it.cloudCollectionId] }
        override fun mediaAlbumIds(mediaId: String) = mediaAlbums[mediaId].orEmpty()
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
