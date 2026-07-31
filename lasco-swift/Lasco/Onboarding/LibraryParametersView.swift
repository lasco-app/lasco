import SwiftUI

struct LibraryParametersView: View {
    @Environment(LibraryDirectoryModel.self) private var directory
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme

    @State private var showGlobalSettings = false

    var body: some View {
        NavigationStack {
            ZStack {
                theme.bg.ignoresSafeArea()

                VStack(alignment: .leading, spacing: 0) {
                    HStack {
                        Text("Library")
                            .font(LascoFont.title())
                            .foregroundStyle(theme.ink)
                        Spacer()
                        Button("Done") { dismiss() }
                            .font(LascoFont.body(14))
                            .foregroundStyle(theme.inkMuted)
                            .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 32)
                    .padding(.top, 40)
                    .padding(.bottom, 8)

                    if let activeSession = directory.activeSession {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(activeSession.state.nickname)
                                .font(LascoFont.categoryLarge())
                                .foregroundStyle(theme.ink)
                            Text(activeSession.state.libraryID)
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.inkMuted)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        .padding(.horizontal, 32)
                        .padding(.bottom, 32)
                    }

                    VStack(alignment: .leading, spacing: 0) {
                        NavigationLink {
                            if let activeSession = directory.activeSession {
                                RemotesView(repository: activeSession.repository, session: activeSession.state)
                            }
                        } label: {
                            HStack {
                                Text("Remotes")
                                    .font(LascoFont.body())
                                    .foregroundStyle(theme.inkSub)
                                Spacer()
                                Text("→")
                                    .font(LascoFont.mono())
                                    .foregroundStyle(theme.inkMuted)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 14)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)

                        Divider()
                            .background(theme.inkMuted.opacity(0.2))

                        Button {
                            showGlobalSettings = true
                        } label: {
                            HStack {
                                Text("Global settings")
                                    .font(LascoFont.body())
                                    .foregroundStyle(theme.inkSub)
                                Spacer()
                                Text("→")
                                    .font(LascoFont.mono())
                                    .foregroundStyle(theme.inkMuted)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 14)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                    .lascoPanel()
                    .padding(.horizontal, 32)

                    Spacer()
                }
            }
        }
        .sheet(isPresented: $showGlobalSettings) {
            SettingsView()
        }
    }
}
