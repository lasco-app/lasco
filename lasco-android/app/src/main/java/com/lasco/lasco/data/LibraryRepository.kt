package com.lasco.lasco.data

import android.content.Context
import com.lasco.lasco.LascoApp
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.withContext
import com.lasco.lasco.media.IncrementalDeviceMediaImporter
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem
import uniffi.lasco_ffi.FfiLibrary
import uniffi.lasco_ffi.FfiLocalStateStats
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiMediaNeighbors
import uniffi.lasco_ffi.FfiMediaOrGroupNeighbors
import uniffi.lasco_ffi.FfiOperationGroup
import uniffi.lasco_ffi.FfiSyncResult

/**
 * Session scoped wrapper around an opened FfiLibrary, replacing the Swift
 * LibraryModel god object. Rust never pushes change notifications, so screens
 * share only the changes invalidation signal and each pulls its own snapshot
 * via watch. sync.syncState is the one exception, shared so StatusScreen and
 * RemotesScreen agree on push/fetch busy state.
 *
 * Only calls whose cost scales with the amount of data are wrapped in the
 * injected io dispatcher. Short blocking FfiLibrary calls run on the caller's
 * dispatcher, which is usually Main. The UniFFI async methods (syncAsync,
 * fetchRemoteAsync, pushRemoteAsync, getMediaThumbnailAsync,
 * getMediaBytesAsync) are already suspend on their own executor and must not
 * be wrapped again.
 */
