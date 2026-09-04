package com.lasco.lasco.media

import android.content.Context
import com.lasco.lasco.data.LibraryRepository
import java.io.File
import uniffi.lasco_ffi.FfiMediaUuid

/**
 * Materializes a media item's full-quality plaintext into an app-private
 * cache file, keyed by mediaId + extension. The Rust FFI performs the write
 * so a large video never crosses into Kotlin as a ByteArray. ExoPlayer and
 * Export/share then consume the resulting file through a Uri.
 */
object VideoFileCache {
    suspend fun file(context: Context, repo: LibraryRepository, mediaId: FfiMediaUuid, filenameOriginal: String): File? {
        val ext = filenameOriginal.substringAfterLast('.', "")
        val name = if (ext.isEmpty()) mediaId.value else "${mediaId.value}.$ext"
        val file = File(File(context.cacheDir, "lasco-media"), "media_$name")
        if (file.exists()) return file
        return repo.materializeMedia(mediaId, file.path)?.let(::File)
    }
}
