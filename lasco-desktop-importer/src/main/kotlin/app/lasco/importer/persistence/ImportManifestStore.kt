package app.lasco.importer.persistence

import app.lasco.importer.model.ImportAsset
import app.lasco.importer.model.ImportRunState
import app.lasco.importer.model.ImportSource
import app.lasco.importer.model.SourceMetadata
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.nio.file.Files
import java.nio.file.Path
import java.sql.Connection
import java.sql.DriverManager
import java.time.Instant

/**
 * Durable import checkpoint. SQLite transactions make discovery idempotent and permit a resumed
 * job to reuse source IDs and staging paths rather than discovering a Photos library again.
 */
class ImportManifestStore(private val appData: Path) : AutoCloseable {
    private val json = Json { ignoreUnknownKeys = true }
    private val connection: Connection

    init {
        Files.createDirectories(appData)
        connection = DriverManager.getConnection("jdbc:sqlite:${appData.resolve("importer.sqlite")}")
        connection.createStatement().use { statement ->
            statement.executeUpdate("PRAGMA journal_mode=WAL")
            statement.executeUpdate(
                """CREATE TABLE IF NOT EXISTS jobs (
                    id TEXT PRIMARY KEY, source TEXT NOT NULL, state TEXT NOT NULL,
                    chunk_size INTEGER NOT NULL, pause_requested INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL, updated_at TEXT NOT NULL, failure TEXT
                )""",
            )
            statement.executeUpdate(
                """CREATE TABLE IF NOT EXISTS albums (
                    job_id TEXT NOT NULL, source_album_name TEXT NOT NULL, lasco_album_id TEXT NOT NULL,
                    PRIMARY KEY(job_id, source_album_name)
                )""",
            )
            statement.executeUpdate(
                """CREATE TABLE IF NOT EXISTS assets (
                    job_id TEXT NOT NULL, source_id TEXT NOT NULL, source TEXT NOT NULL,
                    role TEXT NOT NULL, display_name TEXT NOT NULL, byte_count INTEGER NOT NULL,
                    metadata_json TEXT NOT NULL, source_locator TEXT NOT NULL, albums_json TEXT NOT NULL,
                    aae_source_id TEXT, live_video_source_id TEXT, staged_path TEXT,
                    media_id TEXT, already_existed INTEGER, chunk_number INTEGER,
                    PRIMARY KEY(job_id, source_id)
                )""",
            )
            statement.executeUpdate(
                """CREATE TABLE IF NOT EXISTS remote_chunks (
                    job_id TEXT NOT NULL, chunk_number INTEGER NOT NULL, remote_id TEXT NOT NULL,
                    completed_at TEXT NOT NULL, PRIMARY KEY(job_id, chunk_number, remote_id)
                )""",
            )
        }
    }

    fun createJob(id: String, source: ImportSource, chunkSize: Int) = transaction {
        prepareStatement("INSERT OR IGNORE INTO jobs VALUES (?, ?, ?, ?, 0, ?, ?, NULL)").use {
            it.setString(1, id); it.setString(2, source.name); it.setString(3, ImportRunState.SCANNING.name)
            it.setInt(4, chunkSize); it.setString(5, Instant.now().toString()); it.setString(6, Instant.now().toString())
            it.executeUpdate()
        }
    }

