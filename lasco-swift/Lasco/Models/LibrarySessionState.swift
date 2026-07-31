import Foundation
import Observation

@MainActor
@Observable
final class LibrarySessionState {
    private(set) var libraryID: String
    private(set) var nickname: String
    private(set) var username: String?
    private(set) var users: [String] = []
    private(set) var remotes: [FfiRemote] = []
    private(set) var defaultFetchRemoteID: String?
    private(set) var autoImportDeviceMedia = false

    init(libraryID: String, nickname: String, username: String?) {
        self.libraryID = libraryID
        self.nickname = nickname
        self.username = username
    }

    func refresh(using repository: any LibraryRepositoryProtocol) async throws {
        let snapshot = try await repository.sessionSnapshot()
        users = snapshot.users
        remotes = snapshot.remotes
        defaultFetchRemoteID = snapshot.defaultFetchRemoteID
        autoImportDeviceMedia = snapshot.autoImportDeviceMedia
    }

    func listen(
        using repository: any LibraryRepositoryProtocol,
        onRefresh: @MainActor @escaping () -> Void
    ) async {
        let stream = await repository.changes()
        do {
            try await refresh(using: repository)
            onRefresh()
        } catch is CancellationError {
            return
        } catch {
            AppLogger.log(.error, "session refresh failed: \(error)")
        }

        for await change in stream {
            guard change == .session || change == .all else { continue }
            do {
                try await refresh(using: repository)
                onRefresh()
            } catch is CancellationError {
                return
            } catch {
                AppLogger.log(.error, "session refresh failed: \(error)")
            }
        }
    }
}
