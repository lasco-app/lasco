package app.lasco.importer.ffi

import app.lasco.importer.model.ImportedMedia
import app.lasco.importer.model.ApplePhotosAssetRevision
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
    fun createAlbum(name: String): String
    fun addMediaToAlbum(albumId: String, mediaId: String)
    fun importMedia(path: Path, metadata: SourceMetadata, aaeMediaId: String?, liveVideoMediaId: String?): ImportedMedia
    fun applePhotosAssetRevisionMediaIds(revision: ApplePhotosAssetRevision): Map<ApplePhotosResourceDescriptor, String>?
    fun recordApplePhotosResourceOrigin(mediaId: String, revision: ApplePhotosAssetRevision, resourceType: ApplePhotosResourceType, filename: String)
    suspend fun benchmark(remote: LascoRemote, bytesPerUpload: Long = 4L * 1024 * 1024): RemoteBenchmark
    suspend fun push(remote: LascoRemote, maxConcurrentMediaUploads: Int, onProgress: (Double) -> Unit)
}
