package com.lasco.lasco.data

import android.content.Context
import com.lasco.lasco.LascoApp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.withContext
import uniffi.lasco_ffi.FfiCreateLibraryResult
import uniffi.lasco_ffi.FfiLibrary
import uniffi.lasco_ffi.FfiLibraryEntry
import uniffi.lasco_ffi.ffiAddExistingLibraryS3
import uniffi.lasco_ffi.ffiCreateLibrary
import uniffi.lasco_ffi.ffiDeleteLibrary
import uniffi.lasco_ffi.ffiOpenCached
import uniffi.lasco_ffi.listLibraries
import uniffi.lasco_ffi.sessionClear

/**
 * Single boundary between the app and the Rust FFI.
 *
 * Everything that touches the generated bindings goes through here so there is
 * one place that owns the injected app directory and one place that moves the
 * blocking FFI calls off the main thread. Most of the FFI surface is
 * synchronous and hits the local database, so those calls run on the IO
 * dispatcher. The few functions that have native async variants are exposed as
 * suspend functions of their own as we need them.
 *
 * The equivalent of the Swift LibraryModel is built on top of this, split into
 * per screen ViewModels rather than a single object.
 */
class LascoRepository(
    // The app private directory passed to every FFI entry point that resolves
    // the main app data dir. On Android this is context.filesDir.path.
    val appDir: String,
) {
    private val changes = MutableSharedFlow<Unit>(extraBufferCapacity = 64)

    suspend fun listLibraries(): List<FfiLibraryEntry> = withContext(Dispatchers.IO) {
        listLibraries(appDir = appDir)
    }

    /**
     * Subscribes to the library list. Reloads once on subscription and again
     * every time a mutation below signals a change, so a screen coming back
     * after a delete or add never shows a stale list.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    fun watchLibraries(): Flow<List<FfiLibraryEntry>> =
        changes
            .onStart { emit(Unit) }
            .mapLatest { listLibraries() }

    suspend fun createLibrary(nickname: String, username: String, password: String): FfiCreateLibraryResult {
        val result = withContext(Dispatchers.IO) {
            ffiCreateLibrary(nickname = nickname, username = username, password = password, appDir = appDir)
        }
        changes.emit(Unit)
        return result
    }

    suspend fun openLibrary(nickname: String, username: String, password: String): FfiLibrary =
        withContext(Dispatchers.IO) {
            FfiLibrary.open(nickname = nickname, username = username, password = password, appDir = appDir)
        }

    suspend fun openCached(nickname: String, username: String): FfiLibrary? =
        withContext(Dispatchers.IO) {
            ffiOpenCached(nickname = nickname, username = username, appDir = appDir)
        }

    suspend fun addExistingLibraryS3(
        nickname: String,
        username: String,
        password: String,
        newUsername: String? = null,
        newPassword: String? = null,
        remoteId: String,
        endpoint: String,
        bucket: String,
        region: String,
        pathPrefix: String,
        accessKey: String,
        secretKey: String,
    ): FfiLibrary {
        val library = withContext(Dispatchers.IO) {
            ffiAddExistingLibraryS3(
                nickname = nickname,
                username = username,
                password = password,
                newUsername = newUsername,
                newPassword = newPassword,
                remoteId = remoteId,
                endpoint = endpoint,
                bucket = bucket,
                region = region,
                pathPrefix = pathPrefix,
                accessKey = accessKey,
                secretKey = secretKey,
                appDir = appDir,
            )
        }
        changes.emit(Unit)
        return library
    }

    suspend fun signOut(libraryId: String, username: String) = withContext(Dispatchers.IO) {
        sessionClear(libraryId = libraryId, username = username, appDir = appDir)
    }

    suspend fun deleteLibrary(libraryId: String) {
        withContext(Dispatchers.IO) {
            ffiDeleteLibrary(libraryId = libraryId, appDir = appDir)
        }
        changes.emit(Unit)
    }

    companion object {
        fun from(context: Context): LascoRepository =
            (context.applicationContext as LascoApp).repository
    }
}
