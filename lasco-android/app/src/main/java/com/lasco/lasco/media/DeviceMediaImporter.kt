package com.lasco.lasco.media

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.webkit.MimeTypeMap
import androidx.core.content.ContextCompat
import java.io.File
import java.io.IOException
import uniffi.lasco_ffi.FfiLibrary
import uniffi.lasco_ffi.FfiAlbumUuid
import uniffi.lasco_ffi.FfiMediaUuid

/**
 * Copies one device media row out of MediaStore and hands it to the core.
 * Blocking and dispatcher agnostic, the caller decides where it runs.
 *
 * Nothing here knows about progress state, pushing, eviction or the import
 * watermark, so the onboarding import and the incremental one can share it
 * and keep their own policies. The counterpart of the Swift
 * PhotoLibraryImporter, without the watermark that one carries.
 */
class DeviceMediaImporter(
    private val lib: FfiLibrary,
    private val context: Context,
) {
    // A row whose content hash is already in the library keeps its existing
    // thumbnail, regenerating it would re-encrypt and rewrite the same bytes.
    fun import(row: DeviceMediaRow, albumId: FfiAlbumUuid?, tempIndex: Int): FfiMediaUuid {
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

    // Every row would fail the original read without this, so callers check it
    // once up front instead of starting a run that imports nothing.
    fun canReadOriginals(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.Q ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_MEDIA_LOCATION) ==
            PackageManager.PERMISSION_GRANTED

    // A run killed partway leaves its temp files behind, nothing else ever
    // removes them.
    fun clearCache() {
        cacheDir().listFiles()?.forEach { it.delete() }
    }

    // Reading through setRequireOriginal is what keeps the GPS EXIF tags in
    // the bytes. Only originals are imported, so a row that cannot be read
    // that way fails instead of being quietly downgraded to the redacted
    // copy, whose location cannot be recovered later.
    private fun copyToCache(row: DeviceMediaRow, tempIndex: Int): File {
        val uri = DeviceMediaStore.contentUriFor(row)

        // Nothing reads this name. The bytes go to the core by path, the
        // thumbnail is decoded from the content, and the file is deleted once
        // the row is done, so the counter alone is enough.
        val file = File(cacheDir(), tempIndex.toString())

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
            // Thrown before import gets the file, so its finally block cannot
            // be the one to clean up the partial copy.
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

    private fun cacheDir(): File =
        File(context.cacheDir, TEMP_DIR_NAME).apply { mkdirs() }

    private fun copyUri(uri: Uri, file: File) {
        val input = context.contentResolver.openInputStream(uri)
            ?: throw IOException("no stream for $uri")
        input.use { source ->
            file.outputStream().use { output -> source.copyTo(output, bufferSize = 8 * 1024) }
        }
    }

    companion object {
        private const val TEMP_DIR_NAME = "device_import"
    }
}
