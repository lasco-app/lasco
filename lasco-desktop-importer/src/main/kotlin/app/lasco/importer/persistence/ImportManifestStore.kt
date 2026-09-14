package app.lasco.importer.persistence

import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportRunState
import app.lasco.importer.model.ImportSource
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption.ATOMIC_MOVE
import java.nio.file.StandardCopyOption.REPLACE_EXISTING
import java.time.Instant

/**
 * Durable importer checkpoint stored as one local JSON file. Each update is written to a
 * temporary file and atomically moved into place, so closing the app never leaves a half-written
 * manifest. It lets a paused import resume without discovering Apple Photos again.
 */
class ImportManifestStore(private val appData: Path) : AutoCloseable {
    private val json = Json { ignoreUnknownKeys = true; prettyPrint = true }
    private val manifestPath = appData.resolve("importer.json")
    private var document: ManifestDocument

    init {
        Files.createDirectories(appData)
        document = if (Files.exists(manifestPath)) {
            json.decodeFromString(Files.readString(manifestPath))
        } else {
            ManifestDocument()
        }
    }

    fun createJob(id: String, source: ImportSource, chunkSize: Int) = update {
        if (id !in it.jobs) {
            val now = Instant.now().toString()
            it.copy(jobs = it.jobs + (id to PersistedJob(source, ImportRunState.SCANNING, chunkSize, createdAt = now, updatedAt = now)))
        } else {
            it
        }
    }

    fun recordDiscovery(jobId: String, assets: List<ImportAsset>) = updateJob(jobId) { job ->
        val known = job.assets.mapTo(mutableSetOf()) { it.asset.sourceId }
        job.copy(assets = job.assets + assets.filter { known.add(it.sourceId) }.map { PersistedAsset(it) })
    }

    fun markReady(jobId: String) = setState(jobId, ImportRunState.READY)
    fun markImporting(jobId: String) = setState(jobId, ImportRunState.IMPORTING)
    fun markComplete(jobId: String) = setState(jobId, ImportRunState.COMPLETE)
    fun markFailed(jobId: String, message: String) = setState(jobId, ImportRunState.FAILED, message)

    fun requestPause(jobId: String) = updateJob(jobId) { it.copy(pauseRequested = true, state = ImportRunState.PAUSE_REQUESTED) }

    fun consumePauseRequest(jobId: String): Boolean = synchronized(this) {
        val requested = job(jobId).pauseRequested
        if (requested) updateJob(jobId) { it.copy(pauseRequested = false, state = ImportRunState.PAUSED) }
        requested
    }

    fun incompleteAssets(jobId: String): List<StoredAsset> = synchronized(this) {
        job(jobId).assets.asSequence()
            .filter { it.mediaId == null }
            .sortedBy { it.asset.sourceId }
            .map { it.toStoredAsset() }
            .toList()
    }

    fun completedCount(jobId: String): Int = synchronized(this) { job(jobId).assets.count { it.mediaId != null } }
    fun totalCount(jobId: String): Int = synchronized(this) { job(jobId).assets.size }
    fun totalBytes(jobId: String): Long = synchronized(this) { job(jobId).assets.sumOf { it.asset.byteCount } }

    fun markStaged(jobId: String, sourceId: String, stagedPath: Path) = updateJob(jobId) { job ->
        job.copy(assets = job.assets.map { asset ->
            if (asset.asset.sourceId == sourceId) asset.copy(stagedPath = stagedPath.toString()) else asset
        })
    }

    fun markImported(jobId: String, sourceId: String, mediaId: String, alreadyExisted: Boolean, chunk: Int) = updateJob(jobId) { job ->
        job.copy(assets = job.assets.map { asset ->
            if (asset.asset.sourceId == sourceId) asset.copy(mediaId = mediaId, alreadyExisted = alreadyExisted, chunkNumber = chunk) else asset
        })
    }

    fun mediaIdFor(jobId: String, sourceId: String): String? = synchronized(this) {
        job(jobId).assets.firstOrNull { it.asset.sourceId == sourceId }?.mediaId
    }

