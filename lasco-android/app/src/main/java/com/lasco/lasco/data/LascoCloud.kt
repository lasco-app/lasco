package com.lasco.lasco.data

import android.content.Context
import android.util.Base64
import com.lasco.lasco.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.net.HttpURLConnection
import java.net.URL
import java.io.IOException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import java.security.KeyStore

/** Native Cloud API client. Tokens are deliberately scoped by local library id. */
internal class LascoCloud(private val context: Context) {
    private val tokens = CloudTokenStore(context)
    private val json = Json { ignoreUnknownKeys = true }
    private val baseUrl get() = DevelopmentCloudEndpoint.activeUrl(context).trimEnd('/')

    suspend fun login(libraryId: String, email: String, password: String): CloudLogin = withContext(Dispatchers.IO) {
        val response = request("/api/v1/sessions", "POST", null, json.encodeToString(LoginRequest(email, password, "android", BuildConfig.VERSION_NAME)))
        val login = json.decodeFromString<CloudLogin>(response)
        tokens.put(libraryId, login.token)
        login
    }

    fun logout(libraryId: String) {
        tokens.remove(libraryId)
    }

    fun isLoggedIn(libraryId: String): Boolean = tokens.get(libraryId) != null

    suspend fun storageCredentials(libraryId: String): List<CloudRemote> = withContext(Dispatchers.IO) {
        val token = tokens.get(libraryId) ?: throw CloudUnauthorizedException()
        try {
            val remotes = json.decodeFromString<CloudRemoteInfoResponse>(
                request("/api/v1/remotes", "GET", token, null),
            ).remotes
            val credentials = json.decodeFromString<CloudCredentialsResponse>(
                request("/api/v1/storage-credentials", "POST", token, null),
            ).credentials.associateBy { it.id }
            if (remotes.size != 2 || credentials.size != remotes.size || remotes.any { it.id !in credentials }) {
                throw CloudInvalidRemoteCountException()
            }
            remotes.map { remote ->
                val credential = checkNotNull(credentials[remote.id])
                CloudRemote(
                    id = remote.id,
                    libraryId = remote.libraryId,
                    name = remote.name,
                    endpoint = remote.endpoint,
                    bucket = remote.bucket,
                    region = remote.region,
                    pathPrefix = remote.pathPrefix,
                    accessKeyId = credential.accessKeyId,
                    secretAccessKey = credential.secretAccessKey,
                    expiresAt = credential.expiresAt,
                )
            }
        } catch (e: CloudUnauthorizedException) {
            tokens.remove(libraryId)
            throw e
        }
    }

    suspend fun setRemoteLibraryIds(libraryId: String, remoteIds: List<String>) = withContext(Dispatchers.IO) {
        val token = tokens.get(libraryId) ?: throw CloudUnauthorizedException()
        try {
            val body = json.encodeToString(CloudRemoteLibraryIdRequest(libraryId))
            remoteIds.forEach { remoteId ->
                request("/api/v1/remotes/$remoteId/library-id", "PUT", token, body)
            }
        } catch (e: CloudUnauthorizedException) {
            tokens.remove(libraryId)
            throw e
        }
    }

    suspend fun subscription(libraryId: String): CloudAccount = withContext(Dispatchers.IO) {
        val token = tokens.get(libraryId) ?: throw CloudUnauthorizedException()
        try {
            json.decodeFromString<CloudAccount>(
                request("/api/v1/subscription", "GET", token, null),
            )
        } catch (e: CloudUnauthorizedException) {
            tokens.remove(libraryId)
            throw e
        }
    }

