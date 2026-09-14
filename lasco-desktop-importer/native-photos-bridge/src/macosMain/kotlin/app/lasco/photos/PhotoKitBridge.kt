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
import platform.Foundation.NSURL
import platform.Photos.PHAsset
import platform.Photos.PHAssetCollection
import platform.Photos.PHAssetResource
import platform.Photos.PHAssetResourceManager
import platform.Photos.PHAssetResourceRequestOptions
import platform.Photos.PHAuthorizationStatusAuthorized
import platform.Photos.PHAuthorizationStatusLimited
import platform.Photos.PHPhotoLibrary
import platform.Photos.PHAccessLevelReadWrite
import platform.darwin.NSObject

@Serializable
private data class ResourceRecord(
    val resourceId: String, val assetId: String, val type: String, val filename: String, val byteCount: Long,
    val capturedAt: String?, val modifiedAt: String?, val latitude: Double?, val longitude: Double?, val albumNames: List<String>,
    val pairedVideoResourceId: String?, val aaeResourceId: String?,
)

private val json = Json
private val resources = mutableMapOf<String, PHAssetResource>()
private const val resourceIdSeparator = '\u001f'
@Volatile private var discoveredAssetCount = 0
@Volatile private var totalAssetCount = 0

private fun resourceId(asset: PHAsset, resource: PHAssetResource): String =
    "${asset.localIdentifier}$resourceIdSeparator${resource.type}$resourceIdSeparator${resource.originalFilename}"

private fun retainedUtf8(value: String): CPointer<ByteVar> {
    val bytes = value.encodeToByteArray()
    return nativeHeap.allocArray<ByteVar>(bytes.size + 1).also { pointer ->
        bytes.forEachIndexed { index, byte -> pointer[index] = byte }
        pointer[bytes.size] = 0
    }
}

private fun resourceForPersistedId(id: String): PHAssetResource? {
    resources[id]?.let { return it }
    // A process restart clears the in-memory map. The identifier is intentionally sufficient to
    // resolve one known resource directly, so resume never repeats the expensive library scan.
    val pieces = id.split(resourceIdSeparator, limit = 3)
    if (pieces.size != 3) return null
    val assets = PHAsset.fetchAssetsWithLocalIdentifiers(listOf(pieces[0]), null)
    if (assets.count.toInt() != 1) return null
    val asset = assets.objectAtIndex(0u) as PHAsset
    return PHAssetResource.assetResourcesForAsset(asset)
        .filterIsInstance<PHAssetResource>()
        .firstOrNull { resource ->
        resourceId(asset, resource) == id
    }?.also { resources[id] = it }
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

/** Enumerates PHAsset/PHAssetResource once. The JVM persists returned resource IDs for resume. */
@CName("lasco_photos_discover_json")
fun discoverJson(): CPointer<ByteVar>? = memScoped {
    check(photosAccessGranted()) { "Photos permission denied" }
    val records = mutableListOf<ResourceRecord>()
    val assets = PHAsset.fetchAssetsWithOptions(null)
    discoveredAssetCount = 0
    totalAssetCount = assets.count.toInt()
    assets.enumerateObjectsUsingBlock { asset, _, _ ->
        val photo = asset as PHAsset
        val assetResources = PHAssetResource.assetResourcesForAsset(photo)
            .filterIsInstance<PHAssetResource>()
        val aae = assetResources.firstOrNull { it.type.toInt() == 9 }
        val pairedVideo = assetResources.firstOrNull { it.type.toInt() == 3 }
        assetResources.forEach { resource ->
            val id = resourceId(photo, resource)
            resources[id] = resource
            val type = when (resource.type.toInt()) { 9 -> "aae"; 3 -> "pairedVideo"; else -> "primary" }
            val coordinates = photo.location?.coordinate?.useContents { latitude to longitude }
            // PhotoKit has no public per-resource byte-size API. Avoid using private KVC and do
            // not download iCloud originals during discovery merely to calculate it.
            records += ResourceRecord(id, photo.localIdentifier, type, resource.originalFilename, 0,
                photo.creationDate?.description, photo.modificationDate?.description, coordinates?.first, coordinates?.second,
                emptyList(), pairedVideo?.let { resourceId(photo, it) }, aae?.let { resourceId(photo, it) })
        }
        discoveredAssetCount += 1
    }
    retainedUtf8(json.encodeToString(records))
}

/** Downloads an iCloud resource using PHAssetResourceManager into the caller's staging directory. */
@CName("lasco_photos_stage")
fun stage(resourceId: CPointer<ByteVar>?, directory: CPointer<ByteVar>?): CPointer<ByteVar>? = memScoped {
    val resource = resourceForPersistedId(resourceId!!.toKString()) ?: error("unknown persisted Photos resource id")
    val destination = NSURL.fileURLWithPath(directory!!.toKString())
        .URLByAppendingPathComponent("${resourceId!!.toKString().hashCode()}-${resource.originalFilename}")!!
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
