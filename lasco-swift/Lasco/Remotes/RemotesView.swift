import SwiftUI

struct RemotesView: View {
    @Environment(ToastManager.self) var toastManager
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme

    @AppStorage("expertMode") private var expertMode = false

    @State private var showRemotePicker = false
    @State private var showAddS3 = false
    @State private var showAddLocalFS = false
    @State private var showCloudLogin = false
    @State private var isUpdatingMediaSourceOrder = false
    @State private var removalBlocked: RemoteRemovalBlockedContext?
    @State private var removalBlockedByScheduledPush: FfiRemote?
    @State private var removalFailed: String?
    let repository: LibraryRepository
    let session: LibrarySessionState
    let syncCoordinator: SyncCoordinator

    init(repository: LibraryRepository, session: LibrarySessionState, syncCoordinator: SyncCoordinator) {
        self.repository = repository
        self.session = session
        self.syncCoordinator = syncCoordinator
    }

    /// Removes a remote unless this client cannot account for media once it is gone, in which
    /// case the sheet offers to refresh what the remaining remotes are known to hold first.
    /// A scheduled push claims the remote when it fires, which would make the removal fail on
    /// timing alone. Refusing up front says so while the countdown is still visible.
    private func isPushScheduled(for remote: FfiRemote) -> Bool {
        remote.autoPush && syncCoordinator.nextPushDate != nil
    }

    private func removeRemote(_ remote: FfiRemote) async {
        guard !isPushScheduled(for: remote) else {
            removalBlockedByScheduledPush = remote
            return
        }
        let count = (try? await repository.mediaCountLostIfRemoteRemoved(remoteID: remote.remoteId)) ?? 0
        guard count > 0 else {
            await commitRemoval(remote)
            return
        }
        removalBlocked = RemoteRemovalBlockedContext(
            target: remote,
            others: session.remotes.filter { $0.remoteId != remote.remoteId },
            mediaCount: count
        )
    }

