package app.lasco.importer.engine

import app.lasco.importer.ffi.LascoGateway
import app.lasco.importer.ffi.LascoRemote
import app.lasco.importer.model.ApplePhotosResourceDescriptor
import app.lasco.importer.model.ApplePhotosCollectionDescriptor
import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportPlan
import app.lasco.importer.model.ImportProgress
import app.lasco.importer.model.ImportRunState
import app.lasco.importer.model.MediaCounts
import app.lasco.importer.model.RemoteBenchmark
import app.lasco.importer.model.RemoteImportSummary
import app.lasco.importer.model.ResourceRole
import app.lasco.importer.source.ImportSourceReader
import app.lasco.importer.source.ApplePhotosCollectionSourceReader
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.nio.file.Files
import java.nio.file.Path

/** Preserves both the destination identity and the native storage error for the import UI. */
internal class RemotePushFailure(
    remote: LascoRemote,
    cause: Throwable,
) : IllegalStateException(buildMessage(remote, cause), cause) {
    private companion object {
        fun buildMessage(remote: LascoRemote, cause: Throwable): String {
            val details = cause.message?.trim().orEmpty()
            return buildString {
                append("Could not upload to remote \"")
                append(remote.name)
                append("\" (")
                append(remote.kind)
                append(").")
                if (details.isNotEmpty()) append("\n\nDetails: ").append(details)
                if (details.contains("HTTP 403", ignoreCase = true)) {
                    append("\n\nHTTP 403 means the storage service denied a request. A successful speed test only confirms that a small temporary write and delete worked at that moment; an import later performs additional reads, listings, and writes. This response does not identify which operation was denied. Check the remote credentials, request signing, and read/list/write/delete permissions.")
                }
            }
        }
    }
}

/**
 * An importer session lives only for the lifetime of this process. An interrupted import is
 * recovered by reopening its normal Lasco library and rescanning the source, never by replaying
 * a separate importer checkpoint.
 */
