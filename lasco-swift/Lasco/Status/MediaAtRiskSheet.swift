import SwiftUI

/// One remote offered as a place the media might turn out to be, with the part it plays in
/// the situation being explained.
struct MediaAtRiskRemote: Identifiable {
    let remote: FfiRemote
    let role: String

    var id: FfiRemoteUuid { remote.remoteId }
}

/// Shown when an action is about to go ahead on media this client cannot account for.
///
/// This client answers from the media list it cached for each remote, so media it cannot place
/// is not necessarily lost, a list may simply be out of date. Updating a remote refreshes its
/// list without fetching, which is why every remote that could hold the media gets its own
/// button, and why the action is offered again afterwards.
struct MediaAtRiskSheet: View {
    let title: String
    let message: String
    let remotes: [MediaAtRiskRemote]
    let retryLabel: String
    let cancelLabel: String
    let onConfirm: (FfiRemoteUuid) async -> ConfirmMediaResult
    let onRetry: () -> Void
    let onCancel: () -> Void

    @Environment(\.lascoTheme) var theme
    @State private var outcomes: [FfiRemoteUuid: String] = [:]
    @State private var busyRemote: FfiRemoteUuid?

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text(title)
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                    Spacer()
                    Button(action: onCancel) {
                        Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .foregroundStyle(theme.inkMuted)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 8)

                Text(message)
                    .font(LascoFont.subtitle())
                    .foregroundStyle(theme.inkMuted)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 24)

                ScrollView {
                    VStack(spacing: 12) {
                        ForEach(remotes) { entry in
                            remoteRow(entry.remote, role: entry.role)
                        }
                    }
                    .padding(.horizontal, 32)
                }

                VStack(spacing: 12) {
                    Button(retryLabel, action: onRetry)
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                        .disabled(busyRemote != nil)

                    Button(cancelLabel, action: onCancel)
                        .buttonStyle(LascoSecondaryButtonStyle())
                        .frame(maxWidth: .infinity)
                }
                .padding(.horizontal, 32)
                .padding(.top, 24)
                .padding(.bottom, 32)
            }
        }
    }

    private func remoteRow(_ remote: FfiRemote, role: String) -> some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(remote.name)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                Text(outcomes[remote.remoteId] ?? role)
                    .font(LascoFont.pixel())
                    .foregroundStyle(theme.inkMuted)
            }
            Spacer()
            Button("Update list") {
                Task {
                    busyRemote = remote.remoteId
                    switch await onConfirm(remote.remoteId) {
                    case .confirmed(let count):
                        outcomes[remote.remoteId] = count == 0
                            ? "Nothing new found"
                            : "\(count) newly found"
                    case .failed(let message):
                        outcomes[remote.remoteId] = message
                    }
                    busyRemote = nil
                }
            }
            .buttonStyle(LascoSecondaryButtonStyle())
            .disabled(busyRemote != nil)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .lascoPanel()
    }
}