    private fun request(path: String, method: String, token: String?, body: String?): String {
        try {
            val connection = (URL(baseUrl + path).openConnection() as HttpURLConnection).apply {
                requestMethod = method
                setRequestProperty("Accept", "application/json")
                token?.let { setRequestProperty("Authorization", "Bearer $it") }
                if (body != null) {
                    doOutput = true
                    setRequestProperty("Content-Type", "application/json")
                    outputStream.use { it.write(body.encodeToByteArray()) }
                }
            }
            val code = connection.responseCode
            if (code == HttpURLConnection.HTTP_UNAUTHORIZED) throw CloudUnauthorizedException()
            val stream = if (code in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
            if (code !in 200..299) throw CloudRequestException(path, code)
            return text
        } catch (e: CloudException) {
            throw e
        } catch (e: IOException) {
            throw CloudConnectionException(baseUrl)
        }
    }
}

internal sealed class CloudException(message: String) : IllegalStateException(message)
internal class CloudUnauthorizedException : CloudException("Authenticate with Lasco Cloud again")
internal class CloudInvalidRemoteCountException : CloudException("Lasco Cloud must provide two storage remotes")
internal class CloudRemoteAlreadyAssociatedException : CloudException(
    "Lasco Cloud storage is already associated with another library",
)
internal class CloudSignOutRequiresRemoteRemovalException : CloudException(
    "Remove the Lasco Cloud remotes before signing out",
)
internal class CloudAlreadyConnectedException : CloudException(
    "Lasco Cloud is already connected for this library",
)
private class CloudConnectionException(endpoint: String) : CloudException(
    "Couldn't reach Lasco Cloud at $endpoint. Make sure the server is running. " +
        "On a physical Android device, use your computer's LAN address instead of localhost or 127.0.0.1.",
)
private class CloudRequestException(endpoint: String, statusCode: Int) : CloudException(
    "Lasco Cloud request to $endpoint failed (HTTP $statusCode).",
)

@Serializable private data class LoginRequest(val email: String, val password: String, val platform: String, @SerialName("app_version") val appVersion: String)
@Serializable private data class CloudRemoteLibraryIdRequest(@SerialName("library_id") val libraryId: String)
@Serializable internal data class CloudLogin(val token: String)
@Serializable data class CloudAccount(
    val email: String,
    val subscription: CloudSubscription?,
)
@Serializable data class CloudSubscription(
    @SerialName("plan_id") val planId: String,
    @SerialName("plan_name") val planName: String,
    val status: String,
    @SerialName("storage_quota_bytes") val storageQuotaBytes: Long,
    @SerialName("renews_at") val renewsAt: String,
)
@Serializable private data class CloudRemoteInfoResponse(val remotes: List<CloudRemoteInfo>)
@Serializable private data class CloudCredentialsResponse(val credentials: List<CloudRemoteCredentials>)
@Serializable private data class CloudRemoteInfo(
    val id: String, val name: String, val endpoint: String, val bucket: String, val region: String,
    @SerialName("path_prefix") val pathPrefix: String,
    @SerialName("library_id") val libraryId: String?,
)
@Serializable private data class CloudRemoteCredentials(
    val id: String, @SerialName("access_key_id") val accessKeyId: String,
    @SerialName("secret_access_key") val secretAccessKey: String,
    @SerialName("expires_at") val expiresAt: String,
)
@Serializable internal data class CloudRemote(
    val id: String, val libraryId: String?, val name: String, val endpoint: String, val bucket: String, val region: String,
    val pathPrefix: String, val accessKeyId: String, val secretAccessKey: String,
    @SerialName("expires_at") val expiresAt: String,
)

/** A small Keystore-backed encrypted preference, avoiding tokens in normal preferences. */
private class CloudTokenStore(context: Context) {
    private val prefs = context.getSharedPreferences("lasco_cloud_tokens", Context.MODE_PRIVATE)
    private val alias = "lasco_cloud_token_key"
    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance("AES", "AndroidKeyStore").apply { init(android.security.keystore.KeyGenParameterSpec.Builder(alias, android.security.keystore.KeyProperties.PURPOSE_ENCRYPT or android.security.keystore.KeyProperties.PURPOSE_DECRYPT).setBlockModes(android.security.keystore.KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(android.security.keystore.KeyProperties.ENCRYPTION_PADDING_NONE).build()) }.generateKey()
    }
    fun put(id: String, token: String) { val cipher = Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.ENCRYPT_MODE, key()); prefs.edit().putString(id, Base64.encodeToString(cipher.iv + cipher.doFinal(token.encodeToByteArray()), Base64.NO_WRAP)).apply() }
    fun get(id: String): String? = prefs.getString(id, null)?.let { encoded -> runCatching { val all = Base64.decode(encoded, Base64.NO_WRAP); val cipher = Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, all.copyOfRange(0, 12))); cipher.doFinal(all.copyOfRange(12, all.size)).decodeToString() }.getOrNull() }
    fun remove(id: String) { prefs.edit().remove(id).apply() }
}
