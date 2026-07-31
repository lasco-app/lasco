package com.lasco.lasco.media

import android.content.Context
import android.util.Log
import com.lasco.lasco.data.IncrementalImportState
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.ui.onboarding.InitialImportController
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.lasco_ffi.FfiLibrary

/** Imports only DCIM rows added since the last completed import. */
class IncrementalDeviceMediaImporter(
    private val lib: FfiLibrary,
    private val context: Context,
    private val prefs: Prefs,
    private val onStateChanged: (IncrementalImportState) -> Unit,
    private val onImported: suspend () -> Unit,
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    private val store = DeviceMediaStore(context)
    private val importer = DeviceMediaImporter(lib, context)
    private val requests = Channel<Unit>(Channel.CONFLATED)

    private val worker = scope.launch {
        for (ignored in requests) {
            runImportIfEligible()
        }
    }

    fun requestImport() {
        requests.trySend(Unit)
    }

    suspend fun close() {
        requests.close()
        worker.cancelAndJoin()
    }

    private suspend fun runImportIfEligible() {
        try {
            if (!DeviceMediaPermissions.canReadFullLibrary(context)) {
                onStateChanged(IncrementalImportState.Idle)
                return
            }

            if (!importer.canReadOriginals()) {
                onStateChanged(IncrementalImportState.Idle)
                return
            }

            if (!lib.getAutoImportDeviceMedia()) return

            val libraryId = lib.libraryId()
            val watermark = prefs.importWatermark(libraryId)
            if (watermark == null) {
                prefs.baselineImportWatermark(libraryId)
                onStateChanged(IncrementalImportState.Idle)
                return
            }

            val runStartedAt = System.currentTimeMillis() / 1000
            val scanSince = (watermark - 1).coerceAtLeast(0)
            onStateChanged(IncrementalImportState.Scanning)
            val scan = store.scan(sinceDateAdded = scanSince)

            if (scan.tooLargeCount > 0) {
                onStateChanged(
                    IncrementalImportState.Failed(
                        InitialImportController.tooLargeMessage(scan.tooLargeCount),
                    ),
                )
                return
            }

            withContext(io) {
                importer.clearCache()
                scan.rows.forEachIndexed { index, row ->
                    ensureActive()
                    try {
                        importer.import(row, albumId = null, tempIndex = index)
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        throw IOException(
                            "Could not import ${row.displayName ?: "device media"}",
                            error,
                        )
                    }
                    onStateChanged(
                        IncrementalImportState.Importing(
                            done = index + 1,
                            total = scan.rows.size,
                        ),
                    )
                }
            }

            prefs.setImportWatermark(libraryId, maxOf(watermark, runStartedAt))
            if (scan.rows.isNotEmpty()) onImported()
            onStateChanged(IncrementalImportState.Idle)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            Log.e(TAG, "incremental import failed", error)
            onStateChanged(
                IncrementalImportState.Failed(
                    error.message ?: "Incremental import failed",
                ),
            )
        }
    }

    companion object {
        private const val TAG = "IncrementalImport"
    }
}