    fun recordDiscovery(jobId: String, assets: List<ImportAsset>) = transaction {
        prepareStatement(
            """INSERT OR IGNORE INTO assets
            (job_id, source_id, source, role, display_name, byte_count, metadata_json, source_locator,
             albums_json, aae_source_id, live_video_source_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
        ).use { statement ->
            assets.forEach { asset ->
                statement.setString(1, jobId); statement.setString(2, asset.sourceId); statement.setString(3, asset.source.name)
                statement.setString(4, asset.resourceRole.name); statement.setString(5, asset.displayName); statement.setLong(6, asset.byteCount)
                statement.setString(7, json.encodeToString(asset.metadata)); statement.setString(8, asset.sourceLocator)
                statement.setString(9, json.encodeToString(asset.albumNames)); statement.setString(10, asset.aaeSourceId); statement.setString(11, asset.liveVideoSourceId)
                statement.addBatch()
            }
            statement.executeBatch()
        }
    }

    fun markReady(jobId: String) = setState(jobId, ImportRunState.READY)
    fun markImporting(jobId: String) = setState(jobId, ImportRunState.IMPORTING)
    fun markComplete(jobId: String) = setState(jobId, ImportRunState.COMPLETE)
    fun markFailed(jobId: String, message: String) = setState(jobId, ImportRunState.FAILED, message)

    fun requestPause(jobId: String) = transaction {
        prepareStatement("UPDATE jobs SET pause_requested = 1, state = ?, updated_at = ? WHERE id = ?").use {
            it.setString(1, ImportRunState.PAUSE_REQUESTED.name); it.setString(2, Instant.now().toString()); it.setString(3, jobId); it.executeUpdate()
        }
    }

    fun consumePauseRequest(jobId: String): Boolean = transaction {
        prepareStatement("SELECT pause_requested FROM jobs WHERE id = ?").use { query ->
            query.setString(1, jobId)
            val requested = query.executeQuery().use { it.next() && it.getInt(1) != 0 }
            if (requested) setState(jobId, ImportRunState.PAUSED)
            requested
        }
    }

    fun incompleteAssets(jobId: String): List<StoredAsset> = connection.prepareStatement(
        "SELECT * FROM assets WHERE job_id = ? AND media_id IS NULL ORDER BY source_id",
    ).use { query -> query.setString(1, jobId); query.executeQuery().use(::readAssets) }

    fun completedCount(jobId: String): Int = count(jobId, "media_id IS NOT NULL")
    fun totalCount(jobId: String): Int = count(jobId, "1 = 1")
    fun totalBytes(jobId: String): Long = connection.prepareStatement("SELECT COALESCE(SUM(byte_count), 0) FROM assets WHERE job_id = ?").use {
        it.setString(1, jobId); it.executeQuery().use { result -> result.next(); result.getLong(1) }
    }

    fun markStaged(jobId: String, sourceId: String, stagedPath: Path) = transaction {
        prepareStatement("UPDATE assets SET staged_path = ? WHERE job_id = ? AND source_id = ?").use {
            it.setString(1, stagedPath.toString()); it.setString(2, jobId); it.setString(3, sourceId); it.executeUpdate()
        }
    }

    fun markImported(jobId: String, sourceId: String, mediaId: String, alreadyExisted: Boolean, chunk: Int) = transaction {
        prepareStatement("UPDATE assets SET media_id = ?, already_existed = ?, chunk_number = ? WHERE job_id = ? AND source_id = ?").use {
            it.setString(1, mediaId); it.setInt(2, if (alreadyExisted) 1 else 0); it.setInt(3, chunk); it.setString(4, jobId); it.setString(5, sourceId); it.executeUpdate()
        }
    }

    fun mediaIdFor(jobId: String, sourceId: String): String? = connection.prepareStatement(
        "SELECT media_id FROM assets WHERE job_id = ? AND source_id = ?",
    ).use { query -> query.setString(1, jobId); query.setString(2, sourceId); query.executeQuery().use { if (it.next()) it.getString(1) else null } }

    fun markRemoteChunkComplete(jobId: String, chunk: Int, remoteId: String) = transaction {
        prepareStatement("INSERT OR REPLACE INTO remote_chunks VALUES (?, ?, ?, ?)").use {
            it.setString(1, jobId); it.setInt(2, chunk); it.setString(3, remoteId); it.setString(4, Instant.now().toString()); it.executeUpdate()
        }
    }

    fun importedChunks(jobId: String): List<Int> = connection.prepareStatement(
        "SELECT DISTINCT chunk_number FROM assets WHERE job_id = ? AND chunk_number IS NOT NULL ORDER BY chunk_number",
    ).use { query -> query.setString(1, jobId); query.executeQuery().use { result -> buildList { while (result.next()) add(result.getInt(1)) } } }

    fun nextChunkNumber(jobId: String): Int = connection.prepareStatement(
        "SELECT COALESCE(MAX(chunk_number), 0) + 1 FROM assets WHERE job_id = ?",
    ).use { query -> query.setString(1, jobId); query.executeQuery().use { result -> result.next(); result.getInt(1) } }

    fun remoteChunkCompleted(jobId: String, chunk: Int, remoteId: String): Boolean = connection.prepareStatement(
        "SELECT 1 FROM remote_chunks WHERE job_id = ? AND chunk_number = ? AND remote_id = ?",
    ).use { query -> query.setString(1, jobId); query.setInt(2, chunk); query.setString(3, remoteId); query.executeQuery().use { it.next() } }

    fun mediaIdsForChunk(jobId: String, chunk: Int): List<String> = connection.prepareStatement(
        "SELECT media_id FROM assets WHERE job_id = ? AND chunk_number = ? AND media_id IS NOT NULL",
    ).use { query -> query.setString(1, jobId); query.setInt(2, chunk); query.executeQuery().use { result -> buildList { while (result.next()) add(result.getString(1)) } } }

    fun albumId(jobId: String, sourceAlbumName: String): String? = connection.prepareStatement(
        "SELECT lasco_album_id FROM albums WHERE job_id = ? AND source_album_name = ?",
    ).use { query -> query.setString(1, jobId); query.setString(2, sourceAlbumName); query.executeQuery().use { if (it.next()) it.getString(1) else null } }

    fun recordAlbum(jobId: String, sourceAlbumName: String, lascoAlbumId: String) = transaction {
        prepareStatement("INSERT OR IGNORE INTO albums VALUES (?, ?, ?)").use {
            it.setString(1, jobId); it.setString(2, sourceAlbumName); it.setString(3, lascoAlbumId); it.executeUpdate()
        }
    }

    private fun count(jobId: String, predicate: String): Int = connection.prepareStatement("SELECT COUNT(*) FROM assets WHERE job_id = ? AND $predicate").use {
        it.setString(1, jobId); it.executeQuery().use { result -> result.next(); result.getInt(1) }
    }

    private fun setState(jobId: String, state: ImportRunState, failure: String? = null) = transaction {
        prepareStatement("UPDATE jobs SET state = ?, failure = ?, updated_at = ? WHERE id = ?").use {
            it.setString(1, state.name); it.setString(2, failure); it.setString(3, Instant.now().toString()); it.setString(4, jobId); it.executeUpdate()
        }
    }

    private fun readAssets(result: java.sql.ResultSet): List<StoredAsset> = buildList {
        while (result.next()) add(
            StoredAsset(
                ImportAsset(result.getString("source_id"), ImportSource.valueOf(result.getString("source")),
                    app.lasco.importer.model.ResourceRole.valueOf(result.getString("role")), result.getString("display_name"), result.getLong("byte_count"),
                    json.decodeFromString<SourceMetadata>(result.getString("metadata_json")), result.getString("source_locator"),
                    json.decodeFromString(result.getString("albums_json")), result.getString("aae_source_id"), result.getString("live_video_source_id")),
                result.getString("staged_path")?.let(Path::of), result.getString("media_id"), result.getObject("chunk_number") as? Int,
            ),
        )
    }

    private inline fun <T> transaction(block: Connection.() -> T): T = synchronized(connection) {
        connection.autoCommit = false
        try { connection.block().also { connection.commit() } } catch (error: Throwable) { connection.rollback(); throw error } finally { connection.autoCommit = true }
    }

    override fun close() = connection.close()
}

data class StoredAsset(val asset: ImportAsset, val stagedPath: Path?, val mediaId: String?, val chunkNumber: Int?)
