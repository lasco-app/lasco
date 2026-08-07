import Foundation

@MainActor
final class ActiveLibrarySession {
    let repository: LibraryRepository
    let state: LibrarySessionState
    let syncCoordinator: SyncCoordinator
    let mediaImportCoordinator: MediaImportCoordinator
    let autoPhotoImportCoordinator: AutoPhotoImportCoordinator

    private var listenerTask: Task<Void, Never>?

    init(library: FfiLibrary, nickname: String, username: String?) {
        let repository = LibraryRepository(library: library)
        let state = LibrarySessionState(libraryID: library.libraryId().value, nickname: nickname, username: username)
        self.repository = repository
        self.state = state
        self.syncCoordinator = SyncCoordinator(repository: repository, session: state)
        self.mediaImportCoordinator = MediaImportCoordinator(repository: repository)
        self.autoPhotoImportCoordinator = AutoPhotoImportCoordinator(repository: repository, session: state)
        listenerTask = Task { [weak state, weak syncCoordinator] in
            guard let state, let syncCoordinator else { return }
            await state.listen(using: repository) {
                syncCoordinator.restorePersistedRecords(for: state.remotes)
            }
        }
    }

    func refresh() async throws {
        try await state.refresh(using: repository)
        syncCoordinator.restorePersistedRecords(for: state.remotes)
    }

    func close() async {
        // Stop every consumer of the repository before making it unavailable.
        listenerTask?.cancel()
        listenerTask = nil
        mediaImportCoordinator.close()
        autoPhotoImportCoordinator.close()
        await syncCoordinator.close()
        await repository.close()
    }
}
