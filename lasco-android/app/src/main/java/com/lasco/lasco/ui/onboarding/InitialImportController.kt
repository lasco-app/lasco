package com.lasco.lasco.ui.onboarding

import android.content.Context
import android.util.Log
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.data.SyncController
import com.lasco.lasco.media.DeviceMediaImporter
import com.lasco.lasco.media.DeviceMediaStore
import com.lasco.lasco.media.DeviceScan
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
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
 * The one shot camera folder import the onboarding wizard runs, the
 * counterpart of Swift's IosImportModel.bulkImportFromPhotoLibrary. The row
 * by row work belongs to DeviceMediaImporter, what lives here is the policy
 * the wizard needs, refuse rather than import a partial library, hold the
 * scheduled push back, push and evict per chunk, report progress.
 *
 * The incremental import is a separate controller on LibraryRepository. The
 * two share the importer and the watermark in Prefs, nothing else.
 *
 * Talks to FfiLibrary directly rather than through LibraryRepository, since
 * importMedia there fires a changes.emit plus localMutations.emit per item,
 * tens of thousands of reload signals across a large import.
 *
 * Built and owned by NewLibraryWizardViewModel and run on its viewModelScope.
 * That is only safe because the wizard screen blocks back navigation for the
 * entire time an import is in progress, so the ViewModel cannot be cleared
 * mid run.
 *
 * Only the camera folder (DCIM and its subfolders) is scanned, and everything lands in the
 * default upload album, no album replication like the iOS wizard does.
 */
class InitialImportController(
    private val lib: FfiLibrary,
    private val context: Context,
    private val prefs: Prefs,
    private val sync: SyncController,
    private val onLibraryChanged: suspend () -> Unit,
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    private val deviceMediaStore = DeviceMediaStore(context)
    private val importer = DeviceMediaImporter(lib, context)

    private val _importState = MutableStateFlow<ImportState>(ImportState.Idle)
    val importState: StateFlow<ImportState> = _importState.asStateFlow()

    private var lastScan: DeviceScan? = null
    private var importJob: Job? = null

    // Guards against two overlapping runs. tryLock rather than withLock,
    // since a second call while one is already in flight must be dropped
    // immediately, not queued. Released once a run ends so a retry after
    // ImportState.Error is still allowed.
    private val startLock = Mutex()

    // Scans the camera folder without importing anything, so the wizard can
    // show counts and estimated size before the user commits.
    suspend fun scan(): DeviceScan {
        _importState.value = ImportState.Scanning
        val result = deviceMediaStore.scan()
        lastScan = result
        _importState.value = ImportState.Idle
        return result
    }

    // Imports everything found by the last scan().
    suspend fun runInitialImport() {
        val scan = lastScan ?: return
        if (!startLock.tryLock()) return
        try {
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
            if (!importer.canReadOriginals()) {
                _importState.value = ImportState.Error(NO_LOCATION_PERMISSION_MESSAGE)
                return
            }

            val job = scope.launch { importScan(scan, remoteId) }
            importJob = job
            job.join()
        } finally {
            startLock.unlock()
        }
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

        withContext(io) { importer.clearCache() }

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
                                chunkIds += importer.import(row, albumId, rowIndex)
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

    companion object {
        private const val TAG = "InitialImport"
        private const val CHUNK_SIZE = 32

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
