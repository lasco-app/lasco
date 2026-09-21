package app.lasco.importer.thumbnail

import app.lasco.importer.ffi.NativeLibraryLocations
import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import java.awt.RenderingHints
import java.awt.image.BufferedImage
import java.nio.file.Path
import javax.imageio.ImageIO
import kotlin.math.max
import kotlin.math.roundToInt

private const val THUMBNAIL_MAX_PIXEL_SIZE = 256

/** macOS decodes Photos formats such as HEIC and video through the native staging bridge. */
private interface MacThumbnailNative : Library {
    fun lasco_photos_thumbnail_jpeg(path: String, byteCount: IntArray): Pointer?
    fun lasco_photos_free_buffer(value: Pointer?)
}

/**
 * Creates a JPEG preview before the staged original is removed. On macOS the shared Swift
 * bridge covers Photos formats; the ImageIO fallback keeps ordinary image imports browsable on
 * the other desktop targets as well.
 */
fun generateDesktopThumbnail(path: Path): ByteArray? = macThumbnail(path) ?: imageIoThumbnail(path)

private fun macThumbnail(path: Path): ByteArray? {
    if (!System.getProperty("os.name").lowercase().contains("mac")) return null
    return try {
        val library = NativeLibraryLocations.absolutePath("libLascoPhotoImportKit.dylib")
            ?: "LascoPhotoImportKit"
        val bridge = Native.load(library, MacThumbnailNative::class.java)
        val length = intArrayOf(0)
        val pointer = bridge.lasco_photos_thumbnail_jpeg(path.toString(), length)
        if (pointer == null || length[0] <= 0) null else {
            try {
                pointer.getByteArray(0, length[0])
            } finally {
                bridge.lasco_photos_free_buffer(pointer)
            }
        }
    } catch (_: Exception) {
        null
    }
}

private fun imageIoThumbnail(path: Path): ByteArray? = try {
    val source = ImageIO.read(path.toFile()) ?: return null
    val scale = minOf(
        1.0,
        THUMBNAIL_MAX_PIXEL_SIZE.toDouble() / max(source.width, source.height).toDouble(),
    )
    val width = max(1, (source.width * scale).roundToInt())
    val height = max(1, (source.height * scale).roundToInt())
    val thumbnail = BufferedImage(width, height, BufferedImage.TYPE_INT_RGB)
    val graphics = thumbnail.createGraphics()
    try {
        graphics.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_BICUBIC)
        graphics.drawImage(source, 0, 0, width, height, null)
    } finally {
        graphics.dispose()
    }
    val output = java.io.ByteArrayOutputStream()
    if (ImageIO.write(thumbnail, "jpeg", output)) output.toByteArray() else null
} catch (_: Exception) {
    null
}
