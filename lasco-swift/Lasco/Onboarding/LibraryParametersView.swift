import SwiftUI

struct LibraryParametersView: View {
    @EnvironmentObject var libraryModel: LibraryModel
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

                    if let nickname = libraryModel.openNickname {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(nickname)
                                .font(LascoFont.categoryLarge())
                                .foregroundStyle(theme.ink)
                            if let id = libraryModel.openLibraryId {
                                Text(id)
                                    .font(LascoFont.mono())
                                    .foregroundStyle(theme.inkMuted)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                        }
                        .padding(.horizontal, 32)
                        .padding(.bottom, 32)
                    }

                    VStack(alignment: .leading, spacing: 0) {
                        NavigationLink {
                            RemotesView()
                                .environmentObject(libraryModel)
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
