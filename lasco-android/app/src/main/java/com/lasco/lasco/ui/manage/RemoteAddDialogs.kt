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
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
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
import uniffi.lasco_ffi.FfiRemoteUuid
import uniffi.lasco_ffi.ffiTestS3Remote
import uniffi.lasco_ffi.ffiTestSmbRemote

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
    showCloud: Boolean,
    onCloud: () -> Unit,
    onS3: () -> Unit,
    onSmb: () -> Unit,
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
            if (showCloud) {
                LascoPrimaryButton(text = "Authenticate with Lasco Cloud", onClick = onCloud)
                Spacer(modifier = Modifier.height(12.dp))
            }
            LascoPrimaryButton(text = "Add S3-compatible remote", onClick = onS3)
            Spacer(modifier = Modifier.height(12.dp))
            LascoPrimaryButton(text = "Add SMB remote", onClick = onSmb)
            if (expertMode) {
                Spacer(modifier = Modifier.height(12.dp))
                LascoPrimaryButton(text = "Add local filesystem remote", onClick = onLocalFS)
            }
        }
    }
}

/** Adds an SMB 2/3 share after a real write/read/delete connection probe. */
@Composable
fun AddSmbRemoteDialog(onDismiss: () -> Unit, onResult: (name: String, error: String?) -> Unit) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val repo = remember { LibraryRepository.from(context) }
    val scope = rememberCoroutineScope()
    var name by rememberSaveable { mutableStateOf("") }
    var server by rememberSaveable { mutableStateOf("") }
    var portText by rememberSaveable { mutableStateOf("445") }
    var share by rememberSaveable { mutableStateOf("") }
    var pathPrefix by rememberSaveable { mutableStateOf("") }
    var username by rememberSaveable { mutableStateOf("") }
    var password by rememberSaveable { mutableStateOf("") }
    var domain by rememberSaveable { mutableStateOf("") }
    var acknowledged by rememberSaveable { mutableStateOf(false) }
    var testing by rememberSaveable { mutableStateOf(false) }
    var submitting by rememberSaveable { mutableStateOf(false) }
    var message by rememberSaveable { mutableStateOf<Pair<Boolean, String>?>(null) }
    val port = portText.toUShortOrNull()
    val canTest = server.isNotBlank() && port != null && share.isNotBlank() && username.isNotBlank() && password.isNotBlank() && !testing
    val canSubmit = name.isNotBlank() && canTest && acknowledged && !submitting
    FullSheet(onDismiss = onDismiss) {
        Column(
            modifier = Modifier.fillMaxWidth().weight(1f, fill = false).verticalScroll(rememberScrollState()).padding(horizontal = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text("Add an SMB remote", style = LascoTheme.type.title(26), color = colors.ink)
            Text("Connect to an SMB 2 or SMB 3 shared folder on your NAS, server, or local network. For \\nas.local\\photos, enter photos as the shared folder name, then optionally choose a folder within it.", style = LascoTheme.type.body(16), color = colors.inkSub)
            LascoField(label = "Remote name", value = name, onValueChange = { name = it }, placeholder = "home-nas", testTag = "smb-remote.name")
            LascoField(label = "Server address", value = server, onValueChange = { server = it }, placeholder = "nas.local or 192.168.1.20", testTag = "smb-remote.server")
            LascoField(label = "Port", value = portText, onValueChange = { portText = it }, placeholder = "445", testTag = "smb-remote.port")
            LascoField(label = "Shared folder name", value = share, onValueChange = { share = it }, placeholder = "photos", testTag = "smb-remote.share")
            LascoField(label = "Folders within shared folder (optional)", value = pathPrefix, onValueChange = { pathPrefix = it }, placeholder = "lasco", testTag = "smb-remote.path")
            LascoField(label = "Username", value = username, onValueChange = { username = it }, testTag = "smb-remote.username")
            LascoField(label = "Domain or workgroup (optional)", value = domain, onValueChange = { domain = it }, placeholder = "WORKGROUP", testTag = "smb-remote.domain")
            LascoField(label = "Password", value = password, onValueChange = { password = it }, secure = true, testTag = "smb-remote.password")
            Text("The password is stored locally and encrypted with the library password.", style = LascoTheme.type.body(13), color = colors.inkMuted)
            LascoCheckbox(checked = acknowledged, onCheckedChange = { acknowledged = it }, label = "I understand this app will upload my photos to the SMB share configured above.")
            LascoPrimaryButton(text = if (testing) "Testing…" else "Test connection", enabled = canTest, onClick = {
                val validPort = port ?: return@LascoPrimaryButton
                testing = true; message = null
                scope.launch {
                    message = try {
                        ffiTestSmbRemote(server, validPort, share, pathPrefix, username, password, domain.ifBlank { null })
                        true to "Connection succeeded."
                    } catch (e: Exception) {
                        false to (e.message?.ifBlank { "Connection failed." } ?: "Connection failed.")
                    }
                    testing = false
                }
            })
            message?.let { (ok, text) -> Text(text, style = LascoTheme.type.body(13), color = if (ok) colors.ok else colors.error) }
            Spacer(modifier = Modifier.height(24.dp))
        }
        Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 20.dp)) {
            LascoPrimaryButton(text = if (submitting) "Adding…" else "Add Remote", enabled = canSubmit, onClick = {
                val validPort = port ?: return@LascoPrimaryButton
                submitting = true
                scope.launch {
                    var added: FfiRemoteUuid? = null
                    try {
                        val id = repo.addRemoteSmb(name, server, validPort, share, pathPrefix, username, password, domain.ifBlank { null })
                        added = id; repo.initializeRemote(id, null); onDismiss(); onResult(name, null)
                    } catch (e: Exception) {
                        added?.let { runCatching { repo.removeRemote(it) } }
                        message = false to (e.message?.ifBlank { "Failed to add remote" } ?: "Failed to add remote")
                    } finally { submitting = false }
                }
            })
        }
    }
}