    /// A push or fetch that claimed the remote between the scheduled push check and here stops
    /// the removal, which leaves the configuration untouched. Saying so is the only way the
    /// user learns the remote is still there.
    private func commitRemoval(_ remote: FfiRemote) async {
        do {
            try await repository.removeRemote(id: remote.remoteId)
        } catch LascoError.SyncBusy {
            removalFailed = "\(remote.name) is syncing right now. Wait for it to finish, then remove it."
        } catch {
            removalFailed = "\(remote.name) could not be removed: \(error.localizedDescription)"
        }
    }

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

                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        if session.remotes.isEmpty {
                            Text("No remotes configured.")
                                .font(LascoFont.body())
                                .foregroundStyle(theme.inkMuted)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 16)
                                .padding(.vertical, 20)
                                .lascoPanel()
                        } else {
                            ForEach(session.remotes, id: \.remoteId) { remote in
                                RemoteCard(
                                    remote: remote,
                                    isDefaultFetch: remote.remoteId == session.defaultFetchRemoteID,
                                    onDelete: { Task { await removeRemote(remote) } },
                                    onTestConnection: {
                                        Task {
                                            do {
                                                try await repository.connectRemote(id: remote.remoteId)
                                                toastManager.show(ok: "\(remote.name): reachable")
                                            } catch {
                                                toastManager.show(error: "\(remote.name): unreachable")
                                            }
                                        }
                                    },
                                    onSetDefaultFetch: {
                                        Task { try? await repository.setDefaultFetchRemote(remoteID: remote.remoteId) }
                                    },
                                    onSetAutoPush: { enabled in
                                        Task { try? await repository.setRemoteAutoPush(remoteID: remote.remoteId, enabled: enabled) }
                                    },
                                    onInspectCompactionLock: {
                                        try? await repository.inspectCompactionLock(remoteID: remote.remoteId)
                                    },
                                    onRemoveOwnCompactionLock: {
                                        (try? await repository.removeOwnCompactionLock(remoteID: remote.remoteId)) ?? false
                                    }
                                )
                            }

                            if orderedMediaSources.count > 1 {
                                downloadPrioritySection
                            }
                        }
                    }
                    .padding(.horizontal, 32)
                    .padding(.bottom, 20)
                }

                Button("Add remote") { showRemotePicker = true }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 32)
                    .padding(.top, 12)
                    .padding(.bottom, 48)
            }
        }
        .navigationBarBackButtonHidden(true)
        .navigationTitle("")
        .hideSystemNavigationBar()
        .toolbarBackButton(action: { dismiss() })
        .preference(key: HideTabBarKey.self, value: true)
        .sheet(isPresented: $showRemotePicker) {
            RemoteTypePickerSheet(
                expertMode: expertMode,
                onCloud: { showRemotePicker = false; showCloudLogin = true },
                onS3: { showRemotePicker = false; showAddS3 = true },
                onLocalFS: { showRemotePicker = false; showAddLocalFS = true },
                onDismiss: { showRemotePicker = false }
            )
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
            .presentationDetents([.medium])
        }
        .alert(
            "Remote not removed",
            isPresented: Binding(
                get: { removalFailed != nil },
                set: { if !$0 { removalFailed = nil } }
            )
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            if let removalFailed {
                Text(removalFailed)
            }
        }
        .alert(
            "Push scheduled",
            isPresented: Binding(
                get: { removalBlockedByScheduledPush != nil },
                set: { if !$0 { removalBlockedByScheduledPush = nil } }
            )
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            if let remote = removalBlockedByScheduledPush {
                Text("A push to \(remote.name) is about to run. Let it finish, or turn off Auto Push, then remove the remote.")
            }
        }
        .sheet(item: $removalBlocked) { context in
            MediaAtRiskSheet(
                title: "Remove remote?",
                message: "\(context.mediaLabel) would probably be lost. This is based on each remote's media list as of its last update, so they may already be elsewhere. Update the lists you want, then try again.",
                remotes: context.others.map {
                    MediaAtRiskRemote(remote: $0, role: "Could already hold them")
                },
                retryLabel: "Try again",
                cancelLabel: "Cancel",
                onConfirm: { await syncCoordinator.confirmRemoteMedia(remoteID: $0) },
                onRetry: {
                    let target = context.target
                    removalBlocked = nil
                    Task { await removeRemote(target) }
                },
                onCancel: { removalBlocked = nil }
            )
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showAddS3) {
            AddS3RemoteView()
                .environment(repository)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showCloudLogin) {
            LascoCloudLoginView(repository: repository, libraryID: session.libraryID)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showAddLocalFS) {
            AddLocalFSRemoteView()
                .environment(repository)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
    }

    private var orderedMediaSources: [FfiRemote] {
        let remotesByID = Dictionary(uniqueKeysWithValues: session.remotes.map { ($0.remoteId, $0) })
        return session.mediaSourceOrder.compactMap { remotesByID[$0] }
    }

    private var downloadPrioritySection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("DOWNLOAD PRIORITY")
                .font(LascoFont.mono())
                .foregroundStyle(theme.inkMuted)

            Text("Priority list to download media")
                .font(LascoFont.body())
                .foregroundStyle(theme.inkMuted)

            ForEach(Array(orderedMediaSources.enumerated()), id: \.element.remoteId) { index, remote in
                DownloadPriorityRow(
                    position: index + 1,
                    remote: remote,
                    canMoveEarlier: index > 0 && !isUpdatingMediaSourceOrder,
                    canMoveLater: index < orderedMediaSources.count - 1 && !isUpdatingMediaSourceOrder,
                    onMoveEarlier: { moveMediaSource(at: index, by: -1) },
                    onMoveLater: { moveMediaSource(at: index, by: 1) }
                )
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .lascoPanelHard()
    }

    private func moveMediaSource(at index: Int, by offset: Int) {
        let destination = index + offset
        guard orderedMediaSources.indices.contains(index), orderedMediaSources.indices.contains(destination) else {
            return
        }

        var reorderedIDs = orderedMediaSources.map(\.remoteId)
        reorderedIDs.swapAt(index, destination)
        isUpdatingMediaSourceOrder = true
        Task {
            do {
                try await repository.setMediaSourceOrder(remoteIDs: reorderedIDs)
            } catch {
                toastManager.show(error: "Could not update download priority")
            }
            isUpdatingMediaSourceOrder = false
        }
    }
}

