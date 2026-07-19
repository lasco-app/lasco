import Foundation
import Combine

@MainActor
class LibraryModel: ObservableObject {
    // MARK: - Published state

    @Published var media: [FfiMediaItem] = []
    @Published var albums: [FfiAlbum] = []
    @Published var libraries: [FfiLibraryEntry] = []
    @Published var remotes: [FfiRemote] = []
    @Published var users: [String] = []
    @Published var isOpen = false
    @Published var showOnboarding = false
    @Published var resumeOnboardingLibraryId: String?
    @Published var resumeOnboardingStep: Int = 0
    @Published var error: String?
    @Published var librariesError: String?

    // Media
    @Published var isImporting = false
    @Published var isAutoImporting = false
    @Published var bulkImportProgress: (done: Int, total: Int)? = nil

    // Defaults
    @Published var defaultUploadAlbumId: String?
    @Published var defaultFetchRemoteId: String?
    @Published var autoImportDeviceMedia: Bool?

    // Sync
    @Published var pendingMasterKey: String? = nil

    @Published var nextPushDate: Date? = nil
    @Published var busyRemotes: Set<String> = []
    @Published var fetchInProgress = false
    @Published var lastPushRecords: [String: SyncRecord] = [:]
    @Published var lastFetchRecords: [String: SyncRecord] = [:]
    @Published var pendingMediaCount: Int = 0
    @Published var localStateStats: FfiLocalStateStats? = nil

    // MARK: - Private storage

    var lib: FfiLibrary?
    #if canImport(UIKit)
    let photoImporter = PhotoLibraryImporter()
    #endif
    var videoURLCache: [String: URL] = [:]
    var pushDebounceTask: Task<Void, Never>?

    // MARK: - App support dir