@Composable
fun LascoCloudLoginDialog(onDismiss: () -> Unit, onResult: (String?) -> Unit) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val repo = remember { LibraryRepository.from(context) }
    val scope = rememberCoroutineScope()
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var submitting by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val completedSteps = remember { mutableStateListOf<String>() }
    var currentStep by remember { mutableStateOf<String?>(null) }
    FullSheet(onDismiss = onDismiss) {
        Column(
            modifier = Modifier.fillMaxWidth().weight(1f, fill = false).padding(horizontal = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text("Lasco Cloud", style = LascoTheme.type.title(26), color = colors.ink)
            Text("Authenticate this library with your Lasco Cloud account.", style = LascoTheme.type.body(16), color = colors.inkSub)
            LascoField(label = "Email", value = email, onValueChange = { email = it }, placeholder = "you@example.com")
            LascoField(label = "Password", value = password, onValueChange = { password = it }, secure = true)
            if (completedSteps.isNotEmpty() || currentStep != null) {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    completedSteps.forEach { step ->
                        Text("✓  $step", style = LascoTheme.type.body(13), color = colors.ok)
                    }
                    currentStep?.let { step ->
                        Text(step, style = LascoTheme.type.body(13), color = colors.inkSub)
                    }
                }
            }
            error?.let { Text(it, style = LascoTheme.type.body(13), color = colors.error) }
        }
        Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 20.dp)) {
            LascoPrimaryButton(
                text = if (submitting) "Authenticating…" else "Authenticate",
                enabled = email.isNotBlank() && password.isNotBlank() && !submitting,
                onClick = {
                    submitting = true; error = null; completedSteps.clear(); currentStep = "Authenticating…"
                    scope.launch {
                        try {
                            repo.authenticateLascoCloud(email, password) { step ->
                                when (step) {
                                    LibraryRepository.LascoCloudConnectionStep.Authenticated -> {
                                        completedSteps += "Authentication successful"
                                        currentStep = "Checking Cloud storage…"
                                    }
                                    LibraryRepository.LascoCloudConnectionStep.CredentialsReceived -> {
                                        completedSteps += "Cloud storage verified"
                                        currentStep = "Configuring storage remotes…"
                                    }
                                    LibraryRepository.LascoCloudConnectionStep.RemotesConfigured -> {
                                        completedSteps += "Storage remotes configured"
                                    }
                                }
                            }
                            onDismiss(); onResult(null)
                        } catch (e: Exception) {
                            error = e.message?.ifBlank { null } ?: "Could not authenticate with Lasco Cloud"
                            currentStep = null
                        } finally { submitting = false }
                    }
                },
            )
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
    var addError by remember { mutableStateOf<String?>(null) }

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
            addError?.let { message ->
                Text(text = message, style = LascoTheme.type.body(13), color = colors.error)
            }
            Spacer(modifier = Modifier.height(24.dp))
        }
        Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 20.dp)) {
            LascoPrimaryButton(
                text = "Add Remote",
                enabled = isValid,
                onClick = {
                    submitting = true
                    addError = null
                    scope.launch {
                        var addedRemoteId: FfiRemoteUuid? = null
                        try {
                            val remoteId = repo.addRemoteS3(name, endpoint, bucket, region, pathPrefix, accessKey, secretKey)
                            addedRemoteId = remoteId
                            repo.initializeRemote(remoteId, null)
                            onDismiss()
                            onResult(name, null)
                        } catch (e: Exception) {
                            addedRemoteId?.let { remoteId ->
                                runCatching { repo.removeRemote(remoteId) }
                            }
                            addError = e.message?.ifBlank { null } ?: "Failed to add remote"
                        }
                        submitting = false
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
    var addError by remember { mutableStateOf<String?>(null) }
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
            LascoField(
                label = "Remote name",
                value = name,
                onValueChange = { name = it },
                placeholder = "local-test",
                autoFocus = true,
                testTag = "local-fs-remote.name",
            )
            addError?.let { message ->
                Text(text = message, style = LascoTheme.type.body(13), color = colors.error)
            }
        }
        Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 20.dp)) {
            LascoPrimaryButton(
                text = "Add Remote",
                enabled = isValid,
                onClick = {
                    submitting = true
                    addError = null
                    scope.launch {
                        var addedRemoteId: FfiRemoteUuid? = null
                        try {
                            val remoteId = repo.addRemoteDebugLocalAndroid(name)
                            addedRemoteId = remoteId
                            repo.initializeRemote(remoteId, null)
                            onDismiss()
                            onResult(name, null)
                        } catch (e: Exception) {
                            addedRemoteId?.let { remoteId ->
                                runCatching { repo.removeRemote(remoteId) }
                            }
                            addError = e.message?.ifBlank { null } ?: "Failed to add remote"
                        }
                        submitting = false
                    }
                },
            )
        }
    }
}
