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
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import java.security.KeyStore

/** Native Cloud API client. Tokens are deliberately scoped by local library id. */
internal class LascoCloud(private val context: Context) {
    private val tokens = CloudTokenStore(context)
    private val json = Json { ignoreUnknownKeys = true }
    private val baseUrl = BuildConfig.LASCO_CLOUD_URL.trimEnd('/')

    suspend fun login(libraryId: String, email: String, password: String): CloudLogin = withContext(Dispatchers.IO) {
        val response = request("/api/v1/sessions", null, LoginRequest(email, password, "android", BuildConfig.VERSION_NAME))
        val login = json.decodeFromString<CloudLogin>(response)
        tokens.put(libraryId, login.token)
        login
    }

    suspend fun storageCredentials(libraryId: String): List<CloudRemote> = withContext(Dispatchers.IO) {
        val token = tokens.get(libraryId) ?: throw CloudUnauthorizedException()
        try {
            json.decodeFromString<CloudCredentialsResponse>(request("/api/v1/storage-credentials", token, null)).remotes
        } catch (e: CloudUnauthorizedException) {
            tokens.remove(libraryId)
            throw e
        }
    }

    private fun request(path: String, token: String?, body: Any?): String {
        val connection = (URL(baseUrl + path).openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            setRequestProperty("Accept", "application/json")
            token?.let { setRequestProperty("Authorization", "Bearer $it") }
            if (body != null) {
                doOutput = true
                setRequestProperty("Content-Type", "application/json")
                outputStream.use { it.write(json.encodeToString(LoginRequest.serializer(), body as LoginRequest).encodeToByteArray()) }
            }
        }
        val code = connection.responseCode
        if (code == HttpURLConnection.HTTP_UNAUTHORIZED) throw CloudUnauthorizedException()
        val stream = if (code in 200..299) connection.inputStream else connection.errorStream
        val text = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
        if (code !in 200..299) throw IllegalStateException("Lasco Cloud request failed ($code): $text")
        return text
    }
}

internal class CloudUnauthorizedException : IllegalStateException("Authenticate with Lasco Cloud again")

@Serializable private data class LoginRequest(val email: String, val password: String, val platform: String, @SerialName("app_version") val appVersion: String)
@Serializable internal data class CloudLogin(val token: String, @SerialName("expires_at") val expiresAt: String)
@Serializable internal data class CloudCredentialsResponse(val remotes: List<CloudRemote>)
@Serializable internal data class CloudRemote(
    val id: String, val name: String, val endpoint: String, val bucket: String, val region: String,
    @SerialName("path_prefix") val pathPrefix: String = "", @SerialName("access_key_id") val accessKeyId: String,
    @SerialName("secret_access_key") val secretAccessKey: String, @SerialName("session_token") val sessionToken: String? = null,
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
