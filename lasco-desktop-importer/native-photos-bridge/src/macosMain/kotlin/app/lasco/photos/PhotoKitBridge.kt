@file:OptIn(
    kotlinx.cinterop.ExperimentalForeignApi::class,
    kotlin.experimental.ExperimentalNativeApi::class,
)

package app.lasco.photos

import kotlinx.cinterop.CPointer
import kotlinx.cinterop.CValue
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.ByteVar
import kotlinx.cinterop.allocArray
import kotlinx.cinterop.cstr
import kotlinx.cinterop.get
import kotlinx.cinterop.memScoped
import kotlinx.cinterop.nativeHeap
import kotlinx.cinterop.set
import kotlinx.cinterop.staticCFunction
import kotlinx.cinterop.toKString
import kotlinx.cinterop.useContents
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlin.concurrent.Volatile
import platform.Foundation.NSDate
import platform.Foundation.NSFileManager
import platform.Foundation.NSISO8601DateFormatter
import platform.Foundation.NSNumber
import platform.Foundation.NSSelectorFromString
import platform.Foundation.NSURL
import platform.Foundation.NSUUID
import platform.Photos.PHAsset
import platform.Photos.PHAssetCollection
import platform.Photos.PHCollection
import platform.Photos.PHCollectionList
import platform.Photos.PHAssetResource
import platform.Photos.PHAssetResourceManager
import platform.Photos.PHAssetResourceRequestOptions
import platform.Photos.PHAssetResourceTypeAdjustmentData
import platform.Photos.PHAssetResourceTypeFullSizePairedVideo
import platform.Photos.PHAssetResourceTypeFullSizePhoto
import platform.Photos.PHAssetResourceTypeFullSizeVideo
import platform.Photos.PHAssetResourceTypePairedVideo
import platform.Photos.PHAssetResourceTypePhoto
import platform.Photos.PHAssetResourceTypeVideo
import platform.Photos.PHCloudIdentifierMapping
import platform.Photos.cloudIdentifierMappingsForLocalIdentifiers
import platform.Photos.PHAuthorizationStatusAuthorized
import platform.Photos.PHAuthorizationStatusLimited
import platform.Photos.PHPhotoLibrary
import platform.Photos.PHAccessLevelReadWrite
import platform.darwin.NSObject

@Serializable
private data class ResourceRecord(
    val sessionHandle: String, val assetSessionHandle: String, val type: String, val filename: String, val byteCount: Long,
    val capturedAt: String?, val modifiedAt: String?, val latitude: Double?, val longitude: Double?, val albumNames: List<String>,
    val pairedVideoSessionHandle: String?, val aaeSessionHandle: String?,
    val cloudAssetId: String?, val resourceType: String,
)

@Serializable
private data class CollectionRecord(
    val cloudCollectionId: String,
    val kind: String,
    val name: String,
    val parentCloudCollectionId: String?,
    val memberCloudAssetIds: List<String>,
    val memberSessionHandles: List<String>,
)

@Serializable
private data class DiscoveryResult(
    val resources: List<ResourceRecord>,
    val collections: List<CollectionRecord>,
)

private val json = Json
private val resources = mutableMapOf<String, PHAssetResource>()
private val iso8601 = NSISO8601DateFormatter()
@Volatile private var discoveredAssetCount = 0
@Volatile private var totalAssetCount = 0
private const val cloudMappingBatchSize = 250

private fun sessionHandle(): String = NSUUID().UUIDString
private val valueForKeySelector = NSSelectorFromString("valueForKey:")

/**
 * `fileSize` is PhotoKit KVC metadata. Unlike a local URL, it is available while an iCloud
 * original is still remote, which lets the importer estimate upload size during discovery.
 */
private fun resourceByteCount(resource: PHAssetResource): Long =
    (resource.performSelector(valueForKeySelector, withObject = "fileSize") as? NSNumber)?.longLongValue ?: 0

/**
 * Keep using the serialization already stored in Apple Photos provenance. A future migration to
 * `archivalStringValue` must explicitly preserve matching with existing libraries.
 */
private fun serializedCloudId(mapping: PHCloudIdentifierMapping?): String? = mapping?.cloudIdentifier?.stringValue

/** PhotoKit accepts an array, but keeping requests bounded avoids a giant bridge call on large libraries. */
private fun cloudMappingsFor(localIdentifiers: List<String>): Map<String, Any?> = buildMap {
    localIdentifiers.distinct().chunked(cloudMappingBatchSize).forEach { batch ->
        val mappings = PHPhotoLibrary.sharedPhotoLibrary().cloudIdentifierMappingsForLocalIdentifiers(batch)
        batch.forEach { localIdentifier -> mappings[localIdentifier]?.let { put(localIdentifier, it) } }
    }
}

