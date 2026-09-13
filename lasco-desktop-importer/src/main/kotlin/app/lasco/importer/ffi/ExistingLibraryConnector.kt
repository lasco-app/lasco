package app.lasco.importer.ffi

import uniffi.lasco_ffi.FfiLascoCloudImportConfig
import uniffi.lasco_ffi.ffiAddExistingLibraryFixedPath
import uniffi.lasco_ffi.ffiAddExistingLibraryLascoCloud
import uniffi.lasco_ffi.ffiAddExistingLibraryS3
import uniffi.lasco_ffi.ffiAddExistingLibrarySmb
import java.nio.file.Path

data class LibraryCredentials(val nickname: String, val username: String, val password: String, val newUsername: String? = null, val newPassword: String? = null)

/** USB is intentionally absent: the importer supports every remote usable by desktop FFI. */
sealed interface ExistingRemote {
    val name: String
    data class FixedPath(override val name: String, val path: String) : ExistingRemote
    data class S3(override val name: String, val endpoint: String, val bucket: String, val region: String, val prefix: String, val accessKey: String, val secretKey: String) : ExistingRemote
    data class Smb(override val name: String, val server: String, val port: Int, val share: String, val prefix: String, val username: String, val password: String, val domain: String?) : ExistingRemote
    data class LascoCloud(override val name: String, val cloudBaseUrl: String, val email: String, val cloudPassword: String, val platform: String = "desktop-importer", val appVersion: String = "0.1.0") : ExistingRemote
}

object ExistingLibraryConnector {
    fun connect(credentials: LibraryCredentials, remote: ExistingRemote, appSupport: Path): UniffiLascoGateway {
        val appDir = appSupport.toString()
        val library = when (remote) {
            is ExistingRemote.FixedPath -> ffiAddExistingLibraryFixedPath(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.name, remote.path, appDir)
            is ExistingRemote.S3 -> ffiAddExistingLibraryS3(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.name, remote.endpoint, remote.bucket, remote.region, remote.prefix, remote.accessKey, remote.secretKey, appDir)
            is ExistingRemote.Smb -> ffiAddExistingLibrarySmb(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.name, remote.server, remote.port.toUShort(), remote.share, remote.prefix, remote.username, remote.password, remote.domain, appDir)
            is ExistingRemote.LascoCloud -> ffiAddExistingLibraryLascoCloud(FfiLascoCloudImportConfig(credentials.nickname, credentials.username, credentials.password, credentials.newUsername, credentials.newPassword, remote.cloudBaseUrl, remote.email, remote.cloudPassword, remote.platform, remote.appVersion), appDir)
        }
        return UniffiLascoGateway(library, appDir)
    }
}
