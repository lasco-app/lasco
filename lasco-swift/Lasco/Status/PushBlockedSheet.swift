import SwiftUI

/// Shown when push preparation found no place to get some media from.
///
/// Preparation reads only what this client has recorded, so the media is not necessarily lost:
/// a remote's media list may simply be out of date. Confirming a remote refreshes that list
/// without fetching, which is why every remote that could hold the media gets its own button.
struct PushBlockedSheet: View {
    let targetRemote: FfiRemote
    let sourceRemotes: [FfiRemote]
    let mediaCount: Int
    let onConfirm: (FfiRemoteUuid) async -> ConfirmMediaResult
    let onRetry: () -> Void
    let onCancel: () -> Void

    @Environment(\.lascoTheme) var theme
    @State private var outcomes: [FfiRemoteUuid: String] = [:]
    @State private var busyRemote: FfiRemoteUuid?

    private var mediaLabel: String {
        mediaCount == 1 ? "1 media has" : "\(mediaCount) media have"
    }

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text("Push blocked")
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

                Text("\(mediaLabel) no known place to be copied from. A remote's media list may just be out of date. Update the ones you want, then push again.")
                    .font(LascoFont.subtitle())
                    .foregroundStyle(theme.inkMuted)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 24)

                ScrollView {
                    VStack(spacing: 12) {
                        remoteRow(targetRemote, role: "Destination")
                        ForEach(sourceRemotes, id: \.remoteId) { remote in
                            remoteRow(remote, role: "Source")
                        }
                    }
                    .padding(.horizontal, 32)
                }

                VStack(spacing: 12) {
                    Button("Push again", action: onRetry)
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                        .disabled(busyRemote != nil)

                    Button("Cancel push", action: onCancel)
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
