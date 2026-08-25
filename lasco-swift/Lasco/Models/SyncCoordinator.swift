import Foundation
import Observation

struct SyncRecord: Sendable, Equatable {
    let date: Date
    let success: Bool
}

enum PushResult: Sendable {
    case success
    case missingLocalMedia([FfiMediaId])
    /// Push preparation found no place to get some media from. The remedy is confirming one or
    /// more remotes, so a manual push offers that instead of only reporting the failure.
    case missingMediaOnConfiguredSources([FfiMediaId])
    case failed(String)
}

enum ConfirmMediaResult: Sendable {
    case confirmed(UInt64)
    case failed(String)
}

/// The UniFFI callback reaches this object from Lasco's Rust runtime. It has no UI state of its
/// own; it immediately forwards the fraction to the main actor that owns `SyncCoordinator`.
private final class SyncPushProgressSink: PushProgressSink {
    private let receive: @Sendable (Double) -> Void

    init(receive: @escaping @Sendable (Double) -> Void) {
        self.receive = receive
    }

    func uploadProgress(fraction: Double) {
        receive(fraction)
    }
}

@MainActor
@Observable
final class SyncCoordinator {
    private(set) var busyRemotes: Set<FfiRemoteUuid> = []
    private(set) var pushingRemotes: Set<FfiRemoteUuid> = []
    private(set) var pushUploadProgress: [FfiRemoteUuid: Double] = [:]
    private var activeOperations: [FfiRemoteUuid: Set<UUID>] = [:]
    private var activePushes: [FfiRemoteUuid: Set<UUID>] = [:]
    private var activeFetches: Set<UUID> = []
    private(set) var fetchInProgress = false
    private(set) var nextPushDate: Date?
    private(set) var lastPushRecords: [FfiRemoteUuid: SyncRecord] = [:]
    private(set) var lastFetchRecords: [FfiRemoteUuid: SyncRecord] = [:]

    private let repository: any LibraryRepositoryProtocol
    private let session: LibrarySessionState
    private var delayedPushTask: Task<Void, Never>?
    private var changeTask: Task<Void, Never>?

    init(repository: any LibraryRepositoryProtocol, session: LibrarySessionState) {
        self.repository = repository
        self.session = session
        changeTask = Task { [weak self] in
            await self?.listenForLocalMutations()
        }
    }

    func isPushAllowed(_ remoteID: FfiRemoteUuid) -> Bool {
        !busyRemotes.contains(remoteID)
    }

    func isFetchAllowed(_ remoteID: FfiRemoteUuid) -> Bool {
        !busyRemotes.contains(remoteID) && !fetchInProgress
    }

    func fetchDefaultRemote() async {
        guard let remoteID = session.defaultFetchRemoteID else { return }
        _ = await fetch(remoteID: remoteID)
    }

    func push(remoteID: FfiRemoteUuid, isAutomatic: Bool = false) async -> PushResult {
        // A manual push supersedes the pending automatic push. Only the timer is
        // cancelled; an upload that has already started is left to finish.
        if !isAutomatic {
            cancelScheduledPush()
        }

        let operationID = UUID()
        beginPush(remoteID: remoteID, operationID: operationID)
        defer {
            endPush(remoteID: remoteID, operationID: operationID)
        }
        let progress = SyncPushProgressSink { [weak self] fraction in
            Task { @MainActor [weak self] in
                guard self?.activePushes[remoteID]?.contains(operationID) == true else { return }
                self?.pushUploadProgress[remoteID] = fraction
            }
        }
        do {
            _ = try await repository.push(remoteID: remoteID, progress: progress)
            record(key: "lasco.lastPush", remoteID: remoteID, success: true, in: &lastPushRecords)
            return .success
        } catch is CancellationError {
            return .failed("Push cancelled")
        } catch LascoError.MissingLocalMedia(let mediaIds) {
            record(key: "lasco.lastPush", remoteID: remoteID, success: false, in: &lastPushRecords)
            return .missingLocalMedia(mediaIds)
        } catch LascoError.MissingMediaOnConfiguredSources(let mediaIds) {
            record(key: "lasco.lastPush", remoteID: remoteID, success: false, in: &lastPushRecords)
            return .missingMediaOnConfiguredSources(mediaIds)
        } catch {
            record(key: "lasco.lastPush", remoteID: remoteID, success: false, in: &lastPushRecords)
            return .failed(error.localizedDescription)
        }
    }

    /// Refreshes what this client knows of the media one remote holds, without fetching. This is
    /// how a user resolves a push blocked by an out-of-date media list.
    func confirmRemoteMedia(remoteID: FfiRemoteUuid) async -> ConfirmMediaResult {
        let operationID = UUID()
        beginOperation(remoteID: remoteID, operationID: operationID)
        defer { endOperation(remoteID: remoteID, operationID: operationID) }
        do {
            let confirmed = try await repository.confirmRemoteMedia(remoteID: remoteID)
            return .confirmed(confirmed)
        } catch is CancellationError {
            return .failed("Confirmation cancelled")
        } catch {
            return .failed(error.localizedDescription)
        }
    }

    func fetch(remoteID: FfiRemoteUuid) async -> String? {
        let operationID = UUID()
        beginFetch(remoteID: remoteID, operationID: operationID)
        defer {
            endFetch(remoteID: remoteID, operationID: operationID)
        }
        do {
            _ = try await repository.fetch(remoteID: remoteID)
            record(key: "lasco.lastFetch", remoteID: remoteID, success: true, in: &lastFetchRecords)
            return nil
        } catch is CancellationError {
            return nil
        } catch {
            record(key: "lasco.lastFetch", remoteID: remoteID, success: false, in: &lastFetchRecords)
            return error.localizedDescription
        }
    }

