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
import com.sun.jna.Pointer
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.nio.file.Files
import java.nio.file.Path

/** JVM view of the shared Swift package's C transport. It is never loaded off macOS. */
private interface PhotoKitNative : Library {
    fun lasco_photos_authorization_status(): Int
    fun lasco_photos_request_authorization(): Int
    fun lasco_photos_discover_json(): Pointer?
    fun lasco_photos_discovery_scanned_count(): Int
    fun lasco_photos_discovery_total_count(): Int
    fun lasco_photos_stage(sessionHandle: String, destinationDirectory: String): Pointer?
    fun lasco_photos_free_string(value: Pointer?)
}

@Serializable
internal data class NativePhotoSourceMetadata(
    val capturedAt: String? = null,
    val modifiedAt: String? = null,
    val latitude: Double? = null,
    val longitude: Double? = null,
)

@Serializable
internal data class NativePhotoCompanionTickets(
    val adjustmentData: String? = null,
    val pairedVideo: String? = null,
)

@Serializable
internal data class NativePhotoResource(
    val ticket: String,
    val assetTicket: String,
    val role: String,
    val type: String,
    val filename: String,
    val byteCount: Long,
    val sourceMetadata: NativePhotoSourceMetadata = NativePhotoSourceMetadata(),
    val cloudAssetID: String? = null,
    val companionTickets: NativePhotoCompanionTickets = NativePhotoCompanionTickets(),
)

@Serializable
internal data class NativePhotoAsset(
    val ticket: String,
    val cloudAssetID: String? = null,
    val modificationDate: String? = null,
    val resources: List<NativePhotoResource>,
)

@Serializable
internal data class NativePhotoCollection(
    val cloudCollectionID: String,
    val kind: String,
    val name: String,
    val parentCloudCollectionID: String? = null,
    val memberCloudAssetIDs: List<String> = emptyList(),
    val memberAssetTickets: List<String> = emptyList(),
)

@Serializable
internal data class NativePhotoDiscovery(
    val assets: List<NativePhotoAsset>,
    val collections: List<NativePhotoCollection> = emptyList(),
    val scannedAssetCount: Int = assets.size,
    val totalAssetCount: Int = assets.size,
)

/**
 * Uses the shared Swift package through a narrow C transport. Discovery maps Photos resources to
 * opaque process-only staging handles. A restart requires a rescan; iCloud-only originals are
 * downloaded by the package into caller-owned staging.
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
        val discovery = json.decodeFromString<NativePhotoDiscovery>(bridge.readString(bridge.lasco_photos_discover_json()))
        collections = discovery.collections.map { collection ->
            ApplePhotosCollectionDescriptor(
                cloudCollectionId = collection.cloudCollectionID,
                kind = ApplePhotosCollectionKind.valueOf(collection.kind.uppercase()),
                name = collection.name,
                parentCloudCollectionId = collection.parentCloudCollectionID,
                memberCloudAssetIds = collection.memberCloudAssetIDs,
                memberSessionHandles = collection.memberAssetTickets,
            )
        }
        return discovery.assets.flatMap { asset ->
            val revision = ApplePhotosAssetRevision(
                cloudAssetId = asset.cloudAssetID,
                modificationDate = asset.modificationDate,
                resources = asset.resources.map { ApplePhotosResourceDescriptor(ApplePhotosResourceType.valueOf(it.type), it.filename) },
            )
            asset.resources.map { resource ->
            val resourceType = ApplePhotosResourceType.valueOf(resource.type)
            val isPrimary = resource.role == "primary"
            ImportAsset(
                sourceId = "photos:${resource.ticket}", source = source,
                resourceRole = when (resource.role) { "adjustmentData" -> ResourceRole.AAE_SIDECAR; "pairedVideo" -> ResourceRole.LIVE_PHOTO_VIDEO; else -> ResourceRole.PRIMARY },
                displayName = resource.filename, byteCount = resource.byteCount,
                metadata = SourceMetadata(resource.filename, resource.sourceMetadata.capturedAt, resource.sourceMetadata.modifiedAt, resource.sourceMetadata.latitude, resource.sourceMetadata.longitude),
                sourceLocator = resource.ticket, assetSessionHandle = asset.ticket,
                aaeSourceId = resource.companionTickets.adjustmentData?.takeIf { isPrimary && it != resource.ticket }?.let { "photos:$it" },
                liveVideoSourceId = resource.companionTickets.pairedVideo?.takeIf { isPrimary && it != resource.ticket }?.let { "photos:$it" },
                applePhotosRevision = revision,
                applePhotosResourceType = resourceType,
            )
            }
        }
    }

    override suspend fun stage(asset: ImportAsset, stagingDirectory: Path): StagedAsset {
        Files.createDirectories(stagingDirectory)
        val staged = Path.of(bridge.readString(bridge.lasco_photos_stage(asset.sourceLocator, stagingDirectory.toString()))).normalize()
        require(staged.startsWith(stagingDirectory.normalize())) { "PhotoKit returned a path outside the staging directory" }
        require(Files.isRegularFile(staged)) { "PhotoKit did not stage ${asset.displayName}" }
        return StagedAsset(asset, staged)
    }

    private companion object {
        fun loadBridge(): PhotoKitNative {
            check(System.getProperty("os.name").lowercase().contains("mac")) { "Apple Photos import is macOS-only" }
            return Native.load("LascoPhotoImportKit", PhotoKitNative::class.java)
        }
    }
}

private fun PhotoKitNative.readString(pointer: Pointer?): String {
    requireNotNull(pointer) { "LascoPhotoImportKit returned no result" }
    return try {
        pointer.getString(0)
    } finally {
        lasco_photos_free_string(pointer)
    }
}
