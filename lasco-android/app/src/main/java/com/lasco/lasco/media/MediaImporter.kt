package com.lasco.lasco.media

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import com.lasco.lasco.data.LibraryRepository
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// Copies picked images/videos (from the Photos picker or the Files/document picker) into
// the library. The Android equivalent of the simple PhotosPicker/fileImporter paths in
// Swift's AlbumsView (temp-file copy then importMedia), not the full PHAsset/iCloud sync
// handled by PhotoLibraryImporter.swift.
object MediaImporter {
    suspend fun importUris(context: Context, repo: LibraryRepository, uris: List<Uri>, albumId: String) {
        for (uri in uris) {
            val originalFilename = queryDisplayName(context, uri)
            val tempFile = withContext(Dispatchers.IO) { copyToCache(context, uri, originalFilename) }
            try {
                val mediaId = repo.importMedia(tempFile.path, albumId, originalFilename)
                ThumbnailGenerator.generate(tempFile)?.let { repo.setMediaThumbnail(mediaId, it) }
            } finally {
                tempFile.delete()
            }
        }
    }

    private fun queryDisplayName(context: Context, uri: Uri): String? {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (idx >= 0 && cursor.moveToFirst()) return cursor.getString(idx)
        }
        return null
    }

    private fun copyToCache(context: Context, uri: Uri, originalFilename: String?): File {
        val name = originalFilename ?: UUID.randomUUID().toString()
        val file = File(context.cacheDir, "import_${UUID.randomUUID()}_$name")
        context.contentResolver.openInputStream(uri)?.use { input ->
            file.outputStream().use { output -> input.copyTo(output) }
        }
        return file
    }
}