class LibraryRepository(
    private val lib: FfiLibrary,
    private val nickname: String,
    private val username: String,
    private val appDir: String,
    context: Context,
    private val prefs: Prefs,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    private val appContext = context.applicationContext

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private val changes = MutableSharedFlow<Change>(extraBufferCapacity = 64)

    // Separate from changes, which also fires on remote refreshes (Change.All)
    // and would wrongly trigger auto push if reused here.
    private val localMutations = MutableSharedFlow<Unit>(extraBufferCapacity = 64)

    // Screen ViewModels are activity-scoped, so their StateFlows can outlive the
    // composable that collected them. Closing this gate cancels all repository
    // watches immediately instead of waiting for WhileSubscribed's timeout.
    private val closed = MutableStateFlow(false)

    private val _sessionState = MutableStateFlow(buildSessionState())
    val sessionState: StateFlow<SessionState> = _sessionState.asStateFlow()

    val sync = SyncController(lib = lib, prefs = prefs, onLibraryChanged = { changes.emit(Change.All) }, scope = scope)

    private val incrementalImporter = IncrementalDeviceMediaImporter(
        lib = lib,
        context = appContext,
        prefs = prefs,
        onStateChanged = sync::setIncrementalImportState,
        onImported = ::notifyBatchedLocalMutation,
        scope = scope,
    )

    init {
        scope.launch { localMutations.collect { sync.schedulePush() } }
    }

    // Exists only so the onboarding wizard can build its own
    // InitialImportController (which needs the raw FfiLibrary, not the
    // wrapped suspend methods below, to avoid a changes/localMutations emit
    // per imported row). Not for any other caller, and not to be cached
    // beyond the session's lifetime.
    internal fun ffiLibraryForOnboardingImport(): FfiLibrary = lib

    // For code that mutates the library through a path other than this
    // repository's own wrapped methods above, e.g. the onboarding import.
    suspend fun notifyChanged() {
        changes.emit(Change.All)
    }

    internal suspend fun notifyBatchedLocalMutation() {
        changes.emit(Change.All)
        localMutations.emit(Unit)
    }

    // Must be called on session end (sign out, delete), or the sync loop and
    // its localMutations collector outlive the repository.
    suspend fun close() {
        closed.value = true
        incrementalImporter.close()
        sync.close()
        scope.cancel()
    }

    /** Queues an incremental DCIM import for this open session. */
    fun importNewDeviceMediaIfNeeded() {
        incrementalImporter.requestImport()
    }

    // Reruns load on any of the given scopes (or Change.All), and once per
    // subscription, so a screen returning from the back stack always reloads
    // and never stays stale.
    @OptIn(ExperimentalCoroutinesApi::class)
    fun <T> watch(vararg scopes: Change, load: suspend () -> T): Flow<T> =
        closed.flatMapLatest { isClosed ->
            if (isClosed) {
                emptyFlow()
            } else {
                changes
                    .filter { c -> c is Change.All || scopes.any { it == c } }
                    .onStart { emit(Change.All) }
                    .mapLatest { load() }
            }
        }

    /** Converts offset/limit requests to UniFFI's inclusive UInt range. */
    private fun <T> page(offset: Int, limit: Int, fetch: (UInt, UInt) -> List<T>): List<T> {
        if (offset < 0 || limit <= 0) return emptyList()
        val start = offset.toULong()
        val end = start + limit.toULong() - 1u
        if (end > UInt.MAX_VALUE.toULong()) return emptyList()
        return fetch(start.toUInt(), end.toUInt())
    }

    private suspend fun <T> allPages(count: Int, fetch: suspend (Int, Int) -> List<T>): List<T> {
        val result = mutableListOf<T>()
        for (offset in 0 until count step PAGE_SIZE) result += fetch(offset, PAGE_SIZE)
        return result
    }

    // The FFI positions are inclusive; callers use the usual offset/limit convention.
    suspend fun mediaByDateCount(): Int = withContext(io) { lib.mediaByDateCount().toInt() }

    suspend fun mediaByDate(offset: Int, limit: Int): List<FfiMediaItem> = withContext(io) {
        page(offset, limit, lib::mediaByDateRange)
    }

    suspend fun mediaByDateAll(): List<FfiMediaItem> {
        val count = mediaByDateCount()
        return allPages(count, ::mediaByDate)
    }

    suspend fun mediaByDateNeighbors(position: Int): FfiMediaNeighbors = withContext(io) {
        require(position >= 0) { "position must be non-negative" }
        lib.mediaByDateNeighbors(position.toUInt())
    }

    // Primary media that is not reachable from any live album or group. The
    // FFI query excludes AAE and Live Photo companion resources for us.
    suspend fun orphanMediaByDateCount(): Int = withContext(io) { lib.orphanMediaByDateCount().toInt() }

    suspend fun orphanMediaByDate(offset: Int, limit: Int): List<FfiMediaItem> = withContext(io) {
        page(offset, limit, lib::orphanMediaByDateRange)
    }

    suspend fun albumChildrenCount(parentAlbumId: String?): Int = withContext(io) {
        lib.albumAlbumsCount(parentAlbumId).toInt()
    }

    suspend fun albumChildren(parentAlbumId: String?, offset: Int, limit: Int): List<FfiAlbum> = withContext(io) {
        page(offset, limit) { start, end -> lib.albumAlbumsRange(parentAlbumId, start, end) }
    }

    suspend fun disconnectedAlbumsCount(): Int = withContext(io) { lib.disconnectedAlbumsCount().toInt() }

    suspend fun disconnectedAlbums(offset: Int, limit: Int): List<FfiAlbum> = withContext(io) {
        page(offset, limit, lib::disconnectedAlbumsRange)
    }

    /** Loads the navigation tree a page at a time, breadth first. */
    suspend fun allAlbums(): List<FfiAlbum> {
        val albums = mutableListOf<FfiAlbum>()
        val parents = ArrayDeque<String?>().apply { add(null) }
        val visitedParents = mutableSetOf<String?>()
        while (parents.isNotEmpty()) {
            val parentId = parents.removeFirst()
            if (!visitedParents.add(parentId)) continue
            val count = albumChildrenCount(parentId)
            val children = allPages(count) { offset, limit -> albumChildren(parentId, offset, limit) }
            albums += children
            children.forEach { parents.add(it.albumId) }
        }
        return albums
    }

    suspend fun albumItemsCount(albumId: String): Int = withContext(io) { lib.albumItemsCount(albumId).toInt() }

    suspend fun albumItemsByDate(
        albumId: String,
        ascending: Boolean,
        offset: Int,
        limit: Int,
    ): List<FfiAlbumItem> = withContext(io) {
        page(offset, limit) { start, end -> lib.albumItemsByDateRange(albumId, ascending, start, end) }
    }

    suspend fun albumItemsSorted(albumId: String, ascending: Boolean): List<FfiAlbumItem> {
        val count = albumItemsCount(albumId)
        return allPages(count) { offset, limit -> albumItemsByDate(albumId, ascending, offset, limit) }
    }

    suspend fun albumItemsByDateNeighbors(
        albumId: String,
        ascending: Boolean,
        position: Int,
    ): FfiMediaOrGroupNeighbors = withContext(io) {
        require(position >= 0) { "position must be non-negative" }
        lib.albumItemsByDateNeighbors(albumId, ascending, position.toUInt())
    }

    suspend fun mediaInAlbum(albumId: String): List<FfiMediaItem> =
        albumItemsSorted(albumId, ascending = false).mapNotNull { it.media }

    suspend fun showMedia(mediaId: String): FfiMediaItem = lib.showMedia(mediaId)

    suspend fun renameMedia(mediaId: String, name: String?) {
        lib.renameMedia(mediaId, name)
        changes.emit(Change.Media(mediaId))
        changes.emit(Change.MediaList)
        // Membership unknown, so start broad and narrow to
        // mediaContainingAlbumIds(id) later if this proves heavy.
        changes.emit(Change.AlbumList)
        localMutations.emit(Unit)
    }

    suspend fun createAlbum(name: String, parentAlbumId: String?): String {
        val id = lib.createAlbum(name, parentAlbumId)
        changes.emit(Change.AlbumList)
        localMutations.emit(Unit)
        return id
    }

    suspend fun renameAlbum(albumId: String, name: String) {
        lib.renameAlbum(albumId, name)
        changes.emit(Change.AlbumList)
        changes.emit(Change.Album(albumId))
        localMutations.emit(Unit)
    }

    suspend fun deleteAlbum(albumId: String) {
        lib.deleteAlbum(albumId)
        changes.emit(Change.AlbumList)
        changes.emit(Change.MediaList)
        localMutations.emit(Unit)
    }

    suspend fun setAlbumThumbnail(albumId: String, mediaId: String?) {
        lib.setAlbumThumbnail(albumId, mediaId)
        changes.emit(Change.AlbumList)
        changes.emit(Change.Album(albumId))
        localMutations.emit(Unit)
    }

    suspend fun reparentAlbum(albumId: String, newParentAlbumId: String?) {
        lib.reparentAlbum(albumId, newParentAlbumId)
        changes.emit(Change.AlbumList)
        localMutations.emit(Unit)
    }

    suspend fun moveMediaToAlbum(mediaId: String, fromAlbumId: String, toAlbumId: String) {
        lib.moveMediaToAlbum(mediaId, fromAlbumId, toAlbumId)
        changes.emit(Change.Album(fromAlbumId))
        changes.emit(Change.Album(toAlbumId))
        changes.emit(Change.AlbumList)
        changes.emit(Change.MediaList)
        localMutations.emit(Unit)
    }

    suspend fun removeMediaFromAlbum(albumId: String, mediaId: String) {
        lib.removeMediaFromAlbum(albumId, mediaId)
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        changes.emit(Change.MediaList)
        localMutations.emit(Unit)
    }

    suspend fun addMediaToAlbum(albumId: String, mediaId: String) {
        lib.addMediaToAlbum(albumId, mediaId)
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        changes.emit(Change.MediaList)
        localMutations.emit(Unit)
    }

    suspend fun albumsContainingMedia(mediaId: String): List<FfiAlbum> {
        val ids = lib.mediaContainingAlbumIds(mediaId, true).toSet()
        return allAlbums().filter { it.albumId in ids }
    }

    suspend fun containingAlbums(mediaId: String, excludingAlbumId: String?): List<FfiAlbum> =
        albumsContainingMedia(mediaId).filter { it.albumId != excludingAlbumId }

    suspend fun createGroupFromSelectedMedia(mediaIds: List<String>, albumId: String): String {
        val groupId = lib.createGroup(albumId)
        for (mediaId in mediaIds) lib.addMediaToGroup(groupId, mediaId)
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        localMutations.emit(Unit)
        return groupId
    }

    suspend fun deleteGroup(groupId: String, albumId: String) {
        lib.deleteGroup(groupId)
        changes.emit(Change.Album(albumId))
        changes.emit(Change.MediaList)
        localMutations.emit(Unit)
    }

    // Blocking and proportional to file size. Drop the wrap once the Rust side is async.
    suspend fun importMedia(path: String, albumId: String?, originalFilename: String?): String {
        val id = withContext(io) { lib.importMedia(path, albumId, originalFilename, null, null).mediaId }
        if (albumId != null) changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        changes.emit(Change.MediaList)
        localMutations.emit(Unit)
        return id
    }

    suspend fun setMediaThumbnail(mediaId: String, data: ByteArray) {
        lib.setMediaThumbnail(mediaId, data)
        changes.emit(Change.Media(mediaId))
        localMutations.emit(Unit)
    }

    suspend fun mediaThumbnail(mediaId: String): ByteArray? =
        try {
            lib.getMediaThumbnailAsync(mediaId, appDir)
        } catch (e: Exception) {
            null
        }

    suspend fun mediaBytes(mediaId: String): ByteArray? =
        try {
            lib.getMediaBytesAsync(mediaId, appDir)
        } catch (e: Exception) {
            null
        }

    suspend fun groupMedia(groupId: String): List<FfiMediaItem> = lib.groupListMedia(groupId)

    suspend fun listOperationGroups(): List<FfiOperationGroup> = lib.listOperationGroups()

    // Blocking and proportional to file size. Drop the wrap once the Rust side is async.
    suspend fun loadLocalState() {
        withContext(io) { lib.loadLocalState() }
        changes.emit(Change.MediaList)
        changes.emit(Change.AlbumList)
    }

    suspend fun sync(appSupportDir: String?): FfiSyncResult {
        val result = lib.syncAsync(appSupportDir)
        changes.emit(Change.All)
        refreshSessionState()
        return result
    }

    suspend fun connectRemote(remoteId: String, appSupportDir: String?): Boolean =
        try {
            lib.connectRemote(remoteId, appSupportDir)
            true
        } catch (e: Exception) {
            false
        }

    suspend fun initializeRemote(remoteId: String, appSupportDir: String?) {
        lib.initializeRemote(remoteId, appSupportDir)
    }

    suspend fun hasUnpushedChanges(remoteId: String): Boolean = lib.hasUnpushedChanges(remoteId)

    suspend fun localStateStats(): FfiLocalStateStats = lib.localStateStats()

    suspend fun mediaIdsWithoutRemoteBackup(): List<String> = lib.mediaIdsWithoutRemoteBackup()

    // Blocking and proportional to file size. Drop the wrap once the Rust side is async.
    suspend fun evictLocalData() = withContext(io) { lib.evictLocalData(lib.allMediaIds()) }

    // Blocking and proportional to file size. Drop the wrap once the Rust side is async.
    suspend fun evictLocalThumbnails() = withContext(io) { lib.evictLocalThumbnails(lib.allMediaIds()) }

    suspend fun addRemoteS3(
        name: String,
        endpoint: String,
        bucket: String,
        region: String,
        pathPrefix: String,
        accessKey: String,
        secretKey: String,
    ): String {
        val id = lib.addRemoteS3(name, endpoint, bucket, region, pathPrefix, accessKey, secretKey)
        refreshSessionState()
        return id
    }

    suspend fun addRemoteFixedPath(name: String, path: String): String {
        val id = lib.addRemoteFixedPath(name, path)
        refreshSessionState()
        return id
    }

    suspend fun addRemoteDebugLocalAndroid(name: String): String {
        val id = lib.addRemoteDebugLocalAndroid(name)
        refreshSessionState()
        return id
    }

    suspend fun removeRemote(remoteId: String) {
        lib.removeRemote(remoteId)
        refreshSessionState()
    }

    suspend fun setDefaultFetchRemote(remoteId: String?) {
        lib.setDefaultFetchRemote(remoteId)
        refreshSessionState()
    }

    suspend fun setRemoteAutoPush(remoteId: String, enabled: Boolean) {
        lib.setRemoteAutoPush(remoteId, enabled)
        refreshSessionState()
    }

    suspend fun setAutoImportDeviceMedia(enabled: Boolean) {
        lib.setAutoImportDeviceMedia(enabled)
        refreshSessionState()
    }

    suspend fun addUser(username: String, password: String) {
        lib.userAdd(username, password)
        refreshSessionState()
    }

    private fun refreshSessionState() {
        _sessionState.value = buildSessionState()
    }

    private fun buildSessionState() = SessionState(
        libraryId = lib.libraryId(),
        nickname = nickname,
        username = username,
        users = lib.userList(),
        remotes = lib.listRemotes(),
        defaultFetchRemoteId = lib.getDefaultFetchRemote(),
        autoImportDeviceMedia = lib.getAutoImportDeviceMedia(),
    )

    companion object {
        private const val PAGE_SIZE = 100

        fun from(context: Context): LibraryRepository =
            (context.applicationContext as LascoApp).librarySession
                ?: error("No library is open")
    }
}
