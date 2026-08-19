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

/**
 * Shown when push preparation found no place to get some media from.
 *
 * Preparation reads only what this client has recorded, so the media is not necessarily lost:
 * a remote's media list may simply be out of date. Updating a remote's list refreshes it
 * without fetching, which is why every remote that could hold the media gets its own button.
 */
@Composable
fun PushBlockedDialog(
    targetRemote: FfiRemote,
    sourceRemotes: List<FfiRemote>,
    mediaCount: Int,
    onConfirm: suspend (FfiRemoteUuid) -> ConfirmMediaResult,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
) {
    val colors = LascoTheme.colors
    val scope = rememberCoroutineScope()
    val outcomes = remember { mutableStateMapOf<String, String>() }
    var busy by remember { mutableStateOf(false) }
    val mediaLabel = if (mediaCount == 1) "1 media has" else "$mediaCount media have"

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
                    text = "Push blocked",
                    style = LascoTheme.type.categoryLarge(),
                    color = colors.ink,
                )
                Text(
                    text = "$mediaLabel no known place to be copied from. A remote's media " +
                        "list may just be out of date. Update the ones you want, then push again.",
                    style = LascoTheme.type.subtitle(),
                    color = colors.inkMuted,
                    modifier = Modifier.padding(top = 8.dp, bottom = 24.dp),
                )

                RemoteRow(targetRemote, "Destination", outcomes, busy) { remoteId ->
                    busy = true
                    scope.launch {
                        outcomes[remoteId.value] = describe(onConfirm(remoteId))
                        busy = false
                    }
                }
                sourceRemotes.forEach { remote ->
                    Spacer(modifier = Modifier.height(12.dp))
                    RemoteRow(remote, "Source", outcomes, busy) { remoteId ->
                        busy = true
                        scope.launch {
                            outcomes[remoteId.value] = describe(onConfirm(remoteId))
                            busy = false
                        }
                    }
                }
            }
            Column(modifier = Modifier.padding(32.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                LascoPrimaryButton(text = "Push again", onClick = onRetry, enabled = !busy)
                LascoSecondaryButton(text = "Cancel push", onClick = onCancel)
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
