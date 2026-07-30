import Foundation
import Observation

struct SyncRecord: Sendable, Equatable {
    let date: Date
    let success: Bool
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
    private(set) var busyRemotes: Set<String> = []
    private(set) var fetchInProgress = false
    private(set) var nextPushDate: Date?
    private(set) var lastPushRecords: [String: SyncRecord] = [:]
    private(set) var lastFetchRecords: [String: SyncRecord] = [:]

    private let repository: any LibraryRepositoryProtocol
    private let session: LibrarySessionState
    private let gate = SyncCommandGate()
    private var delayedPushTask: Task<Void, Never>?
    private var changeTask: Task<Void, Never>?

    init(repository: any LibraryRepositoryProtocol, session: LibrarySessionState) {
        self.repository = repository
        self.session = session
        loadRecords(for: session.remotes)
        changeTask = Task { [weak self] in
            await self?.listenForLocalMutations()
        }
    }

    func isPushAllowed(_ remoteID: String) -> Bool {
        !busyRemotes.contains(remoteID) && !fetchInProgress
    }

    func isFetchAllowed(_ remoteID: String) -> Bool {
        !busyRemotes.contains(remoteID) && !fetchInProgress
    }

    func fetchDefaultRemote() async {
        guard let remoteID = session.defaultFetchRemoteID else { return }
        _ = await fetch(remoteID: remoteID)
    }

    func push(remoteID: String) async -> String? {
        busyRemotes.insert(remoteID)
        defer { busyRemotes.remove(remoteID) }
        do {
            _ = try await gate.run { [repository] in
                try await repository.push(remoteID: remoteID)
            }
            record(key: "lasco.lastPush", remoteID: remoteID, success: true, in: &lastPushRecords)
            return nil
        } catch is CancellationError {
            return nil
        } catch {
            record(key: "lasco.lastPush", remoteID: remoteID, success: false, in: &lastPushRecords)
            return error.localizedDescription
        }
    }

    func fetch(remoteID: String) async -> String? {
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

    func sync() async -> String? {
        fetchInProgress = true
        defer { fetchInProgress = false }
        do {
            _ = try await gate.run { [repository] in
                try await repository.sync()
            }
            return nil
        } catch is CancellationError {
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    func schedulePush() {
        delayedPushTask?.cancel()
        let date = Date.now.addingTimeInterval(30)
        nextPushDate = date
        delayedPushTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(30))
                guard let self, !Task.isCancelled else { return }
                nextPushDate = nil
                for remote in session.remotes {
                    _ = await push(remoteID: remote.id)
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
            guard change == .localMutation else { continue }
            schedulePush()
        }
    }

    private func loadRecords(for remotes: [FfiRemote]) {
        for remote in remotes {
            if let date = UserDefaults.standard.object(forKey: "lasco.lastPush.\(remote.id)") as? Date {
                lastPushRecords[remote.id] = SyncRecord(date: date, success: UserDefaults.standard.bool(forKey: "lasco.lastPushOk.\(remote.id)"))
            }
            if let date = UserDefaults.standard.object(forKey: "lasco.lastFetch.\(remote.id)") as? Date {
                lastFetchRecords[remote.id] = SyncRecord(date: date, success: UserDefaults.standard.bool(forKey: "lasco.lastFetchOk.\(remote.id)"))
            }
        }
    }

    private func record(key: String, remoteID: String, success: Bool, in records: inout [String: SyncRecord]) {
        let now = Date.now
        UserDefaults.standard.set(now, forKey: "\(key).\(remoteID)")
        UserDefaults.standard.set(success, forKey: "\(key)Ok.\(remoteID)")
        records[remoteID] = SyncRecord(date: now, success: success)
    }
}
