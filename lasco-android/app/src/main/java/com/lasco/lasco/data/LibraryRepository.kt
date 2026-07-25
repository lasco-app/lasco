package com.lasco.lasco.data

import android.content.Context
import com.lasco.lasco.LascoApp
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiOperationGroup
import uniffi.lasco_ffi.FfiSyncResult

/**
 * Session scoped repository sitting on top of an opened FfiLibrary. FfiLibrary
 * is pull only and mostly blocking, Rust never pushes a change notification,
 * so the only thing shared across screens is the invalidation signal carried
 * by changes. Data itself is never shared, each screen pulls its own snapshot
 * through watch and holds it locally. This is what replaces the Swift
 * LibraryModel god object on Android.
 *
 * Blocking FfiLibrary calls are wrapped in the injected io dispatcher. The
 * UniFFI async methods (syncAsync, fetchRemoteAsync, pushRemoteAsync,
 * getMediaThumbnailAsync, getMediaBytesAsync) are already suspend on UniFFI's
 * own executor and must not be wrapped again.
 */
class LibraryRepository(
    private val lib: FfiLibrary,
    private val nickname: String,
    private val username: String,
    private val appDir: String,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    private val changes = MutableSharedFlow<Change>(extraBufferCapacity = 64)

    private val _sessionState = MutableStateFlow(buildSessionState())
    val sessionState: StateFlow<SessionState> = _sessionState.asStateFlow()

    /**
     * Subscribes to a load that reruns whenever one of the given scopes (or
     * the All wildcard) is emitted. Re-fires on every re-subscription too, so
     * a screen coming back from the back stack always reloads once and can
     * never stay stale, this is what makes a SharedFlow event bus safe here.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    fun <T> watch(vararg scopes: Change, load: suspend () -> T): Flow<T> =
        changes
            .filter { c -> c is Change.All || scopes.any { it == c } }
            .onStart { emit(Change.All) }
            .mapLatest { withContext(io) { load() } }

    suspend fun mediaByDate(): List<FfiMediaItem> = withContext(io) { lib.mediaByDate() }

    suspend fun listAlbums(): List<FfiAlbum> = withContext(io) { lib.listAlbums() }

    suspend fun mediaInAlbum(albumId: String): List<FfiMediaItem> =
        withContext(io) { lib.mediaInAlbum(albumId) }

    suspend fun showMedia(mediaId: String): FfiMediaItem = withContext(io) { lib.showMedia(mediaId) }

    suspend fun renameMedia(mediaId: String, name: String?) {
        withContext(io) { lib.renameMedia(mediaId, name) }
        changes.emit(Change.Media(mediaId))
        changes.emit(Change.MediaList)
        // Membership is unknown here, start broad and narrow to
        // mediaContainingAlbumIds(id) -> Album(id) later if this proves heavy.
        changes.emit(Change.AlbumList)
    }

    suspend fun albumItemsSorted(albumId: String, ascending: Boolean): List<FfiAlbumItem> =
        withContext(io) { lib.albumListItemsSorted(albumId, ascending) }

    suspend fun createAlbum(name: String, parentAlbumId: String?): String {
        val id = withContext(io) { lib.createAlbum(name, parentAlbumId) }
        changes.emit(Change.AlbumList)
        return id
    }

    suspend fun renameAlbum(albumId: String, name: String) {
        withContext(io) { lib.renameAlbum(albumId, name) }
        changes.emit(Change.AlbumList)
        changes.emit(Change.Album(albumId))
    }

    suspend fun deleteAlbum(albumId: String) {
        withContext(io) { lib.deleteAlbum(albumId) }
        changes.emit(Change.AlbumList)
    }

    suspend fun setAlbumThumbnail(albumId: String, mediaId: String?) {
        withContext(io) { lib.setAlbumThumbnail(albumId, mediaId) }
        changes.emit(Change.AlbumList)
        changes.emit(Change.Album(albumId))
    }

    suspend fun reparentAlbum(albumId: String, newParentAlbumId: String?) {
        withContext(io) { lib.reparentAlbum(albumId, newParentAlbumId) }
        changes.emit(Change.AlbumList)
    }

    suspend fun moveMediaToAlbum(mediaId: String, fromAlbumId: String, toAlbumId: String) {
        withContext(io) { lib.moveMediaToAlbum(mediaId, fromAlbumId, toAlbumId) }
        changes.emit(Change.Album(fromAlbumId))
        changes.emit(Change.Album(toAlbumId))
        changes.emit(Change.AlbumList)
    }

    suspend fun removeMediaFromAlbum(albumId: String, mediaId: String) {
        withContext(io) { lib.removeMediaFromAlbum(albumId, mediaId) }
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
    }

    suspend fun addMediaToAlbum(albumId: String, mediaId: String) {
        withContext(io) { lib.addMediaToAlbum(albumId, mediaId) }
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
    }

    suspend fun albumsContainingMedia(mediaId: String): List<FfiAlbum> = withContext(io) {
        val ids = lib.mediaContainingAlbumIds(mediaId, true).toSet()
        lib.listAlbums().filter { it.albumId in ids }
    }

    suspend fun containingAlbums(mediaId: String, excludingAlbumId: String?): List<FfiAlbum> =
        albumsContainingMedia(mediaId).filter { it.albumId != excludingAlbumId }

    suspend fun createGroupFromSelectedMedia(mediaIds: List<String>, albumId: String): String {
        val groupId = withContext(io) {
            val id = lib.createGroup(albumId)
            for (mediaId in mediaIds) lib.addMediaToGroup(id, mediaId)
            id
        }
        changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        return groupId
    }

    suspend fun deleteGroup(groupId: String, albumId: String) {
        withContext(io) { lib.deleteGroup(groupId) }
        changes.emit(Change.Album(albumId))
    }

    suspend fun importMedia(path: String, albumId: String?, originalFilename: String?): String {
        val id = withContext(io) { lib.importMedia(path, albumId, originalFilename, null, null) }
        if (albumId != null) changes.emit(Change.Album(albumId))
        changes.emit(Change.AlbumList)
        changes.emit(Change.MediaList)
        return id
    }

    suspend fun setMediaThumbnail(mediaId: String, data: ByteArray) {
        withContext(io) { lib.setMediaThumbnail(mediaId, data) }
        changes.emit(Change.Media(mediaId))
    }

    // getMediaThumbnailAsync is already suspend on UniFFI's own executor, so
    // this must not be wrapped in withContext(io) again.
    suspend fun mediaThumbnail(mediaId: String): ByteArray? =
        try {
            lib.getMediaThumbnailAsync(mediaId, appDir)
        } catch (e: Exception) {
            null
        }

    // getMediaBytesAsync is already suspend on UniFFI's own executor, so
    // this must not be wrapped in withContext(io) again.
    suspend fun mediaBytes(mediaId: String): ByteArray? =
        try {
            lib.getMediaBytesAsync(mediaId, appDir)
        } catch (e: Exception) {
            null
        }

    suspend fun groupMedia(groupId: String): List<FfiMediaItem> =
        withContext(io) { lib.groupListMedia(groupId) }

    suspend fun listOperationGroups(): List<FfiOperationGroup> =
        withContext(io) { lib.listOperationGroups() }

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

    suspend fun fetchRemote(remoteId: String, appSupportDir: String?): UInt {
        val pulled = lib.fetchRemoteAsync(remoteId, appSupportDir)
        changes.emit(Change.All)
        return pulled
    }

    suspend fun pushRemote(remoteId: String, appSupportDir: String?): UInt =
        withContext(io) { lib.pushRemoteAsync(remoteId, appSupportDir) }

    suspend fun connectRemote(remoteId: String, appSupportDir: String?): Boolean =
        withContext(io) {
            try {
                lib.connectRemote(remoteId, appSupportDir)
                true
            } catch (e: Exception) {
                false
            }
        }

    suspend fun initializeRemote(remoteId: String, appSupportDir: String?) {
        withContext(io) { lib.initializeRemote(remoteId, appSupportDir) }
    }

    suspend fun hasUnpushedChanges(remoteId: String): Boolean =
        withContext(io) { lib.hasUnpushedChanges(remoteId) }

    suspend fun addRemoteS3(
        name: String,
        endpoint: String,
        bucket: String,
        region: String,
        pathPrefix: String,
        accessKey: String,
        secretKey: String,
    ): String {
        val id = withContext(io) {
            lib.addRemoteS3(name, endpoint, bucket, region, pathPrefix, accessKey, secretKey)
        }
        refreshSessionState()
        return id
    }

    suspend fun addRemoteFixedPath(name: String, path: String): String {
        val id = withContext(io) { lib.addRemoteFixedPath(name, path) }
        refreshSessionState()
        return id
    }

    suspend fun addRemoteDebugLocalAndroid(name: String): String {
        val id = withContext(io) { lib.addRemoteDebugLocalAndroid(name) }
        refreshSessionState()
        return id
    }

    suspend fun removeRemote(remoteId: String) {
        withContext(io) { lib.removeRemote(remoteId) }
        refreshSessionState()
    }

    suspend fun setDefaultFetchRemote(remoteId: String?) {
        withContext(io) { lib.setDefaultFetchRemote(remoteId) }
        refreshSessionState()
    }

    suspend fun setDefaultUploadAlbum(albumId: String?) {
        withContext(io) { lib.setDefaultUploadAlbum(albumId) }
        refreshSessionState()
    }

    suspend fun addUser(username: String, password: String) {
        withContext(io) { lib.userAdd(username, password) }
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
