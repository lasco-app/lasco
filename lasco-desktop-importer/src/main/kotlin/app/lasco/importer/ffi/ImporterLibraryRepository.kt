package app.lasco.importer.ffi

import uniffi.lasco_ffi.FfiLascoCloudImportConfig
import uniffi.lasco_ffi.FfiLibrary
import uniffi.lasco_ffi.FfiLibraryId
import uniffi.lasco_ffi.FfiRemoteUuid
import uniffi.lasco_ffi.ffiAddExistingLibraryFixedPath
import uniffi.lasco_ffi.ffiAddExistingLibraryLascoCloud
import uniffi.lasco_ffi.ffiAddExistingLibraryS3
import uniffi.lasco_ffi.ffiAddExistingLibrarySmb
import uniffi.lasco_ffi.ffiDeleteLibrary
import uniffi.lasco_ffi.ffiOpenCached
import uniffi.lasco_ffi.listLibraries
import java.nio.file.Path

internal const val DEFAULT_LASCO_CLOUD_BASE_URL = "https://cloud.getlasco.app"

data class LibraryCredentials(
    val nickname: String,
    val username: String,
    val password: String,
    val newUsername: String? = null,
    val newPassword: String? = null,
)

/** USB is intentionally absent: the desktop importer supports only remotely backed libraries. */
sealed interface ExistingRemote {
    val name: String
    data class FixedPath(override val name: String, val path: String) : ExistingRemote
    data class S3(override val name: String, val endpoint: String, val bucket: String, val region: String, val prefix: String, val accessKey: String, val secretKey: String) : ExistingRemote
    data class Smb(override val name: String, val server: String, val port: Int, val share: String, val prefix: String, val username: String, val password: String, val domain: String?) : ExistingRemote
    data class LascoCloud(override val name: String, val cloudBaseUrl: String, val email: String, val cloudPassword: String, val platform: String = "desktop-importer", val appVersion: String = "0.1.0") : ExistingRemote
}

sealed interface RemoteConfig {
    val name: String
    data class S3(override val name: String, val endpoint: String, val bucket: String, val region: String, val prefix: String, val accessKey: String, val secretKey: String) : RemoteConfig
    data class Smb(override val name: String, val server: String, val port: Int, val share: String, val prefix: String, val username: String, val password: String, val domain: String?) : RemoteConfig
}

data class ImporterLibrarySummary(
    val libraryId: String,
    val nickname: String,
    val username: String?,
    val remotes: List<LascoRemote>,
    val loadError: String?,
)

sealed interface OpenResult {
    data class Open(val gateway: LascoGateway) : OpenResult
    data object CredentialsRequired : OpenResult
    data class Failed(val message: String) : OpenResult
}

interface ImporterLibraryRepository : AutoCloseable {
    suspend fun list(): List<ImporterLibrarySummary>
    suspend fun openCached(libraryId: String): OpenResult
    suspend fun openWithCredentials(credentials: LibraryCredentials): OpenResult
    suspend fun addInitialLibrary(credentials: LibraryCredentials, remote: ExistingRemote): ImporterLibrarySummary
    suspend fun addRemote(libraryId: String, remote: RemoteConfig)
    suspend fun deleteLocalSetup(libraryId: String)
    suspend fun closeAll()
}

/** Keeps the rollback guarantee independently testable from UniFFI handles. */
internal suspend fun initializeRemoteOrRollback(
    initialize: suspend () -> Unit,
    remove: suspend () -> Unit,
) {
    try {
        initialize()
    } catch (failure: Throwable) {
        runCatching { remove() }
        throw failure
    }
}

