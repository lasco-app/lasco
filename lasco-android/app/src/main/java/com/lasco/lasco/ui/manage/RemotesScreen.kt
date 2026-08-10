package com.lasco.lasco.ui.manage

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.material3.Switch
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.ui.components.LascoConfirmDialog
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiRemote

/**
 * Ported from Swift's RemotesView. Reads LibraryRepository.sync.syncState and
 * shares the add-remote dialogs from RemoteAddDialogs.kt.
 */
@Composable
fun RemotesScreen(
    onBack: () -> Unit,
    manageViewModel: ManageViewModel,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val prefs = remember { Prefs.from(context) }
    val repo = remember { LibraryRepository.from(context) }
    val expertMode by prefs.expertMode.collectAsStateWithLifecycle()

    val session by manageViewModel.sessionState.collectAsStateWithLifecycle()
    val syncState by manageViewModel.syncState.collectAsStateWithLifecycle()

    var showRemotePicker by remember { mutableStateOf(false) }
    var showAddS3 by remember { mutableStateOf(false) }
    var showAddLocalFS by remember { mutableStateOf(false) }
    var pendingDelete by remember { mutableStateOf<FfiRemote?>(null) }
    var feedback by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg)
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp),
    ) {
        Row(modifier = Modifier.fillMaxWidth().padding(top = 20.dp, bottom = 12.dp)) {
            Text(
                text = "← Manage",
                style = LascoTheme.type.body(),
                color = colors.inkMuted,
                modifier = Modifier.clickable { onBack() },
            )
        }
        Text(text = "REMOTES", style = LascoTheme.type.categoryLarge(), color = colors.ink)
        Spacer(modifier = Modifier.height(16.dp))

        feedback?.let {
            Text(
                text = it,
                style = LascoTheme.type.body(13),
                color = colors.inkMuted,
                modifier = Modifier.padding(bottom = 8.dp),
            )
        }

        if (session.remotes.isEmpty()) {
            Text(
                text = "No remotes configured.",
                style = LascoTheme.type.body(),
                color = colors.inkMuted,
                modifier = Modifier.fillMaxWidth().lascoPanel().padding(horizontal = 16.dp, vertical = 14.dp),
            )
        } else {
            session.remotes.forEach { remote ->
                RemoteCard(
                    remote = remote,
                    isDefaultFetch = remote.remoteId == session.defaultFetchRemoteId,
                    busy = remote.remoteId in syncState.busyRemoteIds,
                    onTestConnection = {
                        scope.launch {
                            val ok = repo.connectRemote(remote.remoteId, null)
                            feedback = if (ok) "${remote.name}: connection succeeded" else "${remote.name}: connection failed"
                        }
                    },
                    onSetDefaultFetch = {
                        scope.launch {
                            repo.setDefaultFetchRemote(remote.remoteId)
                            feedback = "${remote.name}: set as default fetch"
                        }
                    },
                    onSetAutoPush = { enabled -> manageViewModel.setRemoteAutoPush(remote.remoteId, enabled) },
                    onDelete = { pendingDelete = remote },
                )
                Spacer(modifier = Modifier.height(12.dp))
            }
        }

        Spacer(modifier = Modifier.height(12.dp))
        LascoPrimaryButton(text = "Add remote", onClick = { showRemotePicker = true })
        Spacer(modifier = Modifier.height(100.dp))
    }

    if (showRemotePicker) {
        RemoteTypePickerDialog(
            expertMode = expertMode,
            onS3 = { showRemotePicker = false; showAddS3 = true },
            onLocalFS = { showRemotePicker = false; showAddLocalFS = true },
            onDismiss = { showRemotePicker = false },
        )
    }
    if (showAddS3) {
        AddS3RemoteDialog(
            onDismiss = { showAddS3 = false },
            onResult = { name, error -> feedback = error ?: "$name: initialized" },
        )
    }
    if (showAddLocalFS) {
        AddLocalFSRemoteDialog(
            onDismiss = { showAddLocalFS = false },
            onResult = { name, error -> feedback = error ?: "$name: initialized" },
        )
    }
    pendingDelete?.let { remote ->
        LascoConfirmDialog(
            title = "Delete remote",
            message = "This removes \"${remote.name}\" from this library. Data already pushed to it is not deleted.",
            onConfirm = {
                scope.launch {
                    repo.removeRemote(remote.remoteId)
                    feedback = "${remote.name}: removed"
                }
                pendingDelete = null
            },
            onCancel = { pendingDelete = null },
        )
    }
}

@Composable
private fun RemoteCard(
    remote: FfiRemote,
    isDefaultFetch: Boolean,
    busy: Boolean,
    onTestConnection: () -> Unit,
    onSetDefaultFetch: () -> Unit,
    onSetAutoPush: (Boolean) -> Unit,
    onDelete: () -> Unit,
) {
    val colors = LascoTheme.colors
    val summary = remote.bucket?.let { bucket ->
        "${remote.endpoint.orEmpty()} / $bucket"
    } ?: remote.path.orEmpty()

    Column(modifier = Modifier.fillMaxWidth().lascoPanel().padding(horizontal = 16.dp, vertical = 12.dp)) {
        Row {
            Text(text = remote.name, style = LascoTheme.type.body(), color = colors.ink)
            Spacer(modifier = Modifier.width(8.dp))
            Text(text = remote.kind, style = LascoTheme.type.mono(), color = colors.inkMuted)
            if (isDefaultFetch) {
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = "default fetch",
                    style = LascoTheme.type.mono(10),
                    color = colors.pink,
                    modifier = Modifier.background(colors.pink.copy(alpha = 0.12f)).padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
        }
        if (summary.isNotBlank()) {
            Text(text = summary, style = LascoTheme.type.mono(11), color = colors.inkMuted)
        }
        Row {
            Text(text = "Auto push", style = LascoTheme.type.body(13), color = colors.ink)
            Spacer(modifier = Modifier.width(8.dp))
            Switch(checked = remote.autoPush, onCheckedChange = onSetAutoPush)
        }
        Spacer(modifier = Modifier.height(10.dp))
        Row {
            Text(
                text = if (busy) "Testing…" else "Test connection",
                style = LascoTheme.type.body(13),
                color = if (busy) colors.inkMuted else colors.ink,
                modifier = Modifier.clickable(enabled = !busy) { onTestConnection() },
            )
            Spacer(modifier = Modifier.width(20.dp))
            if (!isDefaultFetch) {
                Text(
                    text = "Set as default fetch",
                    style = LascoTheme.type.body(13),
                    color = colors.ink,
                    modifier = Modifier.clickable { onSetDefaultFetch() },
                )
                Spacer(modifier = Modifier.width(20.dp))
            }
            Text(
                text = "Delete",
                style = LascoTheme.type.body(13),
                color = colors.error,
                modifier = Modifier.clickable { onDelete() },
            )
        }
    }
}
