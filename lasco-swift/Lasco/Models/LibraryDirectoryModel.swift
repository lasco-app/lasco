import Foundation
import Observation

struct CreateLibraryResult: Sendable, Equatable {
    let libraryID: String
    let masterKey: String
}

enum LibraryDirectoryModelError: LocalizedError {
    case remoteUnavailableAfterRefresh

    var errorDescription: String? {
        switch self {
        case .remoteUnavailableAfterRefresh:
            "The remote was not available after refreshing the library session."
        }
    }
}

@MainActor
@Observable
final class LibraryDirectoryModel {
    private(set) var libraries: [FfiLibraryEntry] = []
    private(set) var librariesError: String?
    private(set) var activeSession: ActiveLibrarySession?
    private(set) var isOpen = false

    let onboarding: OnboardingCoordinator
    private let directory: LibraryDirectoryRepository

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
            await install(library: library, nickname: entry.nickname, username: await directory.storedUsername(libraryID: entry.id.value))
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
        let remainsInOnboarding = showOnboarding
        let created = try await directory.create(name: name, username: username, password: password)
        await install(
            library: created.library,
            nickname: name,
            username: username,
            markOpen: false,
            showingOnboarding: remainsInOnboarding
        )
        let result = CreateLibraryResult(libraryID: created.result.libraryId.value, masterKey: created.result.masterKeyHex)
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
        guard let activeSession else { return }
        try? await directory.clearSession(libraryID: activeSession.state.libraryID)
        await closeActive()
        showOnboarding = libraries.isEmpty
    }

    func deleteCurrentLibrary() async -> Bool {
        guard let activeSession else { return false }
        let libraryID = activeSession.state.libraryID
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
        if let activeSession { await activeSession.close() }
        activeSession = nil
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

    func completeOnboarding() {
        isOpen = true
        showOnboarding = false
    }

    func testS3Remote(endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws {
        try await directory.testS3Remote(endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey)
    }

    private func install(
        library: FfiLibrary,
        nickname: String,
        username: String?,
        markOpen: Bool = true,
        showingOnboarding: Bool = false
    ) async {
        if let activeSession {
            await activeSession.close()
        }
        let session = ActiveLibrarySession(library: library, nickname: nickname, username: username)
        activeSession = session
        do {
            try await session.refresh()
        } catch {
            AppLogger.log(.error, "initial session refresh failed: \(error)")
        }
        isOpen = markOpen
        await refreshLibraries()
        showOnboarding = showingOnboarding
    }
}