/** Owns every FFI handle. Composables only dispatch events and render summaries/results. */
class UniffiImporterLibraryRepository(
    private val appSupport: Path,
    private val cloudBaseUrl: () -> String = { DEFAULT_LASCO_CLOUD_BASE_URL },
) : ImporterLibraryRepository {
    private val openGateways = mutableMapOf<String, UniffiLascoGateway>()
    private val appDir get() = appSupport.toString()

    override suspend fun list(): List<ImporterLibrarySummary> = listLibraries(appDir).map { entry ->
        val cached = entry.username?.let { username -> runCatching { ffiOpenCached(entry.nickname, username, appDir) }.getOrNull() }
        val remotes = cached?.use { library -> library.listRemotes().map { LascoRemote(it.remoteId.value, it.name, it.kind) } }.orEmpty()
        ImporterLibrarySummary(entry.libraryId.value, entry.nickname, entry.username, remotes, entry.loadError)
    }

    override suspend fun openCached(libraryId: String): OpenResult {
        openGateways[libraryId]?.let { return OpenResult.Open(it) }
        val entry = listLibraries(appDir).firstOrNull { it.libraryId.value == libraryId }
            ?: return OpenResult.Failed("The local importer setup no longer exists.")
        val username = entry.username ?: return OpenResult.CredentialsRequired
        val library = try { ffiOpenCached(entry.nickname, username, appDir) }
            catch (failure: Throwable) { return OpenResult.Failed(failure.message ?: "Could not open this library.") }
            ?: return OpenResult.CredentialsRequired
        try {
            library.configureLascoCloudAuth(cloudBaseUrl())
        } catch (failure: Throwable) {
            library.close()
            return OpenResult.Failed(failure.message ?: "Could not configure Lasco Cloud for this import.")
        }
        return OpenResult.Open(UniffiLascoGateway(library, appDir).also { openGateways[libraryId] = it })
    }

    override suspend fun openWithCredentials(credentials: LibraryCredentials): OpenResult = try {
        val library = FfiLibrary.open(credentials.nickname, credentials.username, credentials.password, appDir)
        try {
            library.configureLascoCloudAuth(cloudBaseUrl())
        } catch (failure: Throwable) {
            library.close()
            throw failure
        }
        val gateway = UniffiLascoGateway(library, appDir)
        openGateways[gateway.libraryId] = gateway
        OpenResult.Open(gateway)
    } catch (failure: Throwable) {
        OpenResult.Failed(failure.message ?: "Could not unlock this library.")
    }

    override suspend fun addInitialLibrary(credentials: LibraryCredentials, remote: ExistingRemote): ImporterLibrarySummary {
        val library = when (remote) {
            is ExistingRemote.FixedPath -> ffiAddExistingLibraryFixedPath(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.name, remote.path, appDir)
            is ExistingRemote.S3 -> ffiAddExistingLibraryS3(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.name, remote.endpoint, remote.bucket, remote.region, remote.prefix, remote.accessKey, remote.secretKey, appDir)
            is ExistingRemote.Smb -> ffiAddExistingLibrarySmb(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.name, remote.server, remote.port.toUShort(), remote.share, remote.prefix, remote.username, remote.password, remote.domain, appDir)
            is ExistingRemote.LascoCloud -> ffiAddExistingLibraryLascoCloud(FfiLascoCloudImportConfig(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.cloudBaseUrl, remote.email, remote.cloudPassword, remote.platform, remote.appVersion), appDir)
        }
        library.use { opened ->
            return ImporterLibrarySummary(
                opened.libraryId().value,
                credentials.nickname,
                credentials.username,
                opened.listRemotes().map { LascoRemote(it.remoteId.value, it.name, it.kind) },
                null,
            )
        }
    }

    override suspend fun addRemote(libraryId: String, remote: RemoteConfig) {
        val gateway = when (val result = openCached(libraryId)) {
            is OpenResult.Open -> result.gateway
            OpenResult.CredentialsRequired -> error("Unlock this library before adding a remote.")
            is OpenResult.Failed -> error(result.message)
        }
        val library = (gateway as? UniffiLascoGateway)?.ffiLibrary()
            ?: error("This importer library cannot add remotes.")
        val remoteId = when (remote) {
            is RemoteConfig.S3 -> library.addRemoteS3(remote.name, remote.endpoint, remote.bucket, remote.region, remote.prefix, remote.accessKey, remote.secretKey)
            is RemoteConfig.Smb -> library.addRemoteSmb(remote.name, remote.server, remote.port.toUShort(), remote.share, remote.prefix, remote.username, remote.password, remote.domain)
        }
        initializeRemoteOrRollback(
            initialize = { library.initializeRemote(remoteId, appDir) },
            remove = { library.removeRemote(remoteId) },
        )
    }

    override suspend fun deleteLocalSetup(libraryId: String) {
        openGateways.remove(libraryId)?.close()
        ffiDeleteLibrary(FfiLibraryId(libraryId), appDir)
    }

    override suspend fun closeAll() {
        openGateways.values.forEach(UniffiLascoGateway::close)
        openGateways.clear()
    }

    override fun close() { openGateways.values.forEach(UniffiLascoGateway::close); openGateways.clear() }
}
