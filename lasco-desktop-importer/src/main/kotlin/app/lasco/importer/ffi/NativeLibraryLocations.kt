package app.lasco.importer.ffi

import com.sun.jna.NativeLibrary
import java.nio.file.Files
import java.nio.file.Path

/**
 * Resolves native libraries from Compose's packaged resources directory.
 *
 * Mac App Store builds are sandboxed and their signed native libraries must be loaded directly
 * from the app bundle. JNA's resource-extraction fallback is valid for local development but not
 * for a signed TestFlight build.
 */
internal object NativeLibraryLocations {
    private val packagedDirectory: Path? by lazy {
        if (!System.getProperty("os.name").lowercase().contains("mac")) return@lazy null
        val resources = System.getProperty("compose.application.resources.dir") ?: return@lazy null
        val architecture = when (System.getProperty("os.arch").lowercase()) {
            "aarch64", "arm64" -> "aarch64"
            "x86_64", "amd64" -> "x86-64"
            else -> return@lazy null
        }
        Path.of(resources, "darwin-$architecture").takeIf { Files.isDirectory(it) }
    }

    fun configureJnaSearchPaths() {
        val directory = packagedDirectory ?: return
        NativeLibrary.addSearchPath("lasco_ffi", directory.toString())
        NativeLibrary.addSearchPath("LascoPhotoImportKit", directory.toString())
    }

    fun absolutePath(fileName: String): String? = packagedDirectory
        ?.resolve(fileName)
        ?.takeIf { Files.isRegularFile(it) }
        ?.toString()
}
