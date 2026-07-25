package com.lasco.lasco.media

import android.content.Context
import com.lasco.lasco.data.LibraryRepository
import java.io.File

/**
 * Materializes a media item's full-quality bytes to a real file in the
 * cache dir, keyed by mediaId + extension. Needed because ExoPlayer plays
 * from a file/content Uri, not raw bytes, and because Export/share also
 * needs a real file to hand to a FileProvider Uri. The Android equivalent
 * of Swift's LibraryModel.videoURL(for:extension:), which writes to
 * FileManager.default.temporaryDirectory and keeps an in-memory URL cache.
 * Here the filesystem itself is the cache: if the file already exists we
 * skip the write, matching the established cacheDir usage in
 * MediaImporter.kt elsewhere in this codebase.
 */
object VideoFileCache {
    suspend fun file(context: Context, repo: LibraryRepository, mediaId: String, filenameOriginal: String): File? {
        val ext = filenameOriginal.substringAfterLast('.', "")
        val name = if (ext.isEmpty()) mediaId else "$mediaId.$ext"
        val file = File(context.cacheDir, "media_$name")
        if (file.exists()) return file
        val bytes = repo.mediaBytes(mediaId) ?: return null
        file.writeBytes(bytes)
        return file
    }
}