class ImportCoordinator(
    private val gateway: LascoGateway,
    private val stagingRoot: Path,
    private val finalize: suspend () -> String?,
) {
    private data class ImportSession(
        val reader: ImportSourceReader,
        val assets: List<ImportAsset>,
        val chunkSize: Int,
        val totalBytes: Long,
        val importedMediaBySourceId: MutableMap<String, String> = mutableMapOf(),
        val albumIdsByName: MutableMap<String, String> = mutableMapOf(),
        val albumIdsByCloudCollectionId: MutableMap<String, String> = mutableMapOf(),
        val linkableMediaByCloudAssetId: MutableMap<String, String> = mutableMapOf(),
        val linkableMediaBySessionHandle: MutableMap<String, String> = mutableMapOf(),
        val appleCollections: List<ApplePhotosCollectionDescriptor> = emptyList(),
        var nextAssetIndex: Int = 0,
        var pauseRequested: Boolean = false,
    )

    private val _progress = MutableStateFlow(ImportProgress(ImportRunState.READY, 0, 0, 0, 0))
    val progress: StateFlow<ImportProgress> = _progress
    private var session: ImportSession? = null

    suspend fun discover(reader: ImportSourceReader, chunkSize: Int = 32): ImportPlan {
        _progress.value = ImportProgress(ImportRunState.SCANNING, 0, 0, 0, 0, detail = "Discovering ${reader.source}")
        val assets = dependencyOrder(reader.discover())
        val totalBytes = assets.sumOf(ImportAsset::byteCount)
        val collections = (reader as? ApplePhotosCollectionSourceReader)?.collectionDescriptors().orEmpty()
        session = ImportSession(reader, assets, chunkSize, totalBytes, appleCollections = collections)
        val knownMediaByAsset = alreadyImportedMedia(assets)
        val remoteSummaries = gateway.remotes().map { remote ->
            gateway.confirmRemoteMedia(remote)
            val confirmedMediaIds = gateway.confirmedRemoteMediaIds(remote, knownMediaByAsset.values.toSet())
            val alreadyThere = assets.filter { knownMediaByAsset[it] in confirmedMediaIds }
            RemoteImportSummary(
                remoteId = remote.id,
                remoteName = remote.name,
                remoteType = remote.kind,
                alreadyThere = mediaCounts(alreadyThere),
                toUpload = mediaCounts(assets.filterNot { knownMediaByAsset[it] in confirmedMediaIds }),
            )
        }
        val hasMediaToUpload = remoteSummaries.any { it.toUpload.resourceCount() > 0 }
        val plan = ImportPlan(
            source = reader.source,
            candidates = assets.size,
            candidatesBytes = totalBytes,
            alreadyCompleted = knownMediaByAsset.size,
            chunkSize = chunkSize,
            remoteNames = gateway.remotes().map { it.name },
            estimatedSeconds = null,
            library = mediaCounts(assets),
            remotes = remoteSummaries,
            metadataToAdd = !hasMediaToUpload && hasApplePhotosMetadataToAdd(assets, knownMediaByAsset, collections),
            hasMediaToUpload = hasMediaToUpload,
        )
        _progress.value = ImportProgress(ImportRunState.READY, 0, assets.size, 0, totalBytes, detail = "Ready to import")
        return plan
    }

    /** Runs isolated and simultaneous remote benchmarks. A low simultaneous ratio warns the UI. */
    suspend fun benchmark(remotes: List<LascoRemote>): List<RemoteBenchmark> = coroutineScope {
        val isolated = remotes.map { remote -> gateway.benchmark(remote) }
        val combined = remotes.map { remote -> async { gateway.benchmark(remote) } }.awaitAll().associateBy { it.remoteId }
        isolated.map { result -> result.copy(simultaneousBytesPerSecond = combined.getValue(result.remoteId).isolatedBytesPerSecond) }
    }

    suspend fun startOrResume(benchmarks: List<RemoteBenchmark>) {
        val activeSession = requireNotNull(session) { "Scan a source before starting the import" }
        val remotes = gateway.remotes()
        require(remotes.isNotEmpty()) { "Select at least one non-USB remote" }
        ensureApplePhotosCollections(activeSession)

        while (activeSession.nextAssetIndex < activeSession.assets.size) {
            val chunkStart = activeSession.nextAssetIndex
            val chunk = activeSession.assets.drop(chunkStart).take(activeSession.chunkSize)
            val chunkNumber = (chunkStart / activeSession.chunkSize) + 1
            _progress.value = progressFor(activeSession, ImportRunState.IMPORTING, chunkNumber, "Staging and importing chunk $chunkNumber")
            importChunk(activeSession, chunk, chunkNumber)
            pushAll(remotes, benchmarks)
            activeSession.nextAssetIndex += chunk.size
            if (activeSession.pauseRequested) {
                activeSession.pauseRequested = false
                _progress.value = progressFor(activeSession, ImportRunState.PAUSED, null, "Paused after chunk $chunkNumber")
                return
            }
        }

        _progress.value = progressFor(activeSession, ImportRunState.IMPORTING, null, "Finishing every destination")
        pushAll(remotes, benchmarks)
        val cleanupWarning = finalize()
        _progress.value = progressFor(
            activeSession,
            if (cleanupWarning == null) ImportRunState.COMPLETE else ImportRunState.COMPLETE_WITH_CLEANUP_WARNING,
            null,
            cleanupWarning ?: "Import complete",
        )
    }

    fun requestPause() {
        session?.pauseRequested = true
        _progress.value = _progress.value.copy(state = ImportRunState.PAUSE_REQUESTED, detail = "Pausing after this chunk")
    }

    private suspend fun importChunk(session: ImportSession, assets: List<ImportAsset>, chunkNumber: Int) {
        val alreadyImported = alreadyImportedMedia(assets)
        alreadyImported.forEach { (asset, mediaId) ->
            session.importedMediaBySourceId[asset.sourceId] = mediaId
            addAlbumMembership(session, asset, mediaId)
        }
        assets.filterNot(alreadyImported::containsKey).forEach { asset ->
            val staged = session.reader.stage(asset, stagingRoot.resolve(chunkNumber.toString()))
            try {
                val aaeId = asset.aaeSourceId?.let(session.importedMediaBySourceId::get)
                val videoId = asset.liveVideoSourceId?.let(session.importedMediaBySourceId::get)
                if (asset.resourceRole == ResourceRole.PRIMARY) {
                    asset.aaeSourceId?.let { require(aaeId != null) { "missing staged AAE companion: $it" } }
                    asset.liveVideoSourceId?.let { require(videoId != null) { "missing staged Live Photo video: $it" } }
                }
                val imported = gateway.importMedia(staged.path, asset.metadata, aaeId, videoId)
                if (asset.applePhotosRevision?.cloudAssetId != null && asset.applePhotosResourceType != null) {
                    gateway.recordApplePhotosResourceOrigin(
                        imported.mediaId,
                        asset.applePhotosRevision,
                        asset.applePhotosResourceType,
                        asset.displayName,
                    )
                }
                session.importedMediaBySourceId[asset.sourceId] = imported.mediaId
                addAlbumMembership(session, asset, imported.mediaId)
            } finally {
                Files.deleteIfExists(staged.path)
            }
        }
    }

    /** Resolves a complete parent asset in one metadata-only lookup, never per resource. */
    private fun alreadyImportedMedia(assets: List<ImportAsset>): Map<ImportAsset, String> = buildMap {
        assets.groupBy { it.applePhotosRevision }.forEach { (revision, members) ->
            if (revision?.cloudAssetId == null) return@forEach
            val mediaIds = gateway.applePhotosAssetRevisionMediaIds(revision) ?: return@forEach
            members.forEach { asset ->
                val type = asset.applePhotosResourceType ?: return@forEach
                mediaIds[ApplePhotosResourceDescriptor(type, asset.displayName)]?.let { put(asset, it) }
            }
        }
    }

    private fun mediaCounts(assets: List<ImportAsset>) = assets.fold(MediaCounts()) { counts, asset ->
        when (asset.resourceRole) {
            ResourceRole.AAE_SIDECAR -> counts.copy(aaeFiles = counts.aaeFiles + 1, bytes = counts.bytes + asset.byteCount)
            ResourceRole.LIVE_PHOTO_VIDEO -> counts.copy(livePhotoVideos = counts.livePhotoVideos + 1, bytes = counts.bytes + asset.byteCount)
            ResourceRole.PRIMARY -> when (asset.applePhotosResourceType) {
                app.lasco.importer.model.ApplePhotosResourceType.VIDEO,
                app.lasco.importer.model.ApplePhotosResourceType.FULL_SIZE_VIDEO,
                app.lasco.importer.model.ApplePhotosResourceType.PAIRED_VIDEO,
                app.lasco.importer.model.ApplePhotosResourceType.FULL_SIZE_PAIRED_VIDEO,
                -> counts.copy(videos = counts.videos + 1, bytes = counts.bytes + asset.byteCount)
                else -> counts.copy(photos = counts.photos + 1, bytes = counts.bytes + asset.byteCount)
            }
        }
    }

    /** This is needed only when every source resource is already present on every remote. */
    private fun hasApplePhotosMetadataToAdd(
        assets: List<ImportAsset>,
        knownMediaByAsset: Map<ImportAsset, String>,
        collections: List<ApplePhotosCollectionDescriptor>,
    ): Boolean {
        if (collections.isEmpty()) return false
        val albumsByCollection = collections.zip(gateway.applePhotosCollectionLinks(collections)).toMap()
        if (albumsByCollection.values.any { it == null }) return true
        return assets.asSequence()
            .filter { it.resourceRole == ResourceRole.PRIMARY }
            .any { asset ->
                val mediaId = knownMediaByAsset[asset] ?: return@any false
                val currentAlbumIds = gateway.mediaAlbumIds(mediaId)
                collections.any { collection ->
                    val belongsToCollection = asset.applePhotosRevision?.cloudAssetId in collection.memberCloudAssetIds ||
                        asset.assetSessionHandle in collection.memberSessionHandles
                    belongsToCollection && albumsByCollection.getValue(collection) !in currentAlbumIds
                }
            }
    }

    private fun MediaCounts.resourceCount(): Int = photos + videos + livePhotoVideos + aaeFiles

    private fun addAlbumMembership(session: ImportSession, asset: ImportAsset, mediaId: String) {
        if (asset.resourceRole != ResourceRole.PRIMARY) return
        asset.applePhotosRevision?.cloudAssetId?.let { session.linkableMediaByCloudAssetId[it] = mediaId }
        asset.assetSessionHandle?.let { session.linkableMediaBySessionHandle[it] = mediaId }
        session.appleCollections.forEach { collection ->
            val isMember = asset.applePhotosRevision?.cloudAssetId in collection.memberCloudAssetIds ||
                asset.assetSessionHandle in collection.memberSessionHandles
            if (isMember) session.albumIdsByCloudCollectionId[collection.cloudCollectionId]?.let { albumId ->
                gateway.addMediaToAlbum(albumId, mediaId)
            }
        }
        asset.albumNames.forEach { albumName ->
            val albumId = session.albumIdsByName.getOrPut(albumName) { gateway.createAlbum(albumName) }
            gateway.addMediaToAlbum(albumId, mediaId)
        }
    }

    /**
     * Links are looked up once per scan, then created in native parent-first order. Reused albums
     * are intentionally left untouched: reimport may add members but never rename or reparent.
     */
    private fun ensureApplePhotosCollections(session: ImportSession) {
        if (session.appleCollections.isEmpty() || session.albumIdsByCloudCollectionId.isNotEmpty()) return
        val existing = gateway.applePhotosCollectionLinks(session.appleCollections)
        session.appleCollections.zip(existing).forEach { (collection, albumId) ->
            albumId?.let { session.albumIdsByCloudCollectionId[collection.cloudCollectionId] = it }
        }
        session.appleCollections.forEach { collection ->
            if (collection.cloudCollectionId in session.albumIdsByCloudCollectionId) return@forEach
            val parentId = collection.parentCloudCollectionId
                ?.let(session.albumIdsByCloudCollectionId::get)
            val albumId = gateway.createAlbum(collection.name, parentId)
            gateway.recordApplePhotosCollectionLink(albumId, collection)
            session.albumIdsByCloudCollectionId[collection.cloudCollectionId] = albumId
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
            // A companion can never be its own prerequisite. Ignore malformed legacy bridge
            // metadata here so a rescan can proceed; real multi-asset cycles still fail loudly.
            asset.aaeSourceId?.takeUnless { it == asset.sourceId }?.let(byId::get)?.let(::visit)
            asset.liveVideoSourceId?.takeUnless { it == asset.sourceId }?.let(byId::get)?.let(::visit)
            visiting.remove(asset.sourceId)
            emitted += asset.sourceId
            ordered += asset
        }
        assets.forEach(::visit)
        return ordered
    }

    private suspend fun pushAll(remotes: List<LascoRemote>, benchmarks: List<RemoteBenchmark>) = coroutineScope {
        remotes.map { remote -> async {
            val parallelism = benchmarks.firstOrNull { it.remoteId == remote.id }?.selectedParallelism ?: 2
            try {
                gateway.push(remote, parallelism) { fraction ->
                    _progress.value = _progress.value.copy(detail = "Pushing ${remote.name}: ${(fraction * 100).toInt()}%")
                }
            } catch (failure: Throwable) {
                if (failure is CancellationException) throw failure
                throw RemotePushFailure(remote, failure)
            }
        } }.awaitAll()
    }

    private fun progressFor(session: ImportSession, state: ImportRunState, chunk: Int?, detail: String) = ImportProgress(
        state = state,
        completedAssets = session.nextAssetIndex,
        totalAssets = session.assets.size,
        completedBytes = 0,
        totalBytes = session.totalBytes,
        activeChunk = chunk,
        detail = detail,
    )
}
