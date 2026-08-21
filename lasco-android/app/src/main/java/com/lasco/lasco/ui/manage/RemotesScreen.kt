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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.ui.components.LascoConfirmDialog
import com.lasco.lasco.ui.components.LascoInfoDialog
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.components.LascoSecondaryButton
import com.lasco.lasco.ui.status.MediaAtRiskDialog
import com.lasco.lasco.ui.status.MediaAtRiskRemote
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiRemote
import uniffi.lasco_ffi.LascoException
import uniffi.lasco_ffi.FfiCompactionLockInfo

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
    val cloudConnected = manageViewModel.isLascoCloudConnected()

    var showRemotePicker by remember { mutableStateOf(false) }
    var showAddS3 by remember { mutableStateOf(false) }
    var showAddLocalFS by remember { mutableStateOf(false) }
    var showCloudLogin by remember { mutableStateOf(false) }
    var pendingDelete by remember { mutableStateOf<FfiRemote?>(null) }
    var pendingLockRemoval by remember { mutableStateOf<FfiRemote?>(null) }
    var removalBlocked by remember { mutableStateOf<RemoteRemovalBlocked?>(null) }
    var removalBlockedByScheduledPush by remember { mutableStateOf<FfiRemote?>(null) }
    var removalFailed by remember { mutableStateOf<String?>(null) }
    var feedback by remember { mutableStateOf<String?>(null) }
    var isUpdatingMediaSourceOrder by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val remotesById = session.remotes.associateBy { it.remoteId }
    val orderedMediaSources = session.mediaSourceOrder.mapNotNull(remotesById::get)

    // Removes a remote unless this client cannot account for media once it is gone, in which
    // case the dialog offers to refresh what the remaining remotes are known to hold first.
    suspend fun removeRemote(remote: FfiRemote) {
        // A scheduled push claims the remote when it fires, which would make the removal fail
        // on timing alone. Refusing up front says so while the countdown is still visible.
        if (remote.remoteId in syncState.scheduledAutoPushRemoteIds) {
            removalBlockedByScheduledPush = remote
            return
        }
        val lost = runCatching { repo.mediaCountLostIfRemoteRemoved(remote.remoteId) }.getOrDefault(0)
        if (lost > 0) {
            removalBlocked = RemoteRemovalBlocked(
                target = remote,
                others = session.remotes.filter { it.remoteId != remote.remoteId },
                mediaCount = lost,
            )
            return
        }
        // A push or fetch that claimed the remote between the scheduled push check and here
        // stops the removal, which leaves the configuration untouched. Saying so is the only
        // way the user learns the remote is still there.
        try {
            repo.removeRemote(remote.remoteId)
            feedback = "${remote.name}: removed"
        } catch (e: LascoException.SyncBusy) {
            removalFailed = "\"${remote.name}\" is syncing right now. Wait for it to finish, then remove it."
        } catch (e: LascoException) {
            removalFailed = "\"${remote.name}\" could not be removed: ${e.message ?: "unknown error"}"
        }
    }

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
                    onInspectCompactionLock = {
                        repo.inspectCompactionLock(remote.remoteId)
                    },
                    onRemoveOwnCompactionLock = { pendingLockRemoval = remote },
                )
                Spacer(modifier = Modifier.height(12.dp))
            }

            if (orderedMediaSources.size > 1) {
                DownloadPriorityPanel(
                    remotes = orderedMediaSources,
                    isUpdating = isUpdatingMediaSourceOrder,
                    onMove = { index, offset ->
                        val destination = index + offset
                        if (destination in orderedMediaSources.indices && !isUpdatingMediaSourceOrder) {
                            val reorderedIds = orderedMediaSources.map { it.remoteId }.toMutableList()
                            val source = reorderedIds[index]
                            reorderedIds[index] = reorderedIds[destination]
                            reorderedIds[destination] = source
                            isUpdatingMediaSourceOrder = true
                            scope.launch {
                                try {
                                    manageViewModel.setMediaSourceOrder(reorderedIds).await()
                                } catch (_: Exception) {
                                    feedback = "Could not update download priority"
                                } finally {
                                    isUpdatingMediaSourceOrder = false
                                }
                            }
                        }
                    },
                )
            }
        }

        Spacer(modifier = Modifier.height(12.dp))
        LascoPrimaryButton(text = "Add remote", onClick = { showRemotePicker = true })
        Spacer(modifier = Modifier.height(100.dp))
    }

    if (showRemotePicker) {
        RemoteTypePickerDialog(
            expertMode = expertMode,
            showCloud = !cloudConnected,
            onCloud = { showRemotePicker = false; showCloudLogin = true },
            onS3 = { showRemotePicker = false; showAddS3 = true },
            onLocalFS = { showRemotePicker = false; showAddLocalFS = true },
            onDismiss = { showRemotePicker = false },
        )
    }
    if (showCloudLogin) {
        LascoCloudLoginDialog(
            onDismiss = { showCloudLogin = false },
            onResult = { error -> feedback = error ?: "Lasco Cloud: connected" },
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
                scope.launch { removeRemote(remote) }
                pendingDelete = null
            },
            onCancel = { pendingDelete = null },
        )
    }
    removalFailed?.let { message ->
        LascoInfoDialog(
            title = "Remote not removed",
            message = message,
            onDismiss = { removalFailed = null },
        )
    }
    removalBlockedByScheduledPush?.let { remote ->
        LascoInfoDialog(
            title = "Push scheduled",
            message = "A push to \"${remote.name}\" is about to run. Let it finish, or turn off " +
                "Auto Push, then remove the remote.",
            onDismiss = { removalBlockedByScheduledPush = null },
        )
    }
    removalBlocked?.let { blocked ->
        val plural = if (blocked.mediaCount == 1) "" else "s"
        MediaAtRiskDialog(
            title = "Remove remote?",
            message = "${blocked.mediaCount} media$plural would probably be lost. This is based " +
                "on each remote's media list as of its last update, so they may already be " +
                "elsewhere. Update the lists you want, then try again.",
            remotes = blocked.others.map { MediaAtRiskRemote(it, "Could already hold them") },
            retryLabel = "Try again",
            cancelLabel = "Cancel",
            onConfirm = { repo.sync.confirmRemoteMedia(it) },
            onRetry = {
                val target = blocked.target
                removalBlocked = null
                scope.launch { removeRemote(target) }
            },
            onCancel = { removalBlocked = null },
        )
    }
    pendingLockRemoval?.let { remote ->
        LascoConfirmDialog(
            title = "Remove your compaction lock?",
            message = "Only remove this after confirming this device is no longer compacting the remote.",
            confirmLabel = "Remove lock",
            onConfirm = {
                scope.launch {
                    feedback = if (repo.removeOwnCompactionLock(remote.remoteId)) "${remote.name}: lock removed" else "${remote.name}: lock was not owned by this device"
                }
                pendingLockRemoval = null
            },
            onCancel = { pendingLockRemoval = null },
        )
    }
}