struct RemoteTypePickerSheet: View {
    let expertMode: Bool
    let onCloud: () -> Void
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
                    Button("Authenticate with Lasco Cloud", action: onCloud)
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                    Button("Add S3-compatible remote", action: onS3)
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)

                    if expertMode {
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

private struct DownloadPriorityRow: View {
    let position: Int
    let remote: FfiRemote
    let canMoveEarlier: Bool
    let canMoveLater: Bool
    let onMoveEarlier: () -> Void
    let onMoveLater: () -> Void

    @Environment(\.lascoTheme) private var theme

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            Text("\(position)")
                .font(LascoFont.mono())
                .foregroundStyle(theme.pink)
                .frame(width: 20, alignment: .leading)

            VStack(alignment: .leading, spacing: 2) {
                Text(remote.name)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                Text(remote.kind)
                    .font(LascoFont.mono())
                    .foregroundStyle(theme.inkMuted)
            }

            Spacer()

            VStack(spacing: 4) {
                Button(action: onMoveEarlier) {
                    Text("↑")
                }
                    .buttonStyle(LascoSecondaryButtonStyle())
                    .disabled(!canMoveEarlier)
                    .accessibilityLabel("Move \(remote.name) earlier")
                Button(action: onMoveLater) {
                    Text("↓")
                }
                    .buttonStyle(LascoSecondaryButtonStyle())
                    .disabled(!canMoveLater)
                    .accessibilityLabel("Move \(remote.name) later")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Priority \(position), \(remote.name), \(remote.kind)")
    }
}

private struct RemoteCard: View {
    let remote: FfiRemote
    let isDefaultFetch: Bool
    let onDelete: () -> Void
    let onTestConnection: () -> Void
    let onSetDefaultFetch: () -> Void
    let onSetAutoPush: (Bool) -> Void
    let onInspectCompactionLock: () async -> FfiCompactionLockInfo?
    let onRemoveOwnCompactionLock: () async -> Bool

    @State private var showDeleteConfirm = false
    @State private var lockInfo: FfiCompactionLockInfo?
    @State private var showRemoveLockConfirm = false
    @Environment(\.lascoTheme) var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 6) {
                        Text(remote.name)
                            .font(LascoFont.body())
                            .foregroundStyle(theme.ink)
                        Toggle("Auto push", isOn: Binding(
                            get: { remote.autoPush },
                            set: onSetAutoPush
                        ))
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
                        let prefix = remote.path?.isEmpty == false ? "/\(remote.path!)" : ""
                        Text("\(endpoint) / \(bucket)\(prefix)")
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

            if let lockInfo {
                Text("Compaction lock: \(lockInfo.ownerDeviceId.prefix(8))… since \(lockInfo.createdAt)")
                    .font(LascoFont.mono())
                    .foregroundStyle(theme.inkMuted)
                    .lineLimit(2)
                if lockInfo.isOwnedByCurrentDevice {
                    Button("Remove my compaction lock") {
                        showRemoveLockConfirm = true
                    }
                    .buttonStyle(LascoSecondaryButtonStyle())
                    .confirmationDialog("Remove your compaction lock?", isPresented: $showRemoveLockConfirm, titleVisibility: .visible) {
                        Button("Remove Lock", role: .destructive) {
                            Task {
                                if await onRemoveOwnCompactionLock() { self.lockInfo = nil }
                            }
                        }
                        Button("Cancel", role: .cancel) {}
                    } message: {
                        Text("Only remove this after confirming that this device is no longer compacting this remote.")
                    }
                }
            } else {
                Button("Check compaction lock") {
                    Task { lockInfo = await onInspectCompactionLock() }
                }
                .buttonStyle(LascoSecondaryButtonStyle())
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .lascoPanelHard()
    }
}

/// The media that removing one remote would leave this client unable to place, along with the
/// remotes whose media list could still turn out to hold them.
struct RemoteRemovalBlockedContext: Identifiable {
    let target: FfiRemote
    let others: [FfiRemote]
    let mediaCount: Int

    var id: FfiRemoteUuid { target.remoteId }

    var mediaLabel: String {
        mediaCount == 1 ? "1 media" : "\(mediaCount) media"
    }
}
