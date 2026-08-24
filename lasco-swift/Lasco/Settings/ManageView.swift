import SwiftUI

struct ManageView: View {
    @Environment(LibraryDirectoryModel.self) private var directory
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @State private var showGlobalSettings = false
    @State private var showOperations = false
    @State private var showDeleteConfirm = false
    @State private var showLicense = false
    @State private var cloudConnected = false
    @AppStorage("expertMode") private var expertMode = false
    let repository: LibraryRepository
    let session: LibrarySessionState
    let syncCoordinator: SyncCoordinator

    init(repository: LibraryRepository, session: LibrarySessionState, syncCoordinator: SyncCoordinator) {
        self.repository = repository
        self.session = session
        self.syncCoordinator = syncCoordinator
    }

    var body: some View {
        NavigationStack {
            ZStack {
                theme.bg.ignoresSafeArea()

                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("MANAGE")
                                .font(LascoFont.categoryLarge())
                                .foregroundStyle(theme.ink)
                            Text(session.nickname)
                                .font(LascoFont.subtitle())
                                .foregroundStyle(theme.inkMuted)
                        }
                        .padding(.top, 20)

                        VStack(alignment: .leading, spacing: 0) {
                            NavigationLink {
                                RemotesView(repository: repository, session: session, syncCoordinator: syncCoordinator)
                                    .navigationBarBackButtonHidden(true)
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

                            NavigationLink {
                                UsersView(repository: repository, session: session)
                                    .navigationBarBackButtonHidden(true)
                            } label: {
                                HStack {
                                    Text("Users")
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

                            Divider()
                                .background(theme.inkMuted.opacity(0.2))

                            Button {
                                Task { await directory.signOut() }
                            } label: {
                                HStack {
                                    Text("Sign out current library")
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

                        if cloudConnected {
                            NavigationLink {
                                LascoCloudView(repository: repository, libraryID: session.libraryID) {
                                    cloudConnected = false
                                }
                                .navigationBarBackButtonHidden(true)
                            } label: {
                                HStack {
                                    Text("Lasco Cloud")
                                        .font(LascoFont.body())
                                        .foregroundStyle(theme.accent)
                                    Spacer()
                                    Text("→")
                                        .font(LascoFont.mono())
                                        .foregroundStyle(theme.accent)
                                }
                                .padding(.horizontal, 16)
                                .padding(.vertical, 14)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .background(theme.pink)
                        }

                        VStack(alignment: .leading, spacing: 0) {
                            #if canImport(UIKit)
                            Divider()
                                .background(theme.inkMuted.opacity(0.2))

                            Toggle(isOn: Binding(
                                get: { session.autoImportDeviceMedia },
                                set: { enabled in
                                    Task { try? await repository.setAutoImportDeviceMedia(enabled: enabled) }
                                }
                            )) {
                                Text("Auto-import device media")
                                    .font(LascoFont.body())
                                    .foregroundStyle(theme.inkSub)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 14)
                            #endif
                        }
                        .lascoPanel()

                        if expertMode {
                            VStack(alignment: .leading, spacing: 0) {
                                Button {
                                    showOperations = true
                                } label: {
                                    HStack {
                                        Text("Operations")
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
                        }

                        VStack(alignment: .leading, spacing: 0) {
                            Button {
                                showLicense = true
                            } label: {
                                HStack {
                                    Text("Licenses")
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

                            Link(destination: URL(string: "https://getlasco.app/privacy-policy")!) {
                                HStack {
                                    Text("Privacy Policy")
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

                        VStack(alignment: .leading, spacing: 0) {
                            Button(role: .destructive) {
                                showDeleteConfirm = true
                            } label: {
                                HStack {
                                    Text("Delete library")
                                        .font(LascoFont.body())
                                        .foregroundStyle(Color.red)
                                    Spacer()
                                    Text("→")
                                        .font(LascoFont.mono())
                                        .foregroundStyle(Color.red)
                                }
                                .padding(.horizontal, 16)
                                .padding(.vertical, 14)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                        .lascoPanel()

                        Spacer(minLength: 40)
                    }
                    .padding(.horizontal, 20)
                }
                .background(theme.bg)
                .scrollContentBackground(.hidden)
                .navigationTitle("")
                .hideSystemNavigationBar()
            }
            .task(id: session.remotes.count) {
                cloudConnected = await repository.isLascoCloudConnected(libraryID: session.libraryID)
            }
        }
        .background(theme.bg)
        .sheet(isPresented: $showGlobalSettings) {
            SettingsView()
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showOperations) {
            OperationsView(repository: repository)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .confirmationDialog(
            "Delete \(session.nickname)?",
            isPresented: $showDeleteConfirm,
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                let nickname = session.nickname
                Task {
                    if await directory.deleteCurrentLibrary() {
                    toastManager.show(ok: "Deleted \(nickname)")
                    } else {
                        toastManager.show(error: "Failed to delete library")
                    }
                }
            }
            Button("Cancel", role: .cancel) { }
        } message: {
            Text("This removes all local data and unregisters the library from this device. Remote storage is not touched.")
        }
        .sheet(isPresented: $showLicense) {
            LicenseView()
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
    }
}
