import SwiftUI

struct StatusView: View {
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @AppStorage("expertMode") private var expertMode = false

    @State private var showRemotePicker = false
    @State private var showAddS3 = false
    @State private var showAddLocalFS = false
    @State private var showCloudLogin = false
    @State private var cloudConnected = false
    @State private var showCleanConfirm = false
    @State private var cleanBlockedCount: Int?
    @State private var cleanOverrideCount: Int?
    @State private var showClearThumbsConfirm = false
    @State private var pushBlocked: PushBlockedContext?
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
        .sheet(item: $pushBlocked) { context in
            PushBlockedSheet(
                targetRemote: context.target,
                sourceRemotes: context.sources,
                mediaCount: context.mediaCount,
                onConfirm: { await syncCoordinator.confirmRemoteMedia(remoteID: $0) },
                onRetry: {
                    let target = context.target
                    pushBlocked = nil
                    Task { await push(target) }
                },
                onCancel: { pushBlocked = nil }
            )
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showRemotePicker) {
            RemoteTypePickerSheet(
                expertMode: expertMode,
                showCloud: !cloudConnected,
                onCloud: { showRemotePicker = false; showCloudLogin = true },
                onS3: { showRemotePicker = false; showAddS3 = true },
                onLocalFS: { showRemotePicker = false; showAddLocalFS = true },
                onDismiss: { showRemotePicker = false }
            )
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
            .presentationDetents([.medium])
        }
        .sheet(isPresented: $showCloudLogin) {
            LascoCloudLoginView(repository: repository, libraryID: session.libraryID)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .task(id: session.remotes.count) {
            cloudConnected = await repository.isLascoCloudConnected(libraryID: session.libraryID)
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
            if expertMode {
                Button("Clean anyway", role: .destructive) {
                    cleanOverrideCount = cleanBlockedCount
                    cleanBlockedCount = nil
                }
            }
            Button("OK", role: .cancel) {}
        } message: {
            if let count = cleanBlockedCount {
                Text("\(count) item\(count == 1 ? "" : "s") not confirmed on any remote. This is based on each remote's media list as of its last update, so some may already be there. Push to a remote before cleaning local media.")
            }
        }
        .alert(
            "Lose these media forever?",
            isPresented: Binding(
                get: { cleanOverrideCount != nil },
                set: { if !$0 { cleanOverrideCount = nil } }
            )
        ) {
            Button("I understand, I might lose data", role: .destructive) {
                Task { try? await model.cleanLocalMedia() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            if let count = cleanOverrideCount {
                Text("\(count) item\(count == 1 ? "" : "s") might be the only copy left. Cleaning deletes them from this device with no way to get them back.")
            }
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
                    Task {
                        let count = await model.mediaCountLostIfLocalMediaCleared()
                        if count > 0 {
                            cleanBlockedCount = count
                        } else {
                            showCleanConfirm = true
                        }
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
                ForEach(session.remotes, id: \.remoteId) { remote in
                    RemoteStatusCard(
                        remote: remote,
                        isDefaultFetch: remote.remoteId == session.defaultFetchRemoteID,
                        lastPush: syncCoordinator.lastPushRecords[remote.remoteId],
                        lastFetch: syncCoordinator.lastFetchRecords[remote.remoteId],
                        isSynced: isSynced(remote),
                        shortfall: model.shortfall(remoteID: remote.remoteId),
                        nextPushDate: remote.autoPush ? syncCoordinator.nextPushDate : nil,
                        pushEnabled: syncCoordinator.isPushAllowed(remote.remoteId),
                        fetchEnabled: syncCoordinator.isFetchAllowed(remote.remoteId),
                        onPush: { Task { await push(remote) } },
                        onFetch: {
                            Task {
                                if let err = await syncCoordinator.fetch(remoteID: remote.remoteId) {
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

    /// A manual push offers the recovery sheet when preparation could not place some media.
    /// An automatic push has no one to ask, so `SyncCoordinator` reports its failure as is.
    private func push(_ remote: FfiRemote) async {
        switch await syncCoordinator.push(remoteID: remote.remoteId) {
        case .success:
            toastManager.show(ok: "\(remote.name): pushed")
        case .failed(let message):
            toastManager.show(error: message)
        case .missingLocalMedia:
            toastManager.show(error: "Some media is not stored on this device or in the configured download sources.")
        case .missingMediaOnConfiguredSources(let mediaIds):
            pushBlocked = PushBlockedContext(
                target: remote,
                sources: session.mediaSourceOrder
                    .filter { $0 != remote.remoteId }
                    .compactMap { id in session.remotes.first { $0.remoteId == id } },
                mediaCount: mediaIds.count
            )
        }
    }

    private func formatGB(_ bytes: UInt64) -> String {
        let gb = Double(bytes) / 1_000_000_000
        return String(format: "%.2f GB", gb)
    }

    private func isSynced(_ remote: FfiRemote) -> Bool {
        model.isSynced(remoteID: remote.remoteId)
    }
}


private struct RemoteStatusCard: View {
    @Environment(\.lascoTheme) var theme

    let remote: FfiRemote
    let isDefaultFetch: Bool
    let lastPush: SyncRecord?
    let lastFetch: SyncRecord?
    let isSynced: Bool
    let shortfall: FfiRemoteMediaShortfall?
    let nextPushDate: Date?
    let pushEnabled: Bool
    let fetchEnabled: Bool
    let onPush: () -> Void
    let onFetch: () -> Void

    private func pushBannerText(now: Date) -> String {
        guard isSynced else {
            if let nextPushDate, nextPushDate > now {
                let seconds = Int(nextPushDate.timeIntervalSince(now).rounded(.up))
                return "local changes not pushed, pushing in \(seconds)s"
            }
            return "local changes not pushed"
        }
        if let text = shortfallText { return text }
        return "all local changes pushed"
    }

    /// Reads only once every operation has reached the remote, since media a remote has never
    /// been told about cannot be expected on it. A missing thumbnail has no fallback, so a
    /// remote short of thumbnails cannot be browsed from at all.
    private var shortfallText: String? {
        guard let shortfall else { return nil }
        let media = Int(shortfall.missingFull)
        let thumbs = Int(shortfall.missingThumb)
        let parts = [
            media > 0 ? "\(media) media" : nil,
            thumbs > 0 ? "\(thumbs) thumbnail\(thumbs == 1 ? "" : "s")" : nil,
        ].compactMap { $0 }
        guard !parts.isEmpty else { return nil }
        return "\(parts.joined(separator: " and ")) not confirmed on remote"
    }

    private var bannerIsWarning: Bool { !isSynced || shortfallText != nil }

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
                Text(remote.remoteId.value)
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
                    .background(bannerIsWarning ? Color.red : theme.pink)
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

/// What the push recovery sheet needs: the blocked push's target, the remotes that could hold
/// the media, and how many media are concerned.
struct PushBlockedContext: Identifiable {
    let target: FfiRemote
    let sources: [FfiRemote]
    let mediaCount: Int

    var id: String { target.remoteId.value }
}
