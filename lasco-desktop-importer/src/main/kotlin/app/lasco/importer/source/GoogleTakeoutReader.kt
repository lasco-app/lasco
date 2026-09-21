package app.lasco.importer.source

import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportSource
import app.lasco.importer.model.ResourceRole
import app.lasco.importer.model.SourceMetadata
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import java.nio.file.Files
import java.nio.file.Path
import java.time.Instant
import java.util.zip.ZipFile
import kotlin.io.path.name

/** Google Takeout parser that reads archives lazily and stages only the current import chunk. */
class GoogleTakeoutReader(private val archives: List<Path>) : ImportSourceReader {
    override val source = ImportSource.GOOGLE_TAKEOUT
    private val json = Json { ignoreUnknownKeys = true }

    override suspend fun discover(): List<ImportAsset> = buildList {
        archives.forEach { archive ->
            ZipFile(archive.toFile()).use { zip ->
                zip.entries().asSequence()
                    .filter { entry -> !entry.isDirectory && isMedia(entry.name) }
                    .forEach { entry ->
                        val metadata = metadataFor(zip, entry.name)
                        val album = entry.name.substringBeforeLast('/').takeIf { it.isNotBlank() }
                        add(
                            ImportAsset(
                                sourceId = "takeout:${archive.fileName}:${entry.name}", source = source,
                                resourceRole = ResourceRole.PRIMARY, displayName = entry.name.substringAfterLast('/'),
                                byteCount = entry.size.coerceAtLeast(0), metadata = metadata,
                                sourceLocator = "${archive.toAbsolutePath()}\n${entry.name}", albumNames = listOfNotNull(album),
                            ),
                        )
                    }
            }
        }
    }

    override suspend fun stage(asset: ImportAsset, stagingDirectory: Path): app.lasco.importer.model.StagedAsset {
        Files.createDirectories(stagingDirectory)
        val (archivePath, entryName) = asset.sourceLocator.split('\n', limit = 2).let { it[0] to it[1] }
        val target = stagingDirectory.resolve("${asset.sourceId.hashCode()}-${asset.metadata.originalFilename}").normalize()
        require(target.startsWith(stagingDirectory.normalize())) { "unsafe staging filename" }
        if (!Files.exists(target)) ZipFile(Path.of(archivePath).toFile()).use { zip ->
            val entry = zip.getEntry(entryName) ?: error("Takeout entry disappeared: $entryName")
            zip.getInputStream(entry).use { input -> Files.newOutputStream(target).use(input::copyTo) }
        }
        return app.lasco.importer.model.StagedAsset(asset, target)
    }

    private fun metadataFor(zip: ZipFile, mediaPath: String): SourceMetadata {
        val filename = mediaPath.substringAfterLast('/')
        val candidateNames = listOf("$mediaPath.json", "$mediaPath.supplemental-metadata.json")
        val objectNode = candidateNames.firstNotNullOfOrNull { name -> zip.getEntry(name)?.let { entry ->
            zip.getInputStream(entry).bufferedReader().use { json.parseToJsonElement(it.readText()).jsonObject }
        } }
        val timestamp = objectNode?.get("photoTakenTime")?.jsonObject?.get("timestamp")?.jsonPrimitive?.longOrNull
            ?.let { Instant.ofEpochSecond(it).toString() }
        val geo = objectNode?.get("geoData")?.jsonObject
        return SourceMetadata(
            originalFilename = objectNode?.get("title")?.jsonPrimitive?.content ?: filename,
            capturedAt = timestamp,
            latitude = geo?.get("latitude")?.jsonPrimitive?.content?.toDoubleOrNull(),
            longitude = geo?.get("longitude")?.jsonPrimitive?.content?.toDoubleOrNull(),
        )
    }

    private fun isMedia(path: String): Boolean = path.substringAfterLast('.', "").lowercase() in setOf(
        "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "dng", "raw", "mp4", "mov", "m4v", "avi", "webm",
    )
}
