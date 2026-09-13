package app.lasco.importer.engine

import app.lasco.importer.ffi.LascoGateway
import app.lasco.importer.ffi.LascoRemote
import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportPlan
import app.lasco.importer.model.ImportProgress
import app.lasco.importer.model.ImportRunState
import app.lasco.importer.model.RemoteBenchmark
import app.lasco.importer.persistence.ImportManifestStore
import app.lasco.importer.source.ImportSourceReader
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.nio.file.Files
import java.nio.file.Path
import java.util.UUID

/**
 * Chunked, resumable importer. Staging is source-specific; Lasco import/push is source-agnostic.
 * The selected remotes are pushed in parallel after every completed chunk, so no source file is
 * reread or redownloaded once per destination.
 */
class ImportCoordinator(
    private val manifest: ImportManifestStore,
    private val gateway: LascoGateway,
    private val stagingRoot: Path,
) {
    private val _progress = MutableStateFlow(ImportProgress(ImportRunState.READY, 0, 0, 0, 0))
    val progress: StateFlow<ImportProgress> = _progress

    suspend fun discover(reader: ImportSourceReader, chunkSize: Int = 32): Pair<String, ImportPlan> {
        val jobId = UUID.randomUUID().toString()
        manifest.createJob(jobId, reader.source, chunkSize)
        _progress.value = ImportProgress(ImportRunState.SCANNING, 0, 0, 0, 0, detail = "Discovering ${reader.source}")
        val assets = reader.discover()
        manifest.recordDiscovery(jobId, assets)
        manifest.markReady(jobId)
        val completed = manifest.completedCount(jobId)
        val bytes = manifest.totalBytes(jobId)
        val plan = ImportPlan(reader.source, assets.size - completed, bytes, completed, chunkSize, gateway.remotes().map { it.name }, null)
        _progress.value = ImportProgress(ImportRunState.READY, completed, assets.size, 0, bytes, detail = "Ready to import")
        return jobId to plan
    }

    /** Runs isolated and simultaneous remote benchmarks. A low simultaneous ratio warns the UI. */
    suspend fun benchmark(remotes: List<LascoRemote>): List<RemoteBenchmark> = coroutineScope {
        // First establish each target's uncongested best rate. The second pass below deliberately
        // overlaps all remote benchmarks to measure whether the user's uplink is the bottleneck.
        val isolated = remotes.map { remote -> gateway.benchmark(remote) }
        val combined = remotes.map { remote -> async { gateway.benchmark(remote) } }.awaitAll().associateBy { it.remoteId }
        isolated.map { result -> result.copy(simultaneousBytesPerSecond = combined.getValue(result.remoteId).isolatedBytesPerSecond) }
    }

    suspend fun startOrResume(jobId: String, reader: ImportSourceReader, benchmarks: List<RemoteBenchmark>) {
        val remotes = gateway.remotes()
        require(remotes.isNotEmpty()) { "Select at least one non-USB remote" }
        manifest.markImporting(jobId)
        val pending = manifest.incompleteAssets(jobId)
        val total = manifest.totalCount(jobId)
        val totalBytes = manifest.totalBytes(jobId)
        val chunkSize = 32 // persisted job chunk size is intentionally stable for paused jobs.
        // Repair an interruption after import but before every remote acknowledged a chunk. The
        // local cache is intentionally not evicted until this fan-out completed, so no source
        // restage or Photos rescan is needed.
        manifest.importedChunks(jobId).forEach { chunkNumber ->
            if (remotes.any { !manifest.remoteChunkCompleted(jobId, chunkNumber, it.id) }) {
                pushChunk(jobId, chunkNumber, remotes, benchmarks)
                gateway.evict(manifest.mediaIdsForChunk(jobId, chunkNumber))
            }
        }
        val firstNewChunk = manifest.nextChunkNumber(jobId)
        dependencyOrder(pending.map { it.asset }).chunked(chunkSize).forEachIndexed { index, assets ->
            val chunkNumber = firstNewChunk + index
            _progress.value = progressFor(jobId, total, totalBytes, chunkNumber, "Staging and importing chunk $chunkNumber")
            importChunk(jobId, reader, assets, chunkNumber)
            pushChunk(jobId, chunkNumber, remotes, benchmarks)
            if (manifest.consumePauseRequest(jobId)) {
                _progress.value = progressFor(jobId, total, totalBytes, null, "Paused after chunk $chunkNumber")
                return
            }
            // Evict only after every remote acknowledged the chunk; source stays in its original
            // Takeout archive or Photos library and can be staged again if a later repair needs it.
            gateway.evict(manifest.mediaIdsForChunk(jobId, chunkNumber))
        }
        manifest.markComplete(jobId)
        _progress.value = progressFor(jobId, total, totalBytes, null, "Import complete").copy(state = ImportRunState.COMPLETE)
    }

    fun requestPause(jobId: String) = manifest.requestPause(jobId)

    private suspend fun importChunk(jobId: String, reader: ImportSourceReader, assets: List<ImportAsset>, chunk: Int) {
        val ordered = assets.sortedBy { when (it.resourceRole) {
            app.lasco.importer.model.ResourceRole.AAE_SIDECAR -> 0
            app.lasco.importer.model.ResourceRole.LIVE_PHOTO_VIDEO -> 1
            app.lasco.importer.model.ResourceRole.PRIMARY -> 2
        } }
        ordered.forEach { asset ->
            val staged = reader.stage(asset, stagingRoot.resolve(jobId).resolve(chunk.toString()))
            manifest.markStaged(jobId, asset.sourceId, staged.path)
            val aaeId = asset.aaeSourceId?.let { manifest.mediaIdFor(jobId, it) }
            val videoId = asset.liveVideoSourceId?.let { manifest.mediaIdFor(jobId, it) }
            // A primary Live Photo must never be imported until both companions were present in
            // this or an earlier durable chunk. This makes the relationship recoverable on resume.
            if (asset.resourceRole == app.lasco.importer.model.ResourceRole.PRIMARY) {
                asset.aaeSourceId?.let { require(aaeId != null) { "missing staged AAE companion: $it" } }
                asset.liveVideoSourceId?.let { require(videoId != null) { "missing staged Live Photo video: $it" } }
            }
            val imported = gateway.importMedia(staged.path, asset.metadata, aaeId, videoId)
            manifest.markImported(jobId, asset.sourceId, imported.mediaId, imported.alreadyExisted, chunk)
            asset.albumNames.forEach { albumName ->
                val albumId = manifest.albumId(jobId, albumName) ?: gateway.createAlbum(albumName).also {
                    manifest.recordAlbum(jobId, albumName, it)
                }
                gateway.addMediaToAlbum(albumId, imported.mediaId)
            }
            Files.deleteIfExists(staged.path)
        }
    }

    /** Topological ordering keeps each edited Live Photo's AAE → paired video → still sequence. */
    private fun dependencyOrder(assets: List<ImportAsset>): List<ImportAsset> {
        val byId = assets.associateBy { it.sourceId }
        val visiting = mutableSetOf<String>()
        val emitted = mutableSetOf<String>()
        val ordered = mutableListOf<ImportAsset>()
        fun visit(asset: ImportAsset) {
            if (asset.sourceId in emitted) return
            check(visiting.add(asset.sourceId)) { "cyclic Photos companion relationship at ${asset.sourceId}" }
            asset.aaeSourceId?.let(byId::get)?.let(::visit)
            asset.liveVideoSourceId?.let(byId::get)?.let(::visit)
            visiting.remove(asset.sourceId)
            emitted += asset.sourceId
            ordered += asset
        }
        assets.forEach(::visit)
        return ordered
    }

    private suspend fun pushChunk(jobId: String, chunk: Int, remotes: List<LascoRemote>, benchmarks: List<RemoteBenchmark>) = coroutineScope {
        remotes.map { remote -> async {
            if (manifest.remoteChunkCompleted(jobId, chunk, remote.id)) return@async
            val parallelism = benchmarks.firstOrNull { it.remoteId == remote.id }?.selectedParallelism ?: 2
            gateway.push(remote, parallelism) { fraction ->
                _progress.value = _progress.value.copy(detail = "Pushing ${remote.name}: ${(fraction * 100).toInt()}%")
            }
            manifest.markRemoteChunkComplete(jobId, chunk, remote.id)
        } }.awaitAll()
    }

    private fun progressFor(jobId: String, total: Int, totalBytes: Long, chunk: Int?, detail: String): ImportProgress {
        val completed = manifest.completedCount(jobId)
        return ImportProgress(ImportRunState.IMPORTING, completed, total, 0, totalBytes, chunk, detail)
    }
}
