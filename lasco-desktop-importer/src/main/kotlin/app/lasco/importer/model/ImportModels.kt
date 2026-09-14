package app.lasco.importer.model

import kotlinx.serialization.Serializable
import java.nio.file.Path
import java.time.Instant

@Serializable
enum class ImportSource { GOOGLE_TAKEOUT, APPLE_PHOTOS }

@Serializable
enum class ImportRunState { SCANNING, READY, IMPORTING, PAUSE_REQUESTED, PAUSED, COMPLETE, FAILED }

@Serializable
enum class ResourceRole { AAE_SIDECAR, LIVE_PHOTO_VIDEO, PRIMARY }

@Serializable
enum class ApplePhotosResourceType { PHOTO, FULL_SIZE_PHOTO, VIDEO, FULL_SIZE_VIDEO, ADJUSTMENT_DATA, PAIRED_VIDEO, FULL_SIZE_PAIRED_VIDEO }

@Serializable
data class ApplePhotosResourceDescriptor(val type: ApplePhotosResourceType, val filename: String)

@Serializable
data class ApplePhotosAssetRevision(
    val cloudAssetId: String,
    val modificationDate: String?,
    val resources: List<ApplePhotosResourceDescriptor>,
)

@Serializable
data class SourceMetadata(
    val originalFilename: String,
    val capturedAt: String? = null,
    val modifiedAt: String? = null,
    val latitude: Double? = null,
    val longitude: Double? = null,
)

/** A persisted source reference. It deliberately contains no Photos entitlement or remote secret. */
@Serializable
data class ImportAsset(
    val sourceId: String,
    val source: ImportSource,
    val resourceRole: ResourceRole,
    val displayName: String,
    val byteCount: Long,
    val metadata: SourceMetadata,
    val sourceLocator: String,
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