private fun retainedUtf8(value: String): CPointer<ByteVar> {
    val bytes = value.encodeToByteArray()
    return nativeHeap.allocArray<ByteVar>(bytes.size + 1).also { pointer ->
        bytes.forEachIndexed { index, byte -> pointer[index] = byte }
        pointer[bytes.size] = 0
    }
}

private fun photosAccessGranted(): Boolean = authorizationStatus().let { status ->
    status == PHAuthorizationStatusAuthorized.toInt() || status == PHAuthorizationStatusLimited.toInt()
}

@CName("lasco_photos_authorization_status")
fun authorizationStatus(): Int = PHPhotoLibrary.authorizationStatusForAccessLevel(PHAccessLevelReadWrite).toInt()

@CName("lasco_photos_request_authorization")
fun requestAuthorization(): Int {
    var result = authorizationStatus()
    val semaphore = platform.darwin.dispatch_semaphore_create(0)
    PHPhotoLibrary.requestAuthorizationForAccessLevel(PHAccessLevelReadWrite) { status -> result = status.toInt(); platform.darwin.dispatch_semaphore_signal(semaphore) }
    platform.darwin.dispatch_semaphore_wait(semaphore, platform.darwin.DISPATCH_TIME_FOREVER)
    return result
}

@CName("lasco_photos_discovery_scanned_count")
fun discoveryScannedCount(): Int = discoveredAssetCount

@CName("lasco_photos_discovery_total_count")
fun discoveryTotalCount(): Int = totalAssetCount

/**
 * Enumerates PhotoKit once and returns only durable cloud IDs plus random, process-local staging
 * handles.  PhotoKit local identifiers never leave this native adapter.
 */
