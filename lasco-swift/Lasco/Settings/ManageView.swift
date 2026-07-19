import SwiftUI

struct ManageView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @State private var showGlobalSettings = false
    @State private var showOperations = false
    @State private var showDefaultAlbumPicker = false
    @State private var showDeleteConfirm = false
    @State private var showLicense = false
    @AppStorage("expertMode") private var expertMode = false

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
                            if let nickname = libraryModel.openNickname {
                                Text(nickname)
                                    .font(LascoFont.subtitle())
                                    .foregroundStyle(theme.inkMuted)
                            }
                        }
                        .padding(.top, 20)

                        VStack(alignment: .leading, spacing: 0) {
                            NavigationLink {
                                RemotesView()
                                    .environmentObject(libraryModel)
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
                                UsersView()
                                    .environmentObject(libraryModel)
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
                                libraryModel.signOut()
                            } label: {
                                HStack {
                                    Text("Sign out")
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
                            Button {
                                showDefaultAlbumPicker = true
                            } label: {
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text("Default import album")
                                            .font(LascoFont.body())
                                            .foregroundStyle(theme.inkSub)
                                        if let album = libraryModel.defaultUploadAlbum {
                                            Text(album.name)
                                                .font(LascoFont.pixel())
                                                .foregroundStyle(theme.inkMuted)
                                        } else {
                                            Text("No default album for import set.")
                                                .font(LascoFont.pixel())
                                                .foregroundStyle(theme.inkMuted)
                                        }
                                    }
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

                            #if canImport(UIKit)
                            Divider()
                                .background(theme.inkMuted.opacity(0.2))

                            Toggle(isOn: Binding(
                                get: { libraryModel.autoImportDeviceMedia ?? false },
                                set: { libraryModel.setAutoImportDeviceMedia($0) }
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
        }
        .background(theme.bg)
        .sheet(isPresented: $showGlobalSettings) {
            SettingsView()
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showOperations) {
            OperationsView()
                .environmentObject(libraryModel)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .confirmationDialog(
            "Delete \(libraryModel.openNickname ?? "library")?",
            isPresented: $showDeleteConfirm,
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                let nickname = libraryModel.openNickname ?? "library"
                if libraryModel.deleteCurrentLibrary() {
                    toastManager.show(ok: "Deleted \(nickname)")
                } else {
                    toastManager.show(error: libraryModel.error ?? "Failed to delete library")
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
        .sheet(isPresented: $showDefaultAlbumPicker) {
            AlbumPickerView(title: "Default import album") { album in
                libraryModel.setDefaultUploadAlbum(albumId: album.albumId)
                showDefaultAlbumPicker = false
            } onCancel: {
                showDefaultAlbumPicker = false
            }
            .environmentObject(libraryModel)
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
    }
}
