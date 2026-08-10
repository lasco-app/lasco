import SwiftUI

struct StatusView: View {
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @AppStorage("expertMode") private var expertMode = false

    @State private var showRemotePicker = false
    @State private var showAddS3 = false
    @State private var showAddLocalFS = false
    @State private var showCleanConfirm = false
    @State private var cleanBlockedCount: Int? = nil
    @State private var showClearThumbsConfirm = false
    @State private var pendingRelay: PendingRelayRequest?
    let repository: LibraryRepository
    let session: LibrarySessionState
    let syncCoordinator: SyncCoordinator
    @State private var model: StatusModel

    init(repository: LibraryRepository, session: LibrarySessionState, syncCoordinator: SyncCoordinator) {
        self.repository = repository
        self.session = session
        self.syncCoordinator = syncCoordinator
        _model = State(initialValue: StatusModel(repository: repository))
    }

    var body: some View {
        NavigationStack {
            ZStack {
                theme.bg.ignoresSafeArea()

                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("STATUS")
                                .font(LascoFont.categoryLarge())
                                .foregroundStyle(theme.ink)
                            Text(session.nickname)
                                .font(LascoFont.subtitle())
                                .foregroundStyle(theme.inkMuted)
                        }
                        .padding(.top, 20)

                        mediaSection

                        localStateSection

                        remotesSection
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 100)
                }
                .task { await model.start() }
            }
        }
        .sheet(isPresented: $showRemotePicker) {
            RemoteTypePickerSheet(
                expertMode: expertMode,
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
                .environment(repository)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showAddLocalFS) {
            AddLocalFSRemoteView()
                .environment(repository)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .confirmationDialog(
            "Clean local media?",
            isPresented: $showCleanConfirm,
            titleVisibility: .visible
        ) {
            Button("Clean", role: .destructive) { Task { try? await model.cleanLocalMedia() } }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This deletes cached originals from this device. Thumbnails are kept. They remain available on your remotes.")
        }
        .confirmationDialog(
            "Clear local thumbnails?",
            isPresented: $showClearThumbsConfirm,
            titleVisibility: .visible
        ) {
            Button("Clear", role: .destructive) { Task { try? await model.cleanLocalThumbnails() } }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This deletes cached thumbnails from this device. They are re-downloaded as needed.")
        }
        .alert(
            "Not fully backed up",
            isPresented: Binding(
                get: { cleanBlockedCount != nil },
                set: { if !$0 { cleanBlockedCount = nil } }
            )
        ) {
            Button("OK") {}
        } message: {
            if let count = cleanBlockedCount {
                Text("\(count) item\(count == 1 ? "" : "s") not backed up on any remote. Push to a remote before cleaning local media.")
            }
        }
        .confirmationDialog(
            "Choose media source",
            isPresented: Binding(get: { pendingRelay != nil }, set: { if !$0 { pendingRelay = nil } }),
            titleVisibility: .visible
        ) {
            if let request = pendingRelay {
                ForEach(request.candidates, id: \.id) { source in
                    Button(source.name) {
                        pendingRelay = nil
                        Task {
                            switch await syncCoordinator.push(remoteID: request.target.id, sourceRemoteID: source.id) {
                            case .success:
                                toastManager.show(ok: "\(request.target.name): pushed")
                            case .failed(let message):
                                toastManager.show(error: message)
                            case .missingLocalMedia:
                                toastManager.show(error: "The selected remote does not contain all missing media.")
                            }
                        }
                    }
                }
            }
            Button("Cancel", role: .cancel) { pendingRelay = nil }
        } message: {
            Text("Some media is not stored on this device. Choose a remote to download it from before continuing this backup.")
        }
    }

    private var mediaSection: some View {
        return VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Library")
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkSub)
                Spacer()
                VStack(alignment: .trailing, spacing: 2) {
                    Text("\(model.mediaCount) items")
                        .font(LascoFont.mono())
                        .foregroundStyle(theme.ink)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
        .lascoPanel()
    }

    private var localStateSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("LOCAL STATE")
                .font(LascoFont.categoryLarge())
                .foregroundStyle(theme.ink)

            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text("Cached")
                        .font(LascoFont.body())
                        .foregroundStyle(theme.inkSub)
                    Spacer()
                    if let stats = model.localStateStats {
                        VStack(alignment: .trailing, spacing: 2) {
                            Text("\(stats.mediaCachedCount) media  \(formatGB(stats.mediaCachedBytes))")
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.ink)
                            Text("\(stats.thumbCachedCount) thumbnails  \(formatGB(stats.thumbCachedBytes))")
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.inkMuted)
                        }
                    } else {
                        Text("—")
                            .font(LascoFont.mono())
                            .foregroundStyle(theme.inkMuted)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)

                Divider().background(theme.inkMuted.opacity(0.2))

                Button {
                    if let count = model.mediaCountWithoutRemoteBackup {
                        cleanBlockedCount = count
                    } else {
                        showCleanConfirm = true
                    }
                } label: {
                    Text("Clean local media")
                        .font(LascoFont.body())
                        .foregroundStyle(.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                }

                if expertMode {
                    Divider().background(theme.inkMuted.opacity(0.2))

                    Button {
                        showClearThumbsConfirm = true
                    } label: {
                        Text("Clear local thumbnails")
                            .font(LascoFont.body())
                            .foregroundStyle(.red)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 12)
                    }
                }
            }
            .lascoPanel()
        }
    }

    private var remotesSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("REMOTES")
                .font(LascoFont.categoryLarge())
                .foregroundStyle(theme.ink)

            if session.remotes.isEmpty {
                VStack(alignment: .leading, spacing: 12) {
                    Text("No remotes configured.")
                        .font(LascoFont.body())
                        .foregroundStyle(theme.inkMuted)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 14)
                        .lascoPanel()

                    Button("Add remote") { showRemotePicker = true }
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                }
            } else {
                ForEach(session.remotes, id: \.id) { remote in
                    RemoteStatusCard(
                        remote: remote,
                        isDefaultFetch: remote.id == session.defaultFetchRemoteID,
                        lastPush: syncCoordinator.lastPushRecords[remote.id],
                        lastFetch: syncCoordinator.lastFetchRecords[remote.id],
                        isSynced: isSynced(remote),
                        nextPushDate: remote.autoPush ? syncCoordinator.nextPushDate : nil,
                        pushEnabled: syncCoordinator.isPushAllowed(remote.id),
                        fetchEnabled: syncCoordinator.isFetchAllowed(remote.id),
                        onPush: {
                            Task {
                                switch await syncCoordinator.push(remoteID: remote.id) {
                                case .success:
                                    toastManager.show(ok: "\(remote.name): pushed")
                                case .failed(let message):
                                    toastManager.show(error: message)
                                case .missingLocalMedia(let mediaIDs):
                                    let candidates = session.remotes.filter { $0.id != remote.id }
                                    if candidates.isEmpty {
                                        toastManager.show(error: "Some media is not stored on this device, and no other remote is available to retrieve it from.")
                                    } else {
                                        pendingRelay = PendingRelayRequest(target: remote, mediaIDs: mediaIDs, candidates: candidates)
                                    }
                                }
                            }
                        },
                        onFetch: {
                            Task {
                                if let err = await syncCoordinator.fetch(remoteID: remote.id) {
                                    toastManager.show(error: err)
                                } else {
                                    toastManager.show(ok: "\(remote.name): fetched")
                                }
                            }
                        }
                    )
                }
            }
        }
    }

    private func formatGB(_ bytes: UInt64) -> String {
        let gb = Double(bytes) / 1_000_000_000
        return String(format: "%.2f GB", gb)
    }

    private func isSynced(_ remote: FfiRemote) -> Bool {
        model.isSynced(remoteID: remote.id)
    }
}

