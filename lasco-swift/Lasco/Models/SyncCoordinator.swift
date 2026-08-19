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

private actor SyncCommandGate {
    private var tail: Task<Void, Never>?
    private var cancelOperations: [UUID: @Sendable () -> Void] = [:]

    func run<T: Sendable>(_ operation: @escaping @Sendable () async throws -> T) async throws -> T {
        let previous = tail
        let current = Task {
            if let previous { await previous.value }
            return try await operation()
        }
        let operationID = UUID()
        cancelOperations[operationID] = { current.cancel() }
        tail = Task {
            _ = await current.result
        }
        defer { cancelOperations.removeValue(forKey: operationID) }
        return try await current.value
    }

    func cancel() {
        for cancel in cancelOperations.values {
            cancel()
        }
        cancelOperations.removeAll()
        tail?.cancel()
        tail = nil
    }
}

@MainActor
@Observable
final class SyncCoordinator {
    private(set) var busyRemotes: Set<FfiRemoteUuid> = []
    private(set) var fetchInProgress = false
    private(set) var nextPushDate: Date?
    private(set) var lastPushRecords: [FfiRemoteUuid: SyncRecord] = [:]
    private(set) var lastFetchRecords: [FfiRemoteUuid: SyncRecord] = [:]

    private let repository: any LibraryRepositoryProtocol
    private let session: LibrarySessionState
    private let gate = SyncCommandGate()
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
        !busyRemotes.contains(remoteID) && !fetchInProgress
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

        busyRemotes.insert(remoteID)
        defer { busyRemotes.remove(remoteID) }
        do {
            _ = try await gate.run { [repository] in
                try await repository.push(remoteID: remoteID)
            }
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
        busyRemotes.insert(remoteID)
        defer { busyRemotes.remove(remoteID) }
        do {
            let confirmed = try await gate.run { [repository] in
                try await repository.confirmRemoteMedia(remoteID: remoteID)
            }
            return .confirmed(confirmed)
        } catch is CancellationError {
            return .failed("Confirmation cancelled")
        } catch {
            return .failed(error.localizedDescription)
        }
    }

    func fetch(remoteID: FfiRemoteUuid) async -> String? {
        busyRemotes.insert(remoteID)
        fetchInProgress = true
        defer {
            busyRemotes.remove(remoteID)
            fetchInProgress = false
        }
        do {
            _ = try await gate.run { [repository] in
                try await repository.fetch(remoteID: remoteID)
            }
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
                for remote in session.remotes where remote.autoPush {
                    _ = await push(remoteID: remote.remoteId, isAutomatic: true)
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
        await gate.cancel()
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
