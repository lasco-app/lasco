package com.lasco.lasco.ui.manage

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.components.LascoCheckbox
import com.lasco.lasco.ui.components.LascoField
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.LascoException
import uniffi.lasco_ffi.ffiTestS3Remote

@Composable
private fun FullSheet(onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val colors = LascoTheme.colors
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Column(modifier = Modifier.fillMaxSize().background(colors.bg)) {
            Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 24.dp)) {
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "✕",
                    style = LascoTheme.type.body(18),
                    color = colors.ink,
                    modifier = Modifier.clickable { onDismiss() },
                )
            }
            content()
        }
    }
}

/**
 * Ported from RemoteTypePickerSheet. S3 is always offered, local FS only in
 * expert mode, matching the Swift gating.
 */
@Composable
fun RemoteTypePickerDialog(
    expertMode: Boolean,
    onS3: () -> Unit,
    onLocalFS: () -> Unit,
    onDismiss: () -> Unit,
) {
    val colors = LascoTheme.colors
    FullSheet(onDismiss = onDismiss) {
        Column(modifier = Modifier.padding(horizontal = 32.dp)) {
            Text(text = "Add remote", style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Text(
                text = "Choose a remote type",
                style = LascoTheme.type.subtitle(),
                color = colors.inkMuted,
                modifier = Modifier.padding(top = 8.dp, bottom = 32.dp),
            )
            LascoPrimaryButton(text = "Add S3-compatible remote", onClick = onS3)
            if (expertMode) {
                Spacer(modifier = Modifier.height(12.dp))
                LascoPrimaryButton(text = "Add local filesystem remote", onClick = onLocalFS)
            }
        }
    }
}

/**
 * Ported from AddS3RemoteView. Tests the connection through the top level
 * ffiTestS3Remote call, then on submit adds the remote and initializes it.
 */
@Composable
fun AddS3RemoteDialog(
    onDismiss: () -> Unit,
    onResult: (name: String, error: String?) -> Unit,
) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val repo = remember { LibraryRepository.from(context) }
    val scope = rememberCoroutineScope()

    var name by remember { mutableStateOf("") }
    var endpoint by remember { mutableStateOf("") }
    var bucket by remember { mutableStateOf("") }
    var region by remember { mutableStateOf("") }
    var pathPrefix by remember { mutableStateOf("") }
    var accessKey by remember { mutableStateOf("") }
    var secretKey by remember { mutableStateOf("") }
    var acknowledged by remember { mutableStateOf(false) }
    var testing by remember { mutableStateOf(false) }
    var testMessage by remember { mutableStateOf<Pair<Boolean, String>?>(null) }
    var submitting by remember { mutableStateOf(false) }

    val canTest = endpoint.isNotBlank() && bucket.isNotBlank() && accessKey.isNotBlank() && secretKey.isNotBlank() && !testing
    val isValid = name.isNotBlank() && endpoint.isNotBlank() && bucket.isNotBlank() &&
        accessKey.isNotBlank() && secretKey.isNotBlank() && acknowledged && !submitting

    FullSheet(onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f, fill = false)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(text = "Add a S3 remote", style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "Works with any S3-compatible service.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )
            LascoField(label = "Remote name", value = name, onValueChange = { name = it }, placeholder = "my-backups")
            LascoField(label = "Endpoint URL", value = endpoint, onValueChange = { endpoint = it }, placeholder = "https://region1.example-s3-server.com")
            LascoField(label = "Bucket", value = bucket, onValueChange = { bucket = it }, placeholder = "my-photos-bucket")
            LascoField(label = "Region", value = region, onValueChange = { region = it }, placeholder = "region1")
            LascoField(label = "Path prefix (optional)", value = pathPrefix, onValueChange = { pathPrefix = it }, placeholder = "photos/")
            LascoField(label = "Access key", value = accessKey, onValueChange = { accessKey = it })
            LascoField(label = "Secret key", value = secretKey, onValueChange = { secretKey = it }, secure = true)
            Text(
                text = "The secret key is stored locally and encrypted with the library password.",
                style = LascoTheme.type.body(13),
                color = colors.inkMuted,
            )
            LascoCheckbox(
                checked = acknowledged,
                onCheckedChange = { acknowledged = it },
                label = "I understand this app will upload my photos to the S3 bucket configured above.",
            )
            Text(
                text = if (testing) "Testing…" else "Test connection",
                style = LascoTheme.type.body(),
                color = if (canTest) colors.ink else colors.inkMuted,
                modifier = Modifier.clickable(enabled = canTest) {
                    testing = true
                    testMessage = null
                    scope.launch {
                        testMessage = try {
                            ffiTestS3Remote(endpoint, bucket, region, pathPrefix, accessKey, secretKey)
                            true to "Connection succeeded."
                        } catch (e: LascoException) {
                            false to (e.message?.ifBlank { "Connection failed." } ?: "Connection failed.")
                        }
                        testing = false
                    }
                },
            )
            testMessage?.let { (ok, msg) ->
                Text(text = msg, style = LascoTheme.type.body(13), color = if (ok) colors.ok else colors.error)
            }
            Spacer(modifier = Modifier.height(24.dp))
        }
        Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 20.dp)) {
            LascoPrimaryButton(
                text = "Add Remote",
                enabled = isValid,
                onClick = {
                    submitting = true
                    scope.launch {
                        try {
                            val remoteId = repo.addRemoteS3(name, endpoint, bucket, region, pathPrefix, accessKey, secretKey)
                            onDismiss()
                            val error = try {
                                repo.initializeRemote(remoteId, null)
                                repo.pushRemote(remoteId, null)
                                null
                            } catch (e: Exception) {
                                e.message?.ifBlank { null } ?: "Initialization failed"
                            }
                            onResult(name, error)
                        } catch (e: Exception) {
                            submitting = false
                            onResult(name, e.message?.ifBlank { null } ?: "Failed to add remote")
                        }
                    }
                },
            )
        }
    }
}

