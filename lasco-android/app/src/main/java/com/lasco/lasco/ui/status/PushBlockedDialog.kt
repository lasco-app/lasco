package com.lasco.lasco.ui.status

import androidx.compose.runtime.Composable
import com.lasco.lasco.data.ConfirmMediaResult
import uniffi.lasco_ffi.FfiRemote
import uniffi.lasco_ffi.FfiRemoteUuid

/**
 * Shown when push preparation found no place to get some media from.
 *
 * Preparation reads only what this client has recorded, so the media is not necessarily lost:
 * a remote's media list may simply be out of date.
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
    val mediaLabel = if (mediaCount == 1) "1 media has" else "$mediaCount media have"
    MediaAtRiskDialog(
        title = "Push blocked",
        message = "$mediaLabel no known place to be copied from. A remote's media " +
            "list may just be out of date. Update the ones you want, then push again.",
        remotes = listOf(MediaAtRiskRemote(targetRemote, "Destination")) +
            sourceRemotes.map { MediaAtRiskRemote(it, "Source") },
        retryLabel = "Push again",
        cancelLabel = "Cancel push",
        onConfirm = onConfirm,
        onRetry = onRetry,
        onCancel = onCancel,
    )
}