    func schedulePush() {
        cancelScheduledPush()

        // Do not advertise (or run) an automatic push when every remote has it
        // disabled. This also covers local changes made after Auto Push is off.
        guard session.remotes.contains(where: \.autoPush) else { return }

        let date = Date.now.addingTimeInterval(30)
        nextPushDate = date
        delayedPushTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(30))
                guard let self, !Task.isCancelled else { return }

                // From here on this task performs uploads, rather than waiting for
                // a scheduled one. Clearing its handle keeps a manual push from
                // cancelling an upload already in progress.
                delayedPushTask = nil
                nextPushDate = nil
                let remoteIDs = session.remotes
                    .filter(\.autoPush)
                    .map(\.remoteId)
                await withTaskGroup(of: Void.self) { group in
                    for remoteID in remoteIDs {
                        group.addTask { [weak self] in
                            guard let self else { return }
                            _ = await self.push(remoteID: remoteID, isAutomatic: true)
                        }
                    }
                }
            } catch is CancellationError {
                return
            } catch {
                AppLogger.log(.error, "scheduled push failed: \(error)")
            }
        }
    }

    func close() async {
        changeTask?.cancel()
        changeTask = nil
        delayedPushTask?.cancel()
        delayedPushTask = nil
        nextPushDate = nil
    }

    private func beginOperation(remoteID: FfiRemoteUuid, operationID: UUID) {
        activeOperations[remoteID, default: []].insert(operationID)
        busyRemotes.insert(remoteID)
    }

    private func endOperation(remoteID: FfiRemoteUuid, operationID: UUID) {
        guard var operations = activeOperations[remoteID] else { return }
        operations.remove(operationID)
        if operations.isEmpty {
            activeOperations.removeValue(forKey: remoteID)
            busyRemotes.remove(remoteID)
        } else {
            activeOperations[remoteID] = operations
        }
    }

    private func beginPush(remoteID: FfiRemoteUuid, operationID: UUID) {
        beginOperation(remoteID: remoteID, operationID: operationID)
        activePushes[remoteID, default: []].insert(operationID)
        pushingRemotes.insert(remoteID)
    }

    private func endPush(remoteID: FfiRemoteUuid, operationID: UUID) {
        endOperation(remoteID: remoteID, operationID: operationID)
        guard var pushes = activePushes[remoteID] else { return }
        pushes.remove(operationID)
        if pushes.isEmpty {
            activePushes.removeValue(forKey: remoteID)
            pushingRemotes.remove(remoteID)
            pushUploadProgress.removeValue(forKey: remoteID)
        } else {
            activePushes[remoteID] = pushes
        }
    }

    private func beginFetch(remoteID: FfiRemoteUuid, operationID: UUID) {
        beginOperation(remoteID: remoteID, operationID: operationID)
        activeFetches.insert(operationID)
        fetchInProgress = true
    }

    private func endFetch(remoteID: FfiRemoteUuid, operationID: UUID) {
        endOperation(remoteID: remoteID, operationID: operationID)
        activeFetches.remove(operationID)
        fetchInProgress = !activeFetches.isEmpty
    }

    private func listenForLocalMutations() async {
        let stream = await repository.changes()
        for await change in stream {
            switch change {
            case .localMutation:
                schedulePush()
            case .session:
                await cancelScheduledPushIfNoEligibleRemote()
            default:
                continue
            }
        }
    }

    private func cancelScheduledPushIfNoEligibleRemote() async {
        do {
            let snapshot = try await repository.sessionSnapshot()
            guard snapshot.remotes.contains(where: \.autoPush) else {
                cancelScheduledPush()
                return
            }
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "could not check auto push settings: \(error)")
        }
    }

    private func cancelScheduledPush() {
        delayedPushTask?.cancel()
        delayedPushTask = nil
        nextPushDate = nil
    }

    /// Restores the per-remote sync history once the session has loaded its remotes.
    ///
    /// `LibrarySessionState` starts with no remotes, so this must be invoked after
    /// its asynchronous refresh rather than during `SyncCoordinator` initialization.
    func restorePersistedRecords(for remotes: [FfiRemote]) {
        var restoredPushRecords: [FfiRemoteUuid: SyncRecord] = [:]
        var restoredFetchRecords: [FfiRemoteUuid: SyncRecord] = [:]

        for remote in remotes {
            if let date = UserDefaults.standard.object(forKey: "lasco.lastPush.\(remote.remoteId.value)") as? Date {
                restoredPushRecords[remote.remoteId] = SyncRecord(date: date, success: UserDefaults.standard.bool(forKey: "lasco.lastPushOk.\(remote.remoteId.value)"))
            }
            if let date = UserDefaults.standard.object(forKey: "lasco.lastFetch.\(remote.remoteId.value)") as? Date {
                restoredFetchRecords[remote.remoteId] = SyncRecord(date: date, success: UserDefaults.standard.bool(forKey: "lasco.lastFetchOk.\(remote.remoteId.value)"))
            }
        }

        lastPushRecords = restoredPushRecords
        lastFetchRecords = restoredFetchRecords
    }

    private func record(key: String, remoteID: FfiRemoteUuid, success: Bool, in records: inout [FfiRemoteUuid: SyncRecord]) {
        let now = Date.now
        UserDefaults.standard.set(now, forKey: "\(key).\(remoteID.value)")
        UserDefaults.standard.set(success, forKey: "\(key)Ok.\(remoteID.value)")
        records[remoteID] = SyncRecord(date: now, success: success)
    }
}
