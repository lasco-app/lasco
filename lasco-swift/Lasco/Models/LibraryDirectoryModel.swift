import Foundation
import Observation

struct CreateLibraryResult: Sendable, Equatable {
    let libraryID: String
    let masterKey: String
}

@MainActor
@Observable
final class LibraryDirectoryModel {
    private(set) var libraries: [FfiLibraryEntry] = []
    private(set) var librariesError: String?
    private(set) var activeRepository: LibraryRepository?
    private(set) var session: LibrarySessionState?
    private(set) var syncCoordinator: SyncCoordinator?
    private(set) var importCoordinator: ImportCoordinator?
    private(set) var isOpen = false

    let onboarding: OnboardingCoordinator
    private let directory: LibraryDirectoryRepository
    private var sessionTask: Task<Void, Never>?

    init(
        directory: LibraryDirectoryRepository = LibraryDirectoryRepository(),
        onboarding: OnboardingCoordinator? = nil
    ) {
        self.directory = directory
        self.onboarding = onboarding ?? OnboardingCoordinator()
    }

    var showOnboarding: Bool {
        get { onboarding.showOnboarding }
        set { onboarding.showOnboarding = newValue }
    }

    var openLibraryID: String? { session?.libraryID }
    var openNickname: String? { session?.nickname }
    var openUsername: String? { session?.username }

    func start() async {
        await refreshLibraries()
        showOnboarding = libraries.isEmpty
    }

    func refreshLibraries() async {
        do {
            let entries = try await directory.loadLibraries()
            libraries = entries
            librariesError = nil
            showOnboarding = entries.isEmpty
        } catch {
            libraries = []
            librariesError = error.localizedDescription
        }
    }

    func openCached(entry: FfiLibraryEntry) async -> Bool {
        do {
            guard let library = try await directory.openCached(entry: entry) else { return false }
            await install(library: library, nickname: entry.nickname, username: await directory.storedUsername(libraryID: entry.id))
            return true
        } catch {
            onboarding.setError(error)
            return false
        }
    }

    func open(nickname: String?, username: String, password: String) async -> Bool {
        do {
            let library = try await directory.open(nickname: nickname, username: username, password: password)
            await install(library: library, nickname: nickname ?? "", username: username)
            return true
        } catch {
            onboarding.setError(error)
            return false
        }
    }

    func create(name: String, username: String, password: String) async throws -> CreateLibraryResult {
        let created = try await directory.create(name: name, username: username, password: password)
        await install(library: created.library, nickname: name, username: username)
        let result = CreateLibraryResult(libraryID: created.result.libraryId, masterKey: created.result.masterKeyHex)
        setOnboardingStep(1, libraryID: result.libraryID)
        return result
    }

    func addExisting(
        nickname: String,
        username: String,
        password: String,
        newUsername: String?,
        newPassword: String?,
        remoteID: String,
        endpoint: String,
        bucket: String,
        region: String,
        pathPrefix: String,
        accessKey: String,
        secretKey: String
    ) async throws {
        let library = try await directory.addExisting(
            nickname: nickname,
            username: username,
            password: password,
            newUsername: newUsername,
            newPassword: newPassword,
            remoteID: remoteID,
            endpoint: endpoint,
            bucket: bucket,
            region: region,
            pathPrefix: pathPrefix,
            accessKey: accessKey,
            secretKey: secretKey
        )
        let effectiveUsername = newUsername?.isEmpty == false ? newUsername! : username
        await install(library: library, nickname: nickname, username: effectiveUsername)
    }

    func signOut() async {
        guard let session else { return }
        try? await directory.clearSession(libraryID: session.libraryID)
        await closeActive()
        showOnboarding = libraries.isEmpty
    }

    func deleteCurrentLibrary() async -> Bool {
        guard let session else { return false }
        let libraryID = session.libraryID
        await closeActive()
        do {
            try await directory.delete(libraryID: libraryID)
            await refreshLibraries()
            return true
        } catch {
            onboarding.setError(error)
            await refreshLibraries()
            return false
        }
    }

    func closeActive() async {
        sessionTask?.cancel()
        sessionTask = nil
        await syncCoordinator?.close()
        importCoordinator?.close()
        if let activeRepository { await activeRepository.close() }
        activeRepository = nil
        session = nil
        syncCoordinator = nil
        importCoordinator = nil
        isOpen = false
    }

    func onboardingStep(libraryID: String) -> Int? {
        UserDefaults.standard.object(forKey: "lasco.onboardingStep.\(libraryID)") as? Int
    }

    func setOnboardingStep(_ step: Int, libraryID: String) {
        UserDefaults.standard.set(step, forKey: "lasco.onboardingStep.\(libraryID)")
    }

    func clearOnboardingIncomplete(libraryID: String) {
        UserDefaults.standard.removeObject(forKey: "lasco.onboardingStep.\(libraryID)")
    }

    func testS3Remote(endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws {
        try await directory.testS3Remote(endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey)
    }

    private func install(library: FfiLibrary, nickname: String, username: String?) async {
        let repository = LibraryRepository(library: library)
        let state = LibrarySessionState(libraryID: library.libraryId(), nickname: nickname, username: username)
        let sync = SyncCoordinator(repository: repository, session: state)
        let importer = ImportCoordinator(repository: repository, session: state)
        activeRepository = repository
        session = state
        syncCoordinator = sync
        importCoordinator = importer
        isOpen = true
        showOnboarding = false
        sessionTask?.cancel()
        sessionTask = Task { [weak state] in
            guard let state else { return }
            await state.listen(using: repository)
        }
        await refreshLibraries()
    }
}
