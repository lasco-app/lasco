import SwiftUI

struct RemotesView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(ToastManager.self) var toastManager
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme

    @AppStorage("devMode") private var devMode = false

    @State private var showRemotePicker = false
    @State private var showAddS3 = false
    @State private var showAddLocalFS = false

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                #if canImport(UIKit)
                LascoBackButton(action: { dismiss() })
                    .padding(.horizontal, 32)
                    .padding(.top, 20)
                #endif

                VStack(alignment: .leading, spacing: 4) {
                    Text("REMOTES")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                }
                .padding(.horizontal, 32)
                .padding(.top, 16)
                .padding(.bottom, 32)

                VStack(alignment: .leading, spacing: 12) {
                    if libraryModel.remotes.isEmpty {
                        Text("No remotes configured.")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkMuted)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 20)
                            .lascoPanel()
                    } else {
                        ForEach(libraryModel.remotes, id: \.id) { remote in
                            RemoteCard(
                                remote: remote,
                                isDefaultFetch: remote.id == libraryModel.defaultFetchRemoteId,
                                onDelete: { libraryModel.removeRemote(id: remote.id) },
                                onTestConnection: {
                                    Task {
                                        let ok = await libraryModel.connectRemote(remoteId: remote.id)
                                        if ok {
                                            toastManager.show(ok: "\(remote.name): reachable")
                                        } else {
                                            toastManager.show(error: "\(remote.name): unreachable")
                                        }
                                    }
                                },
                                onSetDefaultFetch: {
                                    libraryModel.setDefaultFetchRemote(remoteId: remote.id)
                                }
                            )
                        }
                    }

                    Button("Add remote") { showRemotePicker = true }
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                }
                .padding(.horizontal, 32)

                Spacer()
            }
        }
        .navigationBarBackButtonHidden(true)
        .navigationTitle("")
        .hideSystemNavigationBar()
        .toolbarBackButton(action: { dismiss() })
        .sheet(isPresented: $showRemotePicker) {
            RemoteTypePickerSheet(
                devMode: devMode,
                onS3: { showRemotePicker = false; showAddS3 = true },
                onLocalFS: { showRemotePicker = false; showAddLocalFS = true },
                onDismiss: { showRemotePicker = false }
            )
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
            .presentationDetents([.medium])
        }
        .sheet(isPresented: $showAddS3) {
            AddS3RemoteView()
                .environmentObject(libraryModel)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showAddLocalFS) {
            AddLocalFSRemoteView()
                .environmentObject(libraryModel)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
    }
}

struct RemoteTypePickerSheet: View {
    let devMode: Bool
    let onS3: () -> Void
    let onLocalFS: () -> Void
    let onDismiss: () -> Void
    @Environment(\.lascoTheme) var theme

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text("Add remote")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                    Spacer()
                    Button(action: onDismiss) {
                        Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .foregroundStyle(theme.inkMuted)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 8)

                Text("Choose a remote type")
                    .font(LascoFont.subtitle())
                    .foregroundStyle(theme.inkMuted)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 32)

                VStack(spacing: 12) {
                    Button("Add S3-compatible remote", action: onS3)
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)

                    if devMode {
                        Button("Add local filesystem remote", action: onLocalFS)
                            .buttonStyle(LascoDevButtonStyle())
                            .frame(maxWidth: .infinity)
                    }
                }
                .padding(.horizontal, 32)

                Spacer()
            }
        }
    }
}

private struct RemoteCard: View {
    let remote: FfiRemote
    let isDefaultFetch: Bool
    let onDelete: () -> Void
    let onTestConnection: () -> Void
    let onSetDefaultFetch: () -> Void

    @State private var showDeleteConfirm = false
    @Environment(\.lascoTheme) var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 6) {
                        Text(remote.name)
                            .font(LascoFont.body())
                            .foregroundStyle(theme.ink)
                        if isDefaultFetch {
                            Text("DEFAULT FETCH")
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.pink)
                                .padding(.horizontal, 4)
                                .padding(.vertical, 1)
                                .overlay(RoundedRectangle(cornerRadius: 2).stroke(theme.pink, lineWidth: 1))
                        }
                    }

                    Text(remote.kind)
                        .font(LascoFont.mono())
                        .foregroundStyle(theme.inkMuted)

                    if let endpoint = remote.endpoint, let bucket = remote.bucket {
                        Text("\(endpoint) / \(bucket)")
                            .font(LascoFont.mono())
                            .foregroundStyle(theme.inkMuted)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    } else if let path = remote.path {
                        Text(path)
                            .font(LascoFont.mono())
                            .foregroundStyle(theme.inkMuted)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                Button(action: { showDeleteConfirm = true }) {
                    Image("trash").renderingMode(.template).resizable().frame(width: 18, height: 18)
                        .foregroundStyle(theme.inkMuted)
                }
                .buttonStyle(.plain)
                .confirmationDialog("Remove \"\(remote.name)\"?", isPresented: $showDeleteConfirm, titleVisibility: .visible) {
                    Button("Remove Remote", role: .destructive, action: onDelete)
                    Button("Cancel", role: .cancel) {}
                } message: {
                    Text("This will remove the remote from your config. Synced data will not be deleted.")
                }
            }

            HStack(spacing: 8) {
                Button("Test connection", action: onTestConnection)
                    .buttonStyle(LascoSecondaryButtonStyle())
                if !isDefaultFetch {
                    Button("Set as default fetch", action: onSetDefaultFetch)
                        .buttonStyle(LascoSecondaryButtonStyle())
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .lascoPanelHard()
    }
}
