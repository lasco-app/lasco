package com.lasco.lasco.ui.status

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.lasco.lasco.data.ConfirmMediaResult
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.components.LascoSecondaryButton
import com.lasco.lasco.ui.theme.lascoPanel
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiRemote
import uniffi.lasco_ffi.FfiRemoteUuid

/** One remote offered as a place the media might turn out to be, and the part it plays. */
data class MediaAtRiskRemote(val remote: FfiRemote, val role: String)

/**
 * Shown when an action is about to go ahead on media this client cannot account for.
 *
 * This client answers from the media list it cached for each remote, so media it cannot place
 * is not necessarily lost, a list may simply be out of date. Updating a remote refreshes its
 * list without fetching, which is why every remote that could hold the media gets its own
 * button, and why the action is offered again afterwards.
 */
@Composable
fun MediaAtRiskDialog(
    title: String,
    message: String,
    remotes: List<MediaAtRiskRemote>,
    retryLabel: String,
    cancelLabel: String,
    onConfirm: suspend (FfiRemoteUuid) -> ConfirmMediaResult,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
) {
    val colors = LascoTheme.colors
    val scope = rememberCoroutineScope()
    val outcomes = remember { mutableStateMapOf<String, String>() }
    var busy by remember { mutableStateOf(false) }

    Dialog(onDismissRequest = onCancel, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Column(modifier = Modifier.fillMaxSize().background(colors.bg)) {
            Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 24.dp)) {
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "✕",
                    style = LascoTheme.type.body(18),
                    color = colors.ink,
                    modifier = Modifier.clickable { onCancel() },
                )
            }
            Column(modifier = Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 32.dp)) {
                Text(
                    text = title,
                    style = LascoTheme.type.categoryLarge(),
                    color = colors.ink,
                )
                Text(
                    text = message,
                    style = LascoTheme.type.subtitle(),
                    color = colors.inkMuted,
                    modifier = Modifier.padding(top = 8.dp, bottom = 24.dp),
                )

                remotes.forEachIndexed { index, entry ->
                    if (index > 0) Spacer(modifier = Modifier.height(12.dp))
                    RemoteRow(entry.remote, entry.role, outcomes, busy) { remoteId ->
                        busy = true
                        scope.launch {
                            outcomes[remoteId.value] = describe(onConfirm(remoteId))
                            busy = false
                        }
                    }
                }
            }
            Column(modifier = Modifier.padding(32.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                LascoPrimaryButton(text = retryLabel, onClick = onRetry, enabled = !busy)
                LascoSecondaryButton(text = cancelLabel, onClick = onCancel)
            }
        }
    }
}

@Composable
private fun RemoteRow(
    remote: FfiRemote,
    role: String,
    outcomes: Map<String, String>,
    busy: Boolean,
    onConfirm: (FfiRemoteUuid) -> Unit,
) {
    val colors = LascoTheme.colors
    Row(
        modifier = Modifier.fillMaxWidth().lascoPanel().padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = remote.name,
                style = LascoTheme.type.body(),
                color = colors.ink,
            )
            Text(
                text = outcomes[remote.remoteId.value] ?: role,
                style = LascoTheme.type.pixel(),
                color = colors.inkMuted,
            )
        }
        LascoSecondaryButton(
            text = "Update list",
            onClick = { onConfirm(remote.remoteId) },
            enabled = !busy,
            fillWidth = false,
        )
    }
}

private fun describe(result: ConfirmMediaResult): String = when (result) {
    is ConfirmMediaResult.Confirmed ->
        if (result.newlyConfirmed == 0uL) "Nothing new found" else "${result.newlyConfirmed} newly found"
    is ConfirmMediaResult.Failed -> result.message
}
