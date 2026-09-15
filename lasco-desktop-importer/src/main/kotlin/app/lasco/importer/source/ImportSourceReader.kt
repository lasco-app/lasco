package app.lasco.importer.source

import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportSource
import app.lasco.importer.model.StagedAsset
import java.nio.file.Path

interface ImportSourceReader {
    val source: ImportSource
    suspend fun discover(): List<ImportAsset>
    /** Resolves a source locator belonging to the current in-memory scan. */
    suspend fun stage(asset: ImportAsset, stagingDirectory: Path): StagedAsset
}
