package app.lasco.importer.ffi

import app.lasco.importer.model.ImportedMedia
import app.lasco.importer.model.ApplePhotosAssetRevision
import app.lasco.importer.model.ApplePhotosCollectionDescriptor
import app.lasco.importer.model.ApplePhotosResourceDescriptor
import app.lasco.importer.model.ApplePhotosResourceType
import app.lasco.importer.model.RemoteBenchmark
import app.lasco.importer.model.SourceMetadata
import java.nio.file.Path

data class LascoRemote(val id: String, val name: String, val kind: String)

/** The only importer API over the existing generated Kotlin/JNA bindings. */
interface LascoGateway : AutoCloseable {
    val libraryId: String
    fun remotes(): List<LascoRemote>
    fun createAlbum(name: String, parentAlbumId: String? = null): String
    fun addMediaToAlbum(albumId: String, mediaId: String)
    fun importMedia(path: Path, metadata: SourceMetadata, aaeMediaId: String?, liveVideoMediaId: String?): ImportedMedia
    fun applePhotosAssetRevisionMediaIds(revision: ApplePhotosAssetRevision): Map<ApplePhotosResourceDescriptor, String>?
    fun recordApplePhotosResourceOrigin(mediaId: String, revision: ApplePhotosAssetRevision, resourceType: ApplePhotosResourceType, filename: String)
    fun applePhotosCollectionLinks(collections: List<ApplePhotosCollectionDescriptor>): List<String?>
    fun mediaAlbumIds(mediaId: String): Set<String>
    fun recordApplePhotosCollectionLink(albumId: String, collection: ApplePhotosCollectionDescriptor)
    /** Downloads one remote's metadata operations and merges them into the local library state. */
    suspend fun fetchRemoteOperations(remote: LascoRemote)
    /** Whether this remote is missing a logical operation that is known in the local state. */
    fun hasUnpushedOperations(remote: LascoRemote): Boolean
    /** Refreshes the remote inventory before presenting the scan summary. */
    suspend fun confirmRemoteMedia(remote: LascoRemote)
    /** Media IDs whose full original is confirmed in this remote's local inventory. */
    fun confirmedRemoteMediaIds(remote: LascoRemote, mediaIds: Set<String>): Set<String>
    suspend fun benchmark(remote: LascoRemote, bytesPerUpload: Long = 4L * 1024 * 1024): RemoteBenchmark
    suspend fun push(remote: LascoRemote, maxConcurrentMediaUploads: Int, onProgress: (Double) -> Unit)
}