@Composable
private fun DownloadPriorityPanel(
    remotes: List<FfiRemote>,
    isUpdating: Boolean,
    onMove: (index: Int, offset: Int) -> Unit,
) {
    val colors = LascoTheme.colors

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .lascoPanel()
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Text(text = "DOWNLOAD PRIORITY", style = LascoTheme.type.mono(), color = colors.inkMuted)
        Spacer(modifier = Modifier.height(6.dp))
        Text(
            text = "Priority list to download media",
            style = LascoTheme.type.body(13),
            color = colors.inkMuted,
        )
        remotes.forEachIndexed { index, remote ->
            Spacer(modifier = Modifier.height(12.dp))
            DownloadPriorityRow(
                position = index + 1,
                remote = remote,
                canMoveEarlier = index > 0 && !isUpdating,
                canMoveLater = index < remotes.lastIndex && !isUpdating,
                onMoveEarlier = { onMove(index, -1) },
                onMoveLater = { onMove(index, 1) },
            )
        }
    }
}

@Composable
private fun DownloadPriorityRow(
    position: Int,
    remote: FfiRemote,
    canMoveEarlier: Boolean,
    canMoveLater: Boolean,
    onMoveEarlier: () -> Unit,
    onMoveLater: () -> Unit,
) {
    val colors = LascoTheme.colors

    Row(modifier = Modifier.fillMaxWidth()) {
        Text(
            text = position.toString(),
            style = LascoTheme.type.mono(),
            color = colors.pink,
            modifier = Modifier.width(24.dp),
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(text = remote.name, style = LascoTheme.type.body(), color = colors.ink)
            Text(text = remote.kind, style = LascoTheme.type.mono(11), color = colors.inkMuted)
        }
        Column {
            LascoSecondaryButton(
                text = "↑",
                onClick = onMoveEarlier,
                modifier = Modifier.semantics { contentDescription = "Move ${remote.name} earlier" },
                enabled = canMoveEarlier,
                fillWidth = false,
            )
            Spacer(modifier = Modifier.height(4.dp))
            LascoSecondaryButton(
                text = "↓",
                onClick = onMoveLater,
                modifier = Modifier.semantics { contentDescription = "Move ${remote.name} later" },
                enabled = canMoveLater,
                fillWidth = false,
            )
        }
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
    onInspectCompactionLock: suspend () -> FfiCompactionLockInfo?,
    onRemoveOwnCompactionLock: () -> Unit,
) {
    val colors = LascoTheme.colors
    val scope = rememberCoroutineScope()
    var lockInfo by remember(remote.remoteId) { mutableStateOf<FfiCompactionLockInfo?>(null) }
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
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            text = lockInfo?.let { "Compaction lock: ${it.ownerDeviceId.take(8)} since ${it.createdAt}" }
                ?: "Check compaction lock",
            style = LascoTheme.type.body(13),
            color = colors.ink,
            modifier = Modifier.clickable {
                scope.launch { lockInfo = onInspectCompactionLock() }
            },
        )
        if (lockInfo?.isOwnedByCurrentDevice == true) {
            Text(
                text = "Remove my compaction lock",
                style = LascoTheme.type.body(13),
                color = colors.inkMuted,
                modifier = Modifier.clickable { onRemoveOwnCompactionLock() },
            )
        }
    }
}

/**
 * The media that removing one remote would leave this client unable to place, along with the
 * remotes whose media list could still turn out to hold them.
 */
private data class RemoteRemovalBlocked(
    val target: FfiRemote,
    val others: List<FfiRemote>,
    val mediaCount: Int,
)