    fun markRemoteChunkComplete(jobId: String, chunk: Int, remoteId: String) = updateJob(jobId) { job ->
        val acknowledgement = RemoteChunkAcknowledgement(chunk, remoteId)
        if (acknowledgement in job.remoteChunks) job else job.copy(remoteChunks = job.remoteChunks + acknowledgement)
    }

    fun importedChunks(jobId: String): List<Int> = synchronized(this) {
        job(jobId).assets.mapNotNull { it.chunkNumber }.distinct().sorted()
    }

    fun nextChunkNumber(jobId: String): Int = synchronized(this) {
        (job(jobId).assets.mapNotNull { it.chunkNumber }.maxOrNull() ?: 0) + 1
    }

    fun remoteChunkCompleted(jobId: String, chunk: Int, remoteId: String): Boolean = synchronized(this) {
        RemoteChunkAcknowledgement(chunk, remoteId) in job(jobId).remoteChunks
    }

    fun mediaIdsForChunk(jobId: String, chunk: Int): List<String> = synchronized(this) {
        job(jobId).assets.filter { it.chunkNumber == chunk }.mapNotNull { it.mediaId }
    }

    fun albumId(jobId: String, sourceAlbumName: String): String? = synchronized(this) {
        job(jobId).albums[sourceAlbumName]
    }

    fun recordAlbum(jobId: String, sourceAlbumName: String, lascoAlbumId: String) = updateJob(jobId) { job ->
        if (sourceAlbumName in job.albums) job else job.copy(albums = job.albums + (sourceAlbumName to lascoAlbumId))
    }

    private fun setState(jobId: String, state: ImportRunState, failure: String? = null) = updateJob(jobId) {
        it.copy(state = state, failure = failure)
    }

    private fun updateJob(jobId: String, transform: (PersistedJob) -> PersistedJob) = update { state ->
        val previous = state.jobs[jobId] ?: error("Unknown import job: $jobId")
        val next = transform(previous).copy(updatedAt = Instant.now().toString())
        state.copy(jobs = state.jobs + (jobId to next))
    }

    private fun job(jobId: String): PersistedJob = document.jobs[jobId] ?: error("Unknown import job: $jobId")

    private fun update(transform: (ManifestDocument) -> ManifestDocument) = synchronized(this) {
        val next = transform(document)
        writeAtomically(next)
        document = next
    }

    private fun writeAtomically(next: ManifestDocument) {
        val temporary = Files.createTempFile(appData, "importer-", ".json.tmp")
        try {
            Files.writeString(temporary, json.encodeToString(next))
            try {
                Files.move(temporary, manifestPath, ATOMIC_MOVE, REPLACE_EXISTING)
            } catch (_: AtomicMoveNotSupportedException) {
                Files.move(temporary, manifestPath, REPLACE_EXISTING)
            }
        } finally {
            Files.deleteIfExists(temporary)
        }
    }

    override fun close() = Unit
}

@Serializable
private data class ManifestDocument(val version: Int = 1, val jobs: Map<String, PersistedJob> = emptyMap())

@Serializable
private data class PersistedJob(
    val source: ImportSource,
    val state: ImportRunState,
    val chunkSize: Int,
    val pauseRequested: Boolean = false,
    val createdAt: String,
    val updatedAt: String,
    val failure: String? = null,
    val assets: List<PersistedAsset> = emptyList(),
    val albums: Map<String, String> = emptyMap(),
    val remoteChunks: List<RemoteChunkAcknowledgement> = emptyList(),
)

@Serializable
private data class PersistedAsset(
    val asset: ImportAsset,
    val stagedPath: String? = null,
    val mediaId: String? = null,
    val alreadyExisted: Boolean? = null,
    val chunkNumber: Int? = null,
) {
    fun toStoredAsset() = StoredAsset(asset, stagedPath?.let(Path::of), mediaId, chunkNumber)
}

@Serializable
private data class RemoteChunkAcknowledgement(val chunk: Int, val remoteId: String)

data class StoredAsset(val asset: ImportAsset, val stagedPath: Path?, val mediaId: String?, val chunkNumber: Int?)
