package com.lasco.lasco.ui.status

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.data.SyncViewModel
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.manage.AddLocalFSRemoteDialog
import com.lasco.lasco.ui.manage.AddS3RemoteDialog
import com.lasco.lasco.ui.manage.RemoteTypePickerDialog
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiRemote
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale

/**
 * Status tab, mirroring Swift's StatusView minus the local cached-media/
 * thumbnail section and the auto-push countdown, both out of scope.
 */
@Composable
fun StatusScreen(modifier: Modifier = Modifier) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val prefs = remember { Prefs.from(context) }
    val expertMode by prefs.expertMode.collectAsStateWithLifecycle()

    val statusViewModel: StatusViewModel = viewModel(factory = StatusViewModel.Factory)
    val syncViewModel: SyncViewModel = viewModel(factory = SyncViewModel.Factory)

    val media by statusViewModel.media.collectAsStateWithLifecycle()
    val session by statusViewModel.sessionState.collectAsStateWithLifecycle()
    val unpushed by statusViewModel.unpushed.collectAsStateWithLifecycle()
    val syncState by syncViewModel.syncState.collectAsStateWithLifecycle()

    var showRemotePicker by remember { mutableStateOf(false) }
    var showAddS3 by remember { mutableStateOf(false) }
    var showAddLocalFS by remember { mutableStateOf(false) }
    var feedback by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    Box(modifier = modifier.fillMaxSize().background(colors.bg)) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            Column(modifier = Modifier.padding(top = 20.dp, bottom = 4.dp)) {
                Text(text = "STATUS", style = LascoTheme.type.categoryLarge(), color = colors.ink)
                Text(text = session.nickname, style = LascoTheme.type.subtitle(), color = colors.inkMuted)
            }

            Spacer(modifier = Modifier.height(20.dp))

            val counts = media.mediaTypeCounts()
            Row(
                modifier = Modifier.fillMaxWidth().lascoPanel().padding(horizontal = 16.dp, vertical = 12.dp),
            ) {
                Text(text = "Library", style = LascoTheme.type.body(), color = colors.inkSub)
                Spacer(modifier = Modifier.weight(1f))
                Column(horizontalAlignment = Alignment.End) {
                    Text(text = "${media.size} items", style = LascoTheme.type.mono(), color = colors.ink)
                    Text(
                        text = "${counts.photos} photos  ${counts.videos} videos",
                        style = LascoTheme.type.mono(),
                        color = colors.inkMuted,
                    )
                }
            }

            Spacer(modifier = Modifier.height(24.dp))

            Text(text = "REMOTES", style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Spacer(modifier = Modifier.height(12.dp))

            feedback?.let {
                Text(text = it, style = LascoTheme.type.body(13), color = colors.inkMuted, modifier = Modifier.padding(bottom = 8.dp))
            }

            if (session.remotes.isEmpty()) {
                Text(
                    text = "No remotes configured.",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                    modifier = Modifier.fillMaxWidth().lascoPanel().padding(horizontal = 16.dp, vertical = 14.dp),
                )
                Spacer(modifier = Modifier.height(12.dp))
                LascoPrimaryButton(text = "Add remote", onClick = { showRemotePicker = true })
            } else {
                session.remotes.forEach { remote ->
                    RemoteStatusCard(
                        remote = remote,
                        isDefaultFetch = remote.id == session.defaultFetchRemoteId,
                        isSynced = unpushed[remote.id] != true,
                        lastPush = prefs.lastPush(remote.id),
                        lastFetch = prefs.lastFetch(remote.id),
                        pushEnabled = remote.id !in syncState.busyRemoteIds,
                        fetchEnabled = remote.id !in syncState.busyRemoteIds && !syncState.fetchInProgress,
                        onPush = {
                            scope.launch {
                                val err = syncViewModel.pushRemote(remote.id)
                                statusViewModel.refreshRemote(remote.id)
                                feedback = err ?: "${remote.name}: pushed"
                            }
                        },
                        onFetch = {
                            scope.launch {
                                val err = syncViewModel.fetchRemoteWithResult(remote.id)
                                statusViewModel.refreshRemote(remote.id)
                                feedback = err ?: "${remote.name}: fetched"
                            }
                        },
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                }
            }

            Spacer(modifier = Modifier.height(100.dp))
        }
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
}