    /// Raw Application Support directory path, passed to FFI calls that need
    /// to resolve a debug_local_apple remote. lasco-core appends "lasco" and
    /// "local_fs_test" itself, so this must not include those components.
    var appSupportDirPath: String? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .path
    }

    // MARK: - Init

    init() {
        refreshLibraries()
        showOnboarding = libraries.isEmpty
        if let incomplete = libraries.first(where: { onboardingStep(libraryId: $0.id) != nil }) {
            switch reopenForOnboardingResume(entry: incomplete) {
            case .opened:
                resumeOnboardingLibraryId = incomplete.id
                var step = onboardingStep(libraryId: incomplete.id) ?? 0
                if step == 1 { step = 2 }
                resumeOnboardingStep = step
                showOnboarding = true
            case .noSession, .failed:
                AppLogger.log(.error, "resume: could not reopen incomplete library '\(incomplete.nickname)', dropping resume marker")
                clearOnboardingIncomplete(libraryId: incomplete.id)
            }
        }
    }

    // MARK: - Library list

    func refreshLibraries() {
        AppLogger.log(.debug, "refreshing library list")
        do {
            libraries = try listLibraries()
            librariesError = nil
        } catch {
            AppLogger.log(.error, "refreshLibraries failed: \(error)")
            libraries = []
            librariesError = error.localizedDescription
        }
    }

    var librariesHaveErrors: Bool {
        libraries.contains { $0.loadError != nil }
    }

    var openNickname: String? {
        guard let lib else { return nil }
        return libraries.first { $0.id == lib.libraryId() }?.nickname
    }

    var openLibraryId: String? {
        lib?.libraryId()
    }

    var openUsername: String? {
        guard let libId = lib?.libraryId() else { return nil }
        return storedUsername(libraryId: libId)
    }

    // MARK: - Open / create / close

    enum OpenCachedResult {
        case opened
        case noSession
        case failed(String)
    }

    func openCached(entry: FfiLibraryEntry) -> OpenCachedResult {
        let result = openCachedInternal(entry: entry)
        if case .opened = result { isOpen = true }
        return result
    }

    private func reopenForOnboardingResume(entry: FfiLibraryEntry) -> OpenCachedResult {
        openCachedInternal(entry: entry)
    }

    private func openCachedInternal(entry: FfiLibraryEntry) -> OpenCachedResult {
        guard let username = storedUsername(libraryId: entry.id), !username.isEmpty else {
            AppLogger.log(.debug, "no cached session for library '\(entry.nickname)'")
            return .noSession
        }
        AppLogger.log(.info, "opening library '\(entry.nickname)' from cache for user '\(username)'")
        error = nil
        do {
            guard let library = try ffiOpenCached(nickname: entry.nickname, username: username) else {
                AppLogger.log(.debug, "no cached session for library '\(entry.nickname)'")
                return .noSession
            }
            lib = library
            reload()
            refreshLibraries()
            AppLogger.log(.info, "library opened from cache")
            return .opened
        } catch {
            AppLogger.log(.error, "openCached '\(entry.nickname)' failed: \(error)")
            return .failed(error.localizedDescription)
        }
    }

    func open(nickname: String?, username: String, password: String) {
        AppLogger.log(.info, "opening library '\(nickname ?? "default")' for user '\(username)'")
        error = nil
        do {
            lib = try FfiLibrary.open(nickname: nickname, username: username, password: password)
            if let libId = lib?.libraryId() {
                storeUsername(username, libraryId: libId)
            }
            reload()
            refreshLibraries()
            isOpen = true
            AppLogger.log(.info, "library opened")
        } catch let e as LascoError {
            AppLogger.log(.error, "open '\(nickname ?? "default")' failed: \(e)")
            error = e.friendlyMessage
        } catch {
            AppLogger.log(.error, "open '\(nickname ?? "default")' failed: \(error)")
            self.error = error.localizedDescription
        }
    }

    func create(name: String, username: String, password: String) {
        AppLogger.log(.info, "creating library '\(name)'")
        error = nil
        do {
            let result = try ffiCreateLibrary(nickname: name, username: username, password: password)
            pendingMasterKey = result.masterKeyHex
            lib = try FfiLibrary.open(nickname: name, username: username, password: password)
            if let libId = lib?.libraryId() {
                storeUsername(username, libraryId: libId)
                setOnboardingStep(1, libraryId: libId)
            }
            reload()
            refreshLibraries()
            AppLogger.log(.info, "library created and opened")
        } catch let e as LascoError {
            AppLogger.log(.error, "create '\(name)' failed: \(e)")
            error = e.friendlyMessage
        } catch {
            AppLogger.log(.error, "create '\(name)' failed: \(error)")
            self.error = error.localizedDescription
        }
    }

    @discardableResult
    func addExisting(
        nickname: String,
        username: String,
        password: String,
        newUsername: String?,
        newPassword: String?,
        remoteId: String,
        endpoint: String,
        bucket: String,
        region: String,
        pathPrefix: String,
        accessKey: String,
        secretKey: String
    ) -> Bool {
        AppLogger.log(.info, "adding existing library '\(nickname)' from remote")
        error = nil
        do {
            let library = try ffiAddExistingLibraryS3(
                nickname: nickname,
                username: username,
                password: password,
                newUsername: newUsername,
                newPassword: newPassword,
                remoteId: remoteId,
                endpoint: endpoint,
                bucket: bucket,
                region: region,
                pathPrefix: pathPrefix,
                accessKey: accessKey,
                secretKey: secretKey
            )
            lib = library
            let effectiveUser = (newUsername?.isEmpty == false) ? newUsername! : username
            storeUsername(effectiveUser, libraryId: library.libraryId())
            try lib?.loadLocalState()
            reload()
            refreshLibraries()
            isOpen = true
            AppLogger.log(.info, "existing library added and opened")
            return true
        } catch let e as LascoError {
            AppLogger.log(.error, "addExisting '\(nickname)' failed: \(e)")
            error = e.friendlyMessage
            return false
        } catch {
            AppLogger.log(.error, "addExisting '\(nickname)' failed: \(error)")
            self.error = error.localizedDescription
            return false
        }
    }

    func signOut() {
        guard let lib else { return }
        let libId = lib.libraryId()
        let username = storedUsername(libraryId: libId) ?? ""
        try? sessionClear(libraryId: libId, username: username)
        self.lib = nil
        media = []
        albums = []
        users = []
        defaultUploadAlbumId = nil
        defaultFetchRemoteId = nil
        lastPushRecords = [:]
        lastFetchRecords = [:]
        isOpen = false
        showOnboarding = libraries.isEmpty
    }

    @discardableResult
    func deleteCurrentLibrary() -> Bool {
        guard let libId = lib?.libraryId() else { return false }
        AppLogger.log(.info, "deleting library \(libId)")
        defer { showOnboarding = libraries.isEmpty }
        error = nil
        self.lib = nil
        media = []
        albums = []
        remotes = []
        users = []
        defaultUploadAlbumId = nil
        defaultFetchRemoteId = nil
        lastPushRecords = [:]
        lastFetchRecords = [:]
        isOpen = false
        UserDefaults.standard.removeObject(forKey: "lasco.lastUsername.\(libId)")
        do {
            try ffiDeleteLibrary(libraryId: libId)
            AppLogger.log(.info, "library deleted")
            refreshLibraries()
            return true
        } catch let e as LascoError {
            AppLogger.log(.error, "deleteCurrentLibrary failed: \(e)")
            error = e.friendlyMessage
            refreshLibraries()
            return false
        } catch {
            AppLogger.log(.error, "deleteCurrentLibrary failed: \(error)")
            self.error = error.localizedDescription
            refreshLibraries()
            return false
        }
    }

    // MARK: - State reload

    func reload() {
        do {
            media = try lib?.mediaByDate() ?? []
        } catch {
            AppLogger.log(.error, "reload mediaByDate failed: \(error)")
            media = []
        }
        do {
            albums = try lib?.listAlbums() ?? []
        } catch {
            AppLogger.log(.error, "reload listAlbums failed: \(error)")
            albums = []
        }
        remotes = lib?.listRemotes() ?? []
        pendingMediaCount = Int(lib?.pendingMediaCount() ?? 0)
        localStateStats = lib?.localStateStats()
        defaultUploadAlbumId = lib?.getDefaultUploadAlbum()
        defaultFetchRemoteId = lib?.getDefaultFetchRemote()
        autoImportDeviceMedia = lib?.getAutoImportDeviceMedia()
        for remote in remotes {
            let ud = UserDefaults.standard
            if let d = ud.object(forKey: "lasco.lastPush.\(remote.id)") as? Date {
                let ok = ud.bool(forKey: "lasco.lastPushOk.\(remote.id)")
                lastPushRecords[remote.id] = SyncRecord(date: d, success: ok)
            }
            if let d = ud.object(forKey: "lasco.lastFetch.\(remote.id)") as? Date {
                let ok = ud.bool(forKey: "lasco.lastFetchOk.\(remote.id)")
                lastFetchRecords[remote.id] = SyncRecord(date: d, success: ok)
            }
        }
        refreshUsers()
        AppLogger.log(.debug, "reloading local state — \(media.count) media, \(albums.count) albums")
    }

    /// Returns the count of local media not backed up on any remote, or nil if all
    /// local media is safely backed up and cleaning can proceed.
    func mediaCountWithoutRemoteBackup() -> Int? {
        let unbacked = (try? lib?.mediaIdsWithoutRemoteBackup()) ?? []
        return unbacked.isEmpty ? nil : unbacked.count
    }

    func cleanLocalMedia() {
        let ids = lib?.allMediaIds() ?? []
        try? lib?.evictLocalData(mediaIds: ids)
        localStateStats = lib?.localStateStats()
    }

    func cleanLocalThumbnails() {
        let ids = lib?.allMediaIds() ?? []
        try? lib?.evictLocalThumbnails(mediaIds: ids)
        localStateStats = lib?.localStateStats()
    }

    func refreshUsers() {
        users = (try? lib?.userList()) ?? []
    }

    func addUser(username: String, password: String) -> String? {
        AppLogger.log(.info, "adding user '\(username)'")
        do {
            try lib?.userAdd(username: username, password: password)
            refreshUsers()
            return nil
        } catch let e as LascoError {
            AppLogger.log(.error, "addUser '\(username)' failed: \(e)")
            return e.friendlyMessage
        } catch {
            AppLogger.log(.error, "addUser '\(username)' failed: \(error)")
            return error.localizedDescription
        }
    }

    // MARK: - Credentials

    func storedUsername(libraryId: String) -> String? {
        UserDefaults.standard.string(forKey: "lasco.lastUsername.\(libraryId)")
    }

    func storeUsername(_ username: String, libraryId: String) {
        UserDefaults.standard.set(username, forKey: "lasco.lastUsername.\(libraryId)")
    }

    func onboardingStep(libraryId: String) -> Int? {
        UserDefaults.standard.object(forKey: "lasco.onboardingStep.\(libraryId)") as? Int
    }

    func setOnboardingStep(_ step: Int, libraryId: String) {
        UserDefaults.standard.set(step, forKey: "lasco.onboardingStep.\(libraryId)")
    }

    func clearOnboardingIncomplete(libraryId: String) {
        UserDefaults.standard.removeObject(forKey: "lasco.onboardingStep.\(libraryId)")
    }
}