/**
 * Ported from AddLocalFSRemoteView. Expert-mode only, single name field, uses
 * the debug local-android remote kind, which resolves paths against the
 * app's own data dir instead of requiring a separate app-support dir.
 */
@Composable
fun AddLocalFSRemoteDialog(
    onDismiss: () -> Unit,
    onResult: (name: String, error: String?) -> Unit,
) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val repo = remember { LibraryRepository.from(context) }
    val scope = rememberCoroutineScope()

    var name by remember { mutableStateOf("") }
    var submitting by remember { mutableStateOf(false) }
    val isValid = name.isNotBlank() && !submitting

    FullSheet(onDismiss = onDismiss) {
        Column(
            modifier = Modifier.fillMaxWidth().weight(1f, fill = false).padding(horizontal = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(text = "Add local FS remote", style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "Saves the data locally, use it only for test purposes!",
                style = LascoTheme.type.body(13),
                color = colors.inkMuted,
            )
            LascoField(label = "Remote name", value = name, onValueChange = { name = it }, placeholder = "local-test")
        }
        Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 20.dp)) {
            LascoPrimaryButton(
                text = "Add Remote",
                enabled = isValid,
                onClick = {
                    submitting = true
                    scope.launch {
                        try {
                            val remoteId = repo.addRemoteDebugLocalAndroid(name)
                            onDismiss()
                            val error = try {
                                repo.initializeRemote(remoteId, null)
                                repo.pushRemote(remoteId, null)
                                null
                            } catch (e: Exception) {
                                e.message?.ifBlank { null } ?: "Initialization failed"
                            }
                            onResult(name, error)
                        } catch (e: Exception) {
                            submitting = false
                            onResult(name, e.message?.ifBlank { null } ?: "Failed to add remote")
                        }
                    }
                },
            )
        }
    }
}
