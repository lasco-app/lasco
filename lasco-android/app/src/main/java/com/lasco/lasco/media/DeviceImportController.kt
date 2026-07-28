package com.lasco.lasco.media

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.util.Log
import android.webkit.MimeTypeMap
import androidx.core.content.ContextCompat
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.data.SyncController
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.lasco_ffi.FfiLibrary

sealed interface ImportState {
    data object Idle : ImportState
    data object Scanning : ImportState
    data class Importing(val done: Int, val total: Int) : ImportState
    data class Done(val photos: Int, val videos: Int, val failed: Int) : ImportState
    data class Error(val message: String) : ImportState
}

/**
 * Session scoped device media importer, the initial-import counterpart of
 * iOS's PhotoLibraryImporter/IosImportModel pair. Talks to FfiLibrary
 * directly rather than through LibraryRepository, since importMedia there
 * fires a changes.emit plus localMutations.emit per item, tens of thousands
 * of reload signals across a large import. Owned by LibraryRepository,
 * constructed right after SyncController so it can borrow its push controls.
 *
 * Only the camera folder (DCIM and its subfolders) is scanned, and everything lands in the
 * default upload album, no album replication like the iOS wizard does.
 */
class DeviceImportController(
    private val lib: FfiLibrary,
    private val context: Context,
    private val prefs: Prefs,
    private val sync: SyncController,
    private val onLibraryChanged: suspend () -> Unit,
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    private val deviceMediaStore = DeviceMediaStore(context)

    private val _importState = MutableStateFlow<ImportState>(ImportState.Idle)
    val importState: StateFlow<ImportState> = _importState.asStateFlow()

    private var lastScan: DeviceScan? = null
    private var importJob: Job? = null

    // Scans the camera folder without importing anything, so the wizard can
    // show counts and estimated size before the user commits.
    suspend fun scan(): DeviceScan {
        _importState.value = ImportState.Scanning
        val result = deviceMediaStore.scan()
        lastScan = result
        _importState.value = ImportState.Idle
        return result
    }

    // Imports everything found by the last scan(). Runs on the injected
    // scope rather than the caller's, so close() can cancel it independently
    // of whatever screen started it.
    suspend fun runInitialImport() {
        val scan = lastScan ?: return
        if (importJob?.isActive == true) return

        // Importing everything else and leaving these behind would hand the
        // user a library they believe is complete. Nothing is imported instead,
        // and no watermark is stamped, so making room and coming back still
        // picks up the whole camera folder.
        if (scan.tooLargeCount > 0) {
            _importState.value = ImportState.Error(tooLargeMessage(scan.tooLargeCount))
            return
        }

        // Checked before anything is imported. Without a remote the chunk
        // eviction below would delete every blob it just wrote, leaving a
        // library of unreadable entries.
        val remoteId = lib.getDefaultFetchRemote()
        if (remoteId == null) {
            _importState.value = ImportState.Error(NO_REMOTE_MESSAGE)
            prefs.baselineImportWatermark(lib.libraryId())
            return
        }

        // Every row would fail the original read without this, so it is worth
        // one error up front instead of a run that imports nothing. No
        // watermark is stamped, granting the permission and coming back has
        // to still pick up everything.
        if (!canReadOriginals()) {
            _importState.value = ImportState.Error(NO_LOCATION_PERMISSION_MESSAGE)
            return
        }

        val job = scope.launch { importScan(scan, remoteId) }
        importJob = job
        job.join()
    }

    // Must be called before the owning scope is cancelled (see
    // LibraryRepository.close), since cancelling the scope does not wait and
    // would leave an importMedia call running against a library the caller is
    // about to tear down. Returns once the row in flight has finished, which
    // for a large video can take a while, importRow cannot be interrupted.
    suspend fun cancel() {
        importJob?.cancelAndJoin()
    }

    private suspend fun importScan(scan: DeviceScan, remoteId: String) {
        val albumId = lib.getDefaultUploadAlbum()
        val total = scan.rows.size

        // Read before the first row so the incremental import later picks up
        // anything the device gained while this run was going. Seconds, to
        // match the MediaStore DATE_ADDED it is compared against.
        val startedAt = System.currentTimeMillis() / 1000

        sync.stopScheduledPush()
        _importState.value = ImportState.Importing(0, total)

        // A run killed partway leaves its temp files behind, nothing else ever
        // removes them.
        withContext(io) { clearImportCacheDir() }

        var photosImported = 0
        var videosImported = 0
        var failed = 0
        var done = 0

        // Names the temp file of each row. A counter is used rather than
        // anything coming from MediaStore, so uniqueness is ours to guarantee.
        var tempIndex = 0

        try {
            for (chunk in scan.rows.chunked(CHUNK_SIZE)) {
                val chunkIds = mutableListOf<String>()

                withContext(io) {
                    for (row in chunk) {
                        // Nothing else in this loop suspends, so without this
                        // check cancellation is only noticed when the chunk
                        // ends and cancel() waits out 32 more files.
                        ensureActive()
                        val rowIndex = tempIndex++

                        // scan() already left the oversized rows out, so this
                        // only catches a MediaStore size that was stale then.
                        if (row.size <= DeviceMediaStore.MAX_IMPORT_FILE_BYTES) {
                            // One unreadable row must not abort the whole run,
                            // the same per item tolerance as the iOS importer.
                            try {
                                chunkIds += importRow(row, albumId, rowIndex)
                                if (row.isVideo) videosImported++ else photosImported++
                            } catch (e: CancellationException) {
                                throw e
                            } catch (e: SecurityException) {
                                // The location permission went away mid run,
                                // so every remaining row would import without
                                // its location. Abort rather than tolerate.
                                throw e
                            } catch (e: Throwable) {
                                failed++
                                Log.w(TAG, "import failed for ${row.displayName}", e)
                            }
                        } else {
                            failed++
                        }
                        done++
                        _importState.value = ImportState.Importing(done, total)
                    }
                }

                // pushRemote is UniFFI async on its own executor, kept outside io.
                pushChunk(remoteId)
                withContext(io) { lib.evictLocalData(chunkIds) }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            Log.e(TAG, "import aborted", e)
            _importState.value = ImportState.Error(e.message ?: "Import failed")
            // The run stopped partway, so the rows it never reached are left
            // behind rather than picked up later by the incremental import.
            // Stamping the watermark is what draws that line.
            prefs.baselineImportWatermark(lib.libraryId())
            if (photosImported + videosImported > 0) onLibraryChanged()
            return
        }

        prefs.setImportWatermark(lib.libraryId(), startedAt)
        _importState.value = ImportState.Done(photosImported, videosImported, failed)
        if (photosImported + videosImported > 0) onLibraryChanged()
    }

    // Uploads what the chunk just imported, or throws once the retries are
    // spent. Nothing is evicted until this returns, so a run that kept going
    // through failed pushes would write the whole camera roll to internal
    // storage and never take any of it back. Failing the run is what stops
    // that, the alternative is a silently filled disk.
    private suspend fun pushChunk(remoteId: String) {
        var lastError: String? = null
        repeat(PUSH_ATTEMPTS) { attempt ->
            if (attempt > 0) delay(PUSH_RETRY_DELAY_MS * attempt)
            val error = sync.pushRemote(remoteId) ?: return
            lastError = error
            Log.w(TAG, "push attempt ${attempt + 1} of $PUSH_ATTEMPTS failed: $error")
        }
        throw IOException("$PUSH_FAILED_MESSAGE ($lastError)")
    }

    // A row whose content hash is already in the library keeps its existing
    // thumbnail, regenerating it would re-encrypt and rewrite the same bytes.
    private fun importRow(row: DeviceMediaRow, albumId: String?, tempIndex: Int): String {
        val tempFile = copyToCache(row, tempIndex)
        try {
            // Always named explicitly. Passing null makes media_add fall back to
            // the temp file name, which would put a bare number in the library
            // and leave the extension checks in the UI with nothing to read.
            val name = row.displayName ?: fallbackFilename(row, tempIndex)
            val result = lib.importMedia(tempFile.path, albumId, name, null, null)
            if (!result.alreadyExisted) {
                ThumbnailGenerator.generate(tempFile)?.let { lib.setMediaThumbnail(result.mediaId, it) }
            }
            return result.mediaId
        } finally {
            tempFile.delete()
        }
    }

    // Reading through setRequireOriginal is what keeps the GPS EXIF tags in
    // the bytes. Only originals are imported, so a row that cannot be read
    // that way fails instead of being quietly downgraded to the redacted
    // copy, whose location cannot be recovered later.
    private fun copyToCache(row: DeviceMediaRow, tempIndex: Int): File {
        val uri = deviceMediaStore.contentUriFor(row)

        // Nothing reads this name. The bytes go to the core by path, the
        // thumbnail is decoded from the content, and the file is deleted once
        // the row is done, so the counter alone is enough.
        val file = File(importCacheDir(), tempIndex.toString())

        // Below API 29 nothing is redacted and setRequireOriginal does not
        // exist, so the plain uri already yields the original.
        val source = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            MediaStore.setRequireOriginal(uri)
        } else {
            uri
        }

        try {
            copyUri(source, file)
        } catch (e: Throwable) {
            // Thrown before importRow gets the file, so its finally block
            // cannot be the one to clean up the partial copy.
            file.delete()
            throw e
        }
        return file
    }

    // Only reached when MediaStore has no display name for the row, which is
    // rare. The extension is the part that matters, the UI tells photos and
    // videos apart by reading it back off the stored name.
    private fun fallbackFilename(row: DeviceMediaRow, tempIndex: Int): String {
        val ext = row.mimeType?.let { MimeTypeMap.getSingleton().getExtensionFromMimeType(it) }
            ?: if (row.isVideo) "mp4" else "jpg"
        return "media_$tempIndex.$ext"
    }

    private fun importCacheDir(): File =
        File(context.cacheDir, TEMP_DIR_NAME).apply { mkdirs() }

    private fun clearImportCacheDir() {
        importCacheDir().listFiles()?.forEach { it.delete() }
    }

    private fun canReadOriginals(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.Q ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_MEDIA_LOCATION) ==
            PackageManager.PERMISSION_GRANTED

    private fun copyUri(uri: Uri, file: File) {
        val input = context.contentResolver.openInputStream(uri)
            ?: throw IOException("no stream for $uri")
        input.use { source ->
            file.outputStream().use { output -> source.copyTo(output, bufferSize = 8 * 1024) }
        }
    }

    companion object {
        private const val TAG = "DeviceImport"
        private const val CHUNK_SIZE = 32

        private const val TEMP_DIR_NAME = "device_import"

        const val NO_REMOTE_MESSAGE =
            "Add a remote before importing. Lasco uploads each batch as it goes and keeps only what the remote has."

        const val NO_LOCATION_PERMISSION_MESSAGE =
            "Allow access to photo locations before importing. Lasco imports originals only, and Android strips the location out of every copy it hands over without that permission."

        // Tries per chunk, the run stops when the last one fails.
        private const val PUSH_ATTEMPTS = 3

        // Multiplied by the attempt number, so the waits are 2s then 4s. Long
        // enough for a network handover, short enough that a dead endpoint
        // does not hold the import open for a minute.
        private const val PUSH_RETRY_DELAY_MS = 2_000L

        const val PUSH_FAILED_MESSAGE =
            "Upload failed, so the import stopped. Everything imported so far is on this device until the remote is reachable again."

        fun tooLargeMessage(count: Int): String {
            val fileWord = if (count == 1) "file is" else "files are"
            return "$count $fileWord larger than ${DeviceMediaStore.MAX_IMPORT_FILE_LABEL}, which Lasco cannot import yet. " +
                "Nothing is imported until they are moved out of your camera folder, rather than leaving you " +
                "with a library that is quietly missing them."
        }
    }
}
