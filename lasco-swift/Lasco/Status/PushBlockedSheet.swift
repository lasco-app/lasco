import SwiftUI

/// Shown when push preparation found no place to get some media from.
///
/// Preparation reads only what this client has recorded, so the media is not necessarily lost:
/// a remote's media list may simply be out of date.
struct PushBlockedSheet: View {
    let targetRemote: FfiRemote
    let sourceRemotes: [FfiRemote]
    let mediaCount: Int
    let onConfirm: (FfiRemoteUuid) async -> ConfirmMediaResult
    let onRetry: () -> Void
    let onCancel: () -> Void

    private var mediaLabel: String {
        mediaCount == 1 ? "1 media has" : "\(mediaCount) media have"
    }

    var body: some View {
        MediaAtRiskSheet(
            title: "Push blocked",
            message: "\(mediaLabel) no known place to be copied from. A remote's media list may just be out of date. Update the ones you want, then push again.",
            remotes: [MediaAtRiskRemote(remote: targetRemote, role: "Destination")]
                + sourceRemotes.map { MediaAtRiskRemote(remote: $0, role: "Source") },
            retryLabel: "Push again",
            cancelLabel: "Cancel push",
            onConfirm: onConfirm,
            onRetry: onRetry,
            onCancel: onCancel
        )
    }
}