@CName("lasco_photos_discover_json")
fun discoverJson(): CPointer<ByteVar>? = memScoped {
    check(photosAccessGranted()) { "Photos permission denied" }
    // A new discovery is a new in-memory import session. Expire all previous opaque handles so
    // an old scan cannot be replayed after the user chooses the source again.
    resources.clear()
    val records = mutableListOf<ResourceRecord>()
    val assets = PHAsset.fetchAssetsWithOptions(null)
    discoveredAssetCount = 0
    totalAssetCount = assets.count.toInt()
    val photos = buildList {
        assets.enumerateObjectsUsingBlock { asset, _, _ -> add(asset as PHAsset) }
    }
    val cloudMappings = cloudMappingsFor(photos.map { it.localIdentifier })
    val assetSessionHandles = mutableMapOf<String, String>()
    val cloudAssetIds = mutableMapOf<String, String>()
    photos.forEach { photo ->
        assetSessionHandles[photo.localIdentifier] = sessionHandle()
        serializedCloudId(cloudMappings[photo.localIdentifier] as? PHCloudIdentifierMapping)
            ?.let { cloudAssetIds[photo.localIdentifier] = it }
    }
    photos.forEach { photo ->
        val assetResources = PHAssetResource.assetResourcesForAsset(photo)
            .filterIsInstance<PHAssetResource>()
        val aae = assetResources.firstOrNull { it.type == PHAssetResourceTypeAdjustmentData }
        val pairedVideo = assetResources.firstOrNull { it.type == PHAssetResourceTypePairedVideo }
            ?: assetResources.firstOrNull { it.type == PHAssetResourceTypeFullSizePairedVideo }
        val primaryPhoto = assetResources.firstOrNull { it.type == PHAssetResourceTypePhoto }
            ?: assetResources.firstOrNull { it.type == PHAssetResourceTypeFullSizePhoto }
        val isEditedPhoto = assetResources.any { it.type == PHAssetResourceTypePhoto }
            && assetResources.any { it.type == PHAssetResourceTypeFullSizePhoto }
        val primaryVideo = assetResources.firstOrNull { it.type == PHAssetResourceTypeVideo }
            ?: assetResources.firstOrNull { it.type == PHAssetResourceTypeFullSizeVideo }
        val selected = mutableListOf<PHAssetResource>()
        if (isEditedPhoto && aae != null) selected += aae
        if (primaryPhoto != null && pairedVideo != null) selected += pairedVideo
        when {
            primaryPhoto != null -> selected += primaryPhoto
            pairedVideo != null -> selected += pairedVideo
            primaryVideo != null -> selected += primaryVideo
        }
        val resourceHandles = assetResources.associateWith { sessionHandle() }
        selected.forEach { resource ->
            val handle = resourceHandles.getValue(resource)
            resources[handle] = resource
            val type = when (resource.type) {
                PHAssetResourceTypeAdjustmentData -> "aae"
                PHAssetResourceTypePairedVideo, PHAssetResourceTypeFullSizePairedVideo -> "pairedVideo"
                else -> "primary"
            }
            // Companion links belong on the primary resource only. Attaching the AAE or paired
            // video handle to its own record creates a self-referential import dependency.
            val isPrimary = type == "primary"
            val resourceType = when (resource.type) {
                PHAssetResourceTypePhoto -> "PHOTO"
                PHAssetResourceTypeFullSizePhoto -> "FULL_SIZE_PHOTO"
                PHAssetResourceTypeVideo -> "VIDEO"
                PHAssetResourceTypeFullSizeVideo -> "FULL_SIZE_VIDEO"
                PHAssetResourceTypeAdjustmentData -> "ADJUSTMENT_DATA"
                PHAssetResourceTypePairedVideo -> "PAIRED_VIDEO"
                PHAssetResourceTypeFullSizePairedVideo -> "FULL_SIZE_PAIRED_VIDEO"
                else -> error("unexpected selected PhotoKit resource type")
            }
            val coordinates = photo.location?.coordinate?.useContents { latitude to longitude }
            records += ResourceRecord(handle, assetSessionHandles.getValue(photo.localIdentifier), type, resource.originalFilename, resourceByteCount(resource),
                photo.creationDate?.let(iso8601::stringFromDate), photo.modificationDate?.let(iso8601::stringFromDate), coordinates?.first, coordinates?.second,
                emptyList(), if (isPrimary) pairedVideo?.let(resourceHandles::get) else null, if (isPrimary) aae?.let(resourceHandles::get) else null,
                cloudAssetIds[photo.localIdentifier], resourceType)
        }
        discoveredAssetCount += 1
    }
    // Collection traversal and mapping stays entirely native too. V1 deliberately omits
    // collections without a cloud mapping; names/local IDs are never used as durable identity.
    val collectionLocals = mutableListOf<Pair<PHCollection, String?>>()
    fun collectCollections(result: platform.Photos.PHFetchResult, parent: String?) {
        for (index in 0 until result.count().toInt()) {
            when (val collection = result.objectAtIndex(index.toULong())) {
                is PHCollectionList -> {
                    collectionLocals += collection to parent
                    collectCollections(PHCollection.fetchCollectionsInCollectionList(collection, null), collection.localIdentifier)
                }
                is PHAssetCollection -> if (collection.assetCollectionType == 1L && collection.assetCollectionSubtype == 2L) {
                    collectionLocals += collection to parent
                }
            }
        }
    }
    collectCollections(PHCollectionList.fetchTopLevelUserCollectionsWithOptions(null), null)
    val collectionMappings = cloudMappingsFor(collectionLocals.map { it.first.localIdentifier })
    val collections = collectionLocals.mapNotNull { (collection, parentLocal) ->
        val cloudId = serializedCloudId(collectionMappings[collection.localIdentifier] as? PHCloudIdentifierMapping)
            ?: return@mapNotNull null
        val parentCloudId = parentLocal?.let { local ->
            serializedCloudId(collectionMappings[local] as? PHCloudIdentifierMapping)
        }
        when (collection) {
            is PHCollectionList -> CollectionRecord(cloudId, "FOLDER", collection.localizedTitle ?: "", parentCloudId, emptyList(), emptyList())
            is PHAssetCollection -> {
                val members = PHAsset.fetchAssetsInAssetCollection(collection, null)
                val memberCloudIds = mutableListOf<String>()
                val memberHandles = mutableListOf<String>()
                for (index in 0 until members.count().toInt()) {
                    val local = (members.objectAtIndex(index.toULong()) as PHAsset).localIdentifier
                    cloudAssetIds[local]?.let(memberCloudIds::add) ?: assetSessionHandles[local]?.let(memberHandles::add)
                }
                CollectionRecord(cloudId, "ALBUM", collection.localizedTitle ?: "", parentCloudId, memberCloudIds, memberHandles)
            }
            else -> null
        }
    }
    retainedUtf8(json.encodeToString(DiscoveryResult(records, collections)))
}

/** Downloads an iCloud resource using PHAssetResourceManager into the caller's staging directory. */
@CName("lasco_photos_stage")
fun stage(resourceId: CPointer<ByteVar>?, directory: CPointer<ByteVar>?): CPointer<ByteVar>? = memScoped {
    val handle = resourceId!!.toKString()
    val resource = resources[handle] ?: error("unknown or expired Photos session handle")
    val destination = NSURL.fileURLWithPath(directory!!.toKString())
        .URLByAppendingPathComponent("${handle.hashCode()}-${resource.originalFilename}")!!
    val semaphore = platform.darwin.dispatch_semaphore_create(0)
    var failure: String? = null
    // The default options do not fetch an original that lives only in iCloud. Explicitly permit
    // network access so the requested resource is downloaded once into the import staging area.
    val options = PHAssetResourceRequestOptions().apply { networkAccessAllowed = true }
    PHAssetResourceManager.defaultManager().writeDataForAssetResource(resource, destination, options) { error -> failure = error?.localizedDescription; platform.darwin.dispatch_semaphore_signal(semaphore) }
    platform.darwin.dispatch_semaphore_wait(semaphore, platform.darwin.DISPATCH_TIME_FOREVER)
    check(failure == null) { failure!! }
    retainedUtf8(destination.path!!)
}
