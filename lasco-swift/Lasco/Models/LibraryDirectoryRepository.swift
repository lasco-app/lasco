import Foundation

actor LibraryDirectoryRepository {
    /// The FFI app-data root. This must match `lasco_core::default_app_dir()`
    /// on Apple platforms (`Application Support/lasco`), not the raw
    /// Application Support directory.
    let appSupportDirectory: String?

    init(appSupportDirectory: String? = nil) {
        if let appSupportDirectory {
            self.appSupportDirectory = appSupportDirectory
        } else {
            self.appSupportDirectory = FileManager.default.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first?
                .appendingPathComponent("lasco", isDirectory: true)
                .path
        }
    }

    func loadLibraries() throws -> [FfiLibraryEntry] {
        try listLibraries(appDir: appSupportDirectory)
    }

    func openCached(entry: FfiLibraryEntry) throws -> FfiLibrary? {
        guard let username = storedUsername(libraryID: entry.libraryId.value), !username.isEmpty else { return nil }
        return try ffiOpenCached(nickname: entry.nickname, username: username, appDir: appSupportDirectory)
    }

    func open(nickname: String?, username: String, password: String) throws -> FfiLibrary {
        let library = try FfiLibrary.open(nickname: nickname, username: username, password: password, appDir: appSupportDirectory)
        storeUsername(username, libraryID: library.libraryId().value)
        return library
    }

    func create(name: String, username: String, password: String) throws -> (library: FfiLibrary, result: FfiCreateLibraryResult) {
        let result = try ffiCreateLibrary(nickname: name, username: username, password: password, appDir: appSupportDirectory)
        let library = try open(nickname: name, username: username, password: password)
        return (library, result)
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
    ) throws -> FfiLibrary {
        let library = try ffiAddExistingLibraryS3(
            nickname: nickname,
            username: username,
            password: password,
            newUsername: newUsername,
            newPassword: newPassword,
            remoteId: remoteID,
            endpoint: endpoint,
            bucket: bucket,
            region: region,
            pathPrefix: pathPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
            appDir: appSupportDirectory
        )
        let effectiveUsername = newUsername?.isEmpty == false ? newUsername! : username
        storeUsername(effectiveUsername, libraryID: library.libraryId().value)
        try library.loadLocalState()
        return library
    }

    func delete(libraryID: String) throws {
        try ffiDeleteLibrary(libraryId: FfiLibraryId(value: libraryID), appDir: appSupportDirectory)
        UserDefaults.standard.removeObject(forKey: "lasco.lastUsername.\(libraryID)")
    }

    func clearSession(libraryID: String) throws {
        let username = storedUsername(libraryID: libraryID) ?? ""
        try sessionClear(libraryId: FfiLibraryId(value: libraryID), username: username, appDir: appSupportDirectory)
    }

    func testS3Remote(endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) throws {
        try ffiTestS3Remote(endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey)
    }

    func storedUsername(libraryID: String) -> String? {
        UserDefaults.standard.string(forKey: "lasco.lastUsername.\(libraryID)")
    }

    func storeUsername(_ username: String, libraryID: String) {
        UserDefaults.standard.set(username, forKey: "lasco.lastUsername.\(libraryID)")
    }
}
