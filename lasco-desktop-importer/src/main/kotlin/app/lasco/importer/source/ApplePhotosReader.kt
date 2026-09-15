package app.lasco.importer.source

import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportSource
import app.lasco.importer.model.ResourceRole
import app.lasco.importer.model.ApplePhotosAssetRevision
import app.lasco.importer.model.ApplePhotosCollectionDescriptor
import app.lasco.importer.model.ApplePhotosCollectionKind
import app.lasco.importer.model.ApplePhotosResourceDescriptor
import app.lasco.importer.model.ApplePhotosResourceType
import app.lasco.importer.model.SourceMetadata
import app.lasco.importer.model.StagedAsset
import com.sun.jna.Library
import com.sun.jna.Native
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.nio.file.Files
import java.nio.file.Path

/** JVM view of the Kotlin/Native PhotoKit bridge. It is never loaded off macOS. */
private interface PhotoKitNative : Library {
    fun lasco_photos_authorization_status(): Int
    fun lasco_photos_request_authorization(): Int
    fun lasco_photos_discover_json(): String
    fun lasco_photos_discovery_scanned_count(): Int
    fun lasco_photos_discovery_total_count(): Int
    fun lasco_photos_stage(resourceId: String, destinationDirectory: String): String
}

@Serializable
private data class NativePhotoResource(
    val sessionHandle: String,
    val assetSessionHandle: String,
    val type: String,
    val filename: String,
    val byteCount: Long,
    val capturedAt: String? = null,
    val modifiedAt: String? = null,
    val latitude: Double? = null,
    val longitude: Double? = null,
    val albumNames: List<String> = emptyList(),
    val pairedVideoSessionHandle: String? = null,
    val aaeSessionHandle: String? = null,
    val cloudAssetId: String? = null,
    val resourceType: String,
)

@Serializable
private data class NativePhotoCollection(
    val cloudCollectionId: String,
    val kind: String,
    val name: String,
    val parentCloudCollectionId: String? = null,
    val memberCloudAssetIds: List<String> = emptyList(),
    val memberSessionHandles: List<String> = emptyList(),
)

@Serializable
private data class NativePhotoDiscovery(
    val resources: List<NativePhotoResource>,
    val collections: List<NativePhotoCollection> = emptyList(),
)

/**
 * Uses PhotoKit directly through the bundled Kotlin/Native bridge. Discovery maps Photos resource
 * IDs to opaque process-only staging handles. A restart requires a rescan; iCloud-only originals
 * are downloaded by `PHAssetResourceManager` into staging.
 */
class ApplePhotosReader private constructor(private val bridge: PhotoKitNative) : ApplePhotosCollectionSourceReader {
    constructor() : this(loadBridge())

    override val source = ImportSource.APPLE_PHOTOS
    private val json = Json { ignoreUnknownKeys = true }
    private var collections: List<ApplePhotosCollectionDescriptor> = emptyList()

    override fun collectionDescriptors(): List<ApplePhotosCollectionDescriptor> = collections

    // macOS may grant a limited Photos selection (status 4). It is still valid access and the
    // bridge will enumerate exactly that allowed selection.
    fun hasPermission(): Boolean = bridge.lasco_photos_authorization_status() in setOf(3, 4)
    fun requestPermission(): Boolean = bridge.lasco_photos_request_authorization() in setOf(3, 4)
    fun discoveryProgress(): Pair<Int, Int> = bridge.lasco_photos_discovery_scanned_count() to bridge.lasco_photos_discovery_total_count()

    override suspend fun discover(): List<ImportAsset> {
        check(hasPermission()) { "Apple Photos permission has not been granted" }
        val discovery = json.decodeFromString<NativePhotoDiscovery>(bridge.lasco_photos_discover_json())
        val resources = discovery.resources
        collections = discovery.collections.map { collection ->
            ApplePhotosCollectionDescriptor(
                cloudCollectionId = collection.cloudCollectionId,
                kind = ApplePhotosCollectionKind.valueOf(collection.kind),
                name = collection.name,
                parentCloudCollectionId = collection.parentCloudCollectionId,
                memberCloudAssetIds = collection.memberCloudAssetIds,
                memberSessionHandles = collection.memberSessionHandles,
            )
        }
        val revisionsByAssetHandle = resources.groupBy { it.assetSessionHandle }.mapValues { (_, resourcesForAsset) ->
            val cloudAssetId = resourcesForAsset.firstNotNullOfOrNull { it.cloudAssetId }
            ApplePhotosAssetRevision(
                cloudAssetId = cloudAssetId,
                modificationDate = resourcesForAsset.first().modifiedAt,
                resources = resourcesForAsset.map { ApplePhotosResourceDescriptor(ApplePhotosResourceType.valueOf(it.resourceType), it.filename) },
            )
        }
        return resources.map { resource ->
            val resourceType = ApplePhotosResourceType.valueOf(resource.resourceType)
            ImportAsset(
                sourceId = "photos:${resource.sessionHandle}", source = source,
                resourceRole = when (resource.type) { "aae" -> ResourceRole.AAE_SIDECAR; "pairedVideo" -> ResourceRole.LIVE_PHOTO_VIDEO; else -> ResourceRole.PRIMARY },
                displayName = resource.filename, byteCount = resource.byteCount,
                metadata = SourceMetadata(resource.filename, resource.capturedAt, resource.modifiedAt, resource.latitude, resource.longitude),
                sourceLocator = resource.sessionHandle, assetSessionHandle = resource.assetSessionHandle, albumNames = resource.albumNames,
                aaeSourceId = resource.aaeSessionHandle?.let { "photos:$it" }, liveVideoSourceId = resource.pairedVideoSessionHandle?.let { "photos:$it" },
                applePhotosRevision = revisionsByAssetHandle.getValue(resource.assetSessionHandle),
                applePhotosResourceType = resourceType,
            )
        }
    }

    override suspend fun stage(asset: ImportAsset, stagingDirectory: Path): StagedAsset {
        Files.createDirectories(stagingDirectory)
        val staged = Path.of(bridge.lasco_photos_stage(asset.sourceLocator, stagingDirectory.toString())).normalize()
        require(staged.startsWith(stagingDirectory.normalize())) { "PhotoKit returned a path outside the staging directory" }
        require(Files.isRegularFile(staged)) { "PhotoKit did not stage ${asset.displayName}" }
        return StagedAsset(asset, staged)
    }

    private companion object {
        fun loadBridge(): PhotoKitNative {
            check(System.getProperty("os.name").lowercase().contains("mac")) { "Apple Photos import is macOS-only" }
            return Native.load("lasco_photos_bridge", PhotoKitNative::class.java)
        }
    }
}
