package app.lasco.importer.model

import java.nio.file.Path
import java.time.Instant

enum class ImportSource { GOOGLE_TAKEOUT, APPLE_PHOTOS }

enum class ImportRunState { SCANNING, READY, IMPORTING, PAUSE_REQUESTED, PAUSED, COMPLETE, COMPLETE_WITH_CLEANUP_WARNING, FAILED }

enum class ResourceRole { AAE_SIDECAR, LIVE_PHOTO_VIDEO, PRIMARY }

enum class ApplePhotosResourceType { PHOTO, FULL_SIZE_PHOTO, VIDEO, FULL_SIZE_VIDEO, ADJUSTMENT_DATA, PAIRED_VIDEO, FULL_SIZE_PAIRED_VIDEO }

data class ApplePhotosResourceDescriptor(val type: ApplePhotosResourceType, val filename: String)

data class ApplePhotosAssetRevision(
    val cloudAssetId: String?,
    val modificationDate: String?,
    val resources: List<ApplePhotosResourceDescriptor>,
)
enum class ApplePhotosCollectionKind { FOLDER, ALBUM }
data class ApplePhotosCollectionDescriptor(
    val cloudCollectionId: String,
    val kind: ApplePhotosCollectionKind,
    val name: String,
    val parentCloudCollectionId: String?,
    val memberCloudAssetIds: List<String>,
    /** Current-process-only handles for unmapped members. Never persist these values. */
    val memberSessionHandles: List<String>,
)

data class SourceMetadata(
    val originalFilename: String,
    val capturedAt: String? = null,
    val modifiedAt: String? = null,
    val latitude: Double? = null,
    val longitude: Double? = null,
)

/**
 * A source item for the current import process only. `sourceLocator` may be an opaque PhotoKit
 * staging handle; it is deliberately never serialized or retained across application restarts.
 */
data class ImportAsset(
    val sourceId: String,
    val source: ImportSource,
    val resourceRole: ResourceRole,
    val displayName: String,
    val byteCount: Long,
    val metadata: SourceMetadata,
    val sourceLocator: String,
    /** Opaque PhotoKit handle for this run only; absent for non-PhotoKit readers. */
    val assetSessionHandle: String? = null,
    val albumNames: List<String> = emptyList(),
    val aaeSourceId: String? = null,
    val liveVideoSourceId: String? = null,
    val applePhotosRevision: ApplePhotosAssetRevision? = null,
    val applePhotosResourceType: ApplePhotosResourceType? = null,
)

data class StagedAsset(val asset: ImportAsset, val path: Path)

data class ImportPlan(
    val source: ImportSource,
    val candidates: Int,
    val candidatesBytes: Long,
    val alreadyCompleted: Int,
    val chunkSize: Int,
    val remoteNames: List<String>,
    val estimatedSeconds: Long?,
    val library: MediaCounts,
    val remotes: List<RemoteImportSummary>,
)

/** Counts source resources rather than gallery entries, so Photos companions remain visible. */
data class MediaCounts(
    val photos: Int = 0,
    val videos: Int = 0,
    val livePhotoVideos: Int = 0,
    val aaeFiles: Int = 0,
    val bytes: Long = 0,
)

data class RemoteImportSummary(
    val remoteId: String,
    val remoteName: String,
    val remoteType: String,
    val alreadyThere: MediaCounts,
    val toUpload: MediaCounts,
)

data class RemoteBenchmark(
    val remoteId: String,
    val remoteName: String,
    val selectedParallelism: Int,
    val isolatedBytesPerSecond: Long,
    val simultaneousBytesPerSecond: Long? = null,
)

data class ImportProgress(
    val state: ImportRunState,
    val completedAssets: Int,
    val totalAssets: Int,
    val completedBytes: Long,
    val totalBytes: Long,
    val activeChunk: Int? = null,
    val detail: String = "",
    val updatedAt: Instant = Instant.now(),
)

data class ImportedMedia(
    val mediaId: String,
    val alreadyExisted: Boolean,
)