private fun syncLabel(epochMillis: Long): String {
    val date = Date(epochMillis)
    val today = Calendar.getInstance()
    val target = Calendar.getInstance().apply { time = date }
    val time = SimpleDateFormat("HH:mm", Locale.getDefault()).format(date)
    return if (today.get(Calendar.YEAR) == target.get(Calendar.YEAR) && today.get(Calendar.DAY_OF_YEAR) == target.get(Calendar.DAY_OF_YEAR)) {
        "today $time"
    } else {
        "${SimpleDateFormat("MMM d", Locale.getDefault()).format(date)} $time"
    }
}

@Composable
private fun RemoteStatusCard(
    remote: FfiRemote,
    isDefaultFetch: Boolean,
    isSynced: Boolean,
    lastPush: com.lasco.lasco.data.SyncRecord?,
    lastFetch: com.lasco.lasco.data.SyncRecord?,
    pushEnabled: Boolean,
    fetchEnabled: Boolean,
    onPush: () -> Unit,
    onFetch: () -> Unit,
) {
    val colors = LascoTheme.colors
    Column(modifier = Modifier.fillMaxWidth().lascoPanel()) {
        Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Row {
                Text(text = remote.name, style = LascoTheme.type.body(), color = colors.ink)
                Spacer(modifier = Modifier.width(8.dp))
                Text(text = remote.kind, style = LascoTheme.type.mono(), color = colors.inkMuted)
            }
            Text(text = remote.id, style = LascoTheme.type.mono(10), color = colors.inkMuted)
        }
        Text(
            text = if (isSynced) "all local changes pushed" else "local changes not pushed",
            style = LascoTheme.type.mono(),
            color = Color.White,
            modifier = Modifier
                .fillMaxWidth()
                .background(if (isSynced) colors.pink else colors.error)
                .padding(horizontal = 16.dp, vertical = 10.dp),
        )
        SyncStatusRow(
            label = "Push",
            failed = lastPush?.success == false,
            dateLabel = lastPush?.let { syncLabel(it.epochMillis) } ?: "never",
            enabled = pushEnabled,
            onClick = onPush,
        )
        SyncStatusRow(
            label = "Fetch",
            failed = lastFetch?.success == false,
            dateLabel = lastFetch?.let { syncLabel(it.epochMillis) } ?: "never",
            isDefaultFetch = isDefaultFetch,
            enabled = fetchEnabled,
            onClick = onFetch,
        )
    }
}

@Composable
private fun SyncStatusRow(
    label: String,
    failed: Boolean,
    dateLabel: String,
    isDefaultFetch: Boolean = false,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    val colors = LascoTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, indication = null, interactionSource = null) { onClick() }
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (failed) {
            Box(modifier = Modifier.height(7.dp).width(7.dp).background(colors.error))
            Spacer(modifier = Modifier.width(8.dp))
        }
        Text(text = label, style = LascoTheme.type.body(), color = colors.inkSub)
        if (isDefaultFetch) {
            Spacer(modifier = Modifier.width(8.dp))
            Text(
                text = "default fetch",
                style = LascoTheme.type.mono(10),
                color = colors.pink,
                modifier = Modifier.background(colors.pink.copy(alpha = 0.12f)).padding(horizontal = 6.dp, vertical = 2.dp),
            )
        }
        Spacer(modifier = Modifier.weight(1f))
        Text(text = dateLabel, style = LascoTheme.type.mono(), color = colors.inkMuted)
    }
}