private struct PendingRelayRequest {
    let target: FfiRemote
    let mediaIDs: [FfiMediaId]
    let candidates: [FfiRemote]
}

private struct RemoteStatusCard: View {
    @Environment(\.lascoTheme) var theme

    let remote: FfiRemote
    let isDefaultFetch: Bool
    let lastPush: SyncRecord?
    let lastFetch: SyncRecord?
    let isSynced: Bool
    let nextPushDate: Date?
    let pushEnabled: Bool
    let fetchEnabled: Bool
    let onPush: () -> Void
    let onFetch: () -> Void

    private func pushBannerText(now: Date) -> String {
        guard !isSynced else { return "all local changes pushed" }
        if let nextPushDate, nextPushDate > now {
            let seconds = Int(nextPushDate.timeIntervalSince(now).rounded(.up))
            return "local changes not pushed, pushing in \(seconds)s"
        }
        return "local changes not pushed"
    }

    private func syncLabel(_ record: SyncRecord?) -> String {
        guard let record else { return "never" }
        let cal = Calendar.current
        let time = record.date.formatted(.dateTime.hour().minute())
        if cal.isDateInToday(record.date) { return "today \(time)" }
        let day = record.date.formatted(.dateTime.month(.abbreviated).day())
        return "\(day) \(time)"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    Text(remote.name)
                        .font(LascoFont.body())
                        .foregroundStyle(theme.ink)
                    Text(remote.kind)
                        .font(LascoFont.mono())
                        .foregroundStyle(theme.inkMuted)
                }
                Text(remote.id)
                    .font(.system(size: 10))
                    .foregroundStyle(theme.inkMuted)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)

            Divider().background(theme.inkMuted.opacity(0.2))

            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(pushBannerText(now: context.date))
                    .font(LascoFont.mono())
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .background(isSynced ? theme.pink : Color.red)
            }

            Divider().background(theme.inkMuted.opacity(0.2))

            SyncStatusRow(label: "Push", record: lastPush, dateLabel: syncLabel(lastPush), enabled: pushEnabled, action: onPush)
            Divider().background(theme.inkMuted.opacity(0.2))
            SyncStatusRow(label: "Fetch", record: lastFetch, dateLabel: syncLabel(lastFetch), isDefaultFetch: isDefaultFetch, enabled: fetchEnabled, action: onFetch)
        }
        .lascoPanel()
    }
}

private struct SyncStatusRow: View {
    @Environment(\.lascoTheme) var theme

    let label: String
    let record: SyncRecord?
    let dateLabel: String
    var isDefaultFetch: Bool = false
    var enabled: Bool = true
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let record, !record.success {
                    Circle()
                        .fill(Color.red)
                        .frame(width: 7, height: 7)
                }
                Text(label)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkSub)
                if isDefaultFetch {
                    Text("default fetch")
                        .font(.system(size: 10))
                        .foregroundStyle(theme.pink)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(theme.pink.opacity(0.12))
                        .clipShape(Capsule())
                }
                Spacer()
                Text(dateLabel)
                    .font(LascoFont.mono())
                    .foregroundStyle(theme.inkMuted)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.4)
    }
}
