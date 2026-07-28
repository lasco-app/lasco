package com.lasco.lasco.data

import android.content.Context
import com.lasco.lasco.LascoApp
import com.lasco.lasco.media.DeviceImportController
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
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.withContext
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem
import uniffi.lasco_ffi.FfiLibrary
import uniffi.lasco_ffi.FfiLocalStateStats
import uniffi.lasco_ffi.FfiMediaItem
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
    prefs: Prefs,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private val changes = MutableSharedFlow<Change>(extraBufferCapacity = 64)

    // Separate from changes, which also fires on remote refreshes (Change.All)
    // and would wrongly trigger auto push if reused here.
    private val localMutations = MutableSharedFlow<Unit>(extraBufferCapacity = 64)

    private val _sessionState = MutableStateFlow(buildSessionState())
    val sessionState: StateFlow<SessionState> = _sessionState.asStateFlow()

    val sync = SyncController(lib = lib, prefs = prefs, onLibraryChanged = { changes.emit(Change.All) }, scope = scope)

    val deviceImport = DeviceImportController(
        lib = lib,
        context = context.applicationContext,
        prefs = prefs,
        sync = sync,
        onLibraryChanged = { changes.emit(Change.All) },
        scope = scope,
    )

    init {
        scope.launch { localMutations.collect { sync.schedulePush() } }
    }

    // Must be called on session end (sign out, delete), or the sync loop and
    // its localMutations collector outlive the repository.
    suspend fun close() {
        deviceImport.cancel()
        sync.close()
        scope.cancel()
    }

    // Reruns load on any of the given scopes (or Change.All), and once per
    // subscription, so a screen returning from the back stack always reloads
    // and never stays stale.
    @OptIn(ExperimentalCoroutinesApi::class)
    fun <T> watch(vararg scopes: Change, load: suspend () -> T): Flow<T> =
        changes
            .filter { c -> c is Change.All || scopes.any { it == c } }
            .onStart { emit(Change.All) }
            .mapLatest { load() }

    // Blocking and proportional to file size. Drop the wrap once the Rust side is async.
    suspend fun mediaByDate(): List<FfiMediaItem> = withContext(io) { lib.mediaByDate() }

    suspend fun listAlbums(): List<FfiAlbum> = lib.listAlbums()

    suspend fun mediaInAlbum(albumId: String): List<FfiMediaItem> = lib.mediaInAlbum(albumId)

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

    suspend fun albumItemsSorted(albumId: String, ascending: Boolean): List<FfiAlbumItem> =
        lib.albumListItemsSorted(albumId, ascending)

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
        localMutations.emit(Unit)
    }

    suspend fun removeMediaFromAlbum(albumId: String, mediaId: String) {
        lib.removeMediaFromAlbum(albumId, mediaId)
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        localMutations.emit(Unit)
    }

    suspend fun addMediaToAlbum(albumId: String, mediaId: String) {
        lib.addMediaToAlbum(albumId, mediaId)
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        localMutations.emit(Unit)
    }

    suspend fun albumsContainingMedia(mediaId: String): List<FfiAlbum> {
        val ids = lib.mediaContainingAlbumIds(mediaId, true).toSet()
        return lib.listAlbums().filter { it.albumId in ids }
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
        localMutations.emit(Unit)
    }

    // Blocking and proportional to file size. Drop the wrap once the Rust side is async.
    suspend fun importMedia(path: String, albumId: String?, originalFilename: String?): String {
        val id = withContext(io) { lib.importMedia(path, albumId, originalFilename, null, null) }
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

    suspend fun setDefaultUploadAlbum(albumId: String?) {
        lib.setDefaultUploadAlbum(albumId)
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
        defaultUploadAlbumId = lib.getDefaultUploadAlbum(),
        defaultFetchRemoteId = lib.getDefaultFetchRemote(),
        autoImportDeviceMedia = lib.getAutoImportDeviceMedia(),
    )

    companion object {
        fun from(context: Context): LibraryRepository =
            (context.applicationContext as LascoApp).librarySession
                ?: error("No library is open")
    }
}
