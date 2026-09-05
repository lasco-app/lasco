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
        guard let username = storedUsername(libraryID: entry.libraryId), !username.isEmpty else { return nil }
        return try ffiOpenCached(nickname: entry.nickname, username: username, appDir: appSupportDirectory)
    }

    func open(nickname: String?, username: String, password: String) throws -> FfiLibrary {
        let library = try FfiLibrary.open(nickname: nickname, username: username, password: password, appDir: appSupportDirectory)
        storeUsername(username, libraryID: library.libraryId())
        return library
    }

    func recoverCRDTState(nickname: String, username: String, password: String) throws {
        try ffiRecoverLibraryState(nickname: nickname, username: username, password: password, appDir: appSupportDirectory)
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
            remoteName: remoteID,
            endpoint: endpoint,
            bucket: bucket,
            region: region,
            pathPrefix: pathPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
            appDir: appSupportDirectory
        )
        let effectiveUsername = newUsername?.isEmpty == false ? newUsername! : username
        storeUsername(effectiveUsername, libraryID: library.libraryId())
        try library.loadLocalState()
        return library
    }

    func addExistingLascoCloud(
        nickname: String,
        username: String,
        password: String,
        newUsername: String?,
        newPassword: String?,
        cloudEmail: String,
        cloudPassword: String
    ) throws -> FfiLibrary {
        let library = try ffiAddExistingLibraryLascoCloud(
            config: FfiLascoCloudImportConfig(
                nickname: nickname,
                username: username,
                password: password,
                newUsername: newUsername,
                newPassword: newPassword,
                cloudBaseUrl: LascoCloudEndpoint.url,
                cloudEmail: cloudEmail,
                cloudPassword: cloudPassword,
                platform: "ios",
                appVersion: LascoCloudEndpoint.appVersion
            ),
            appDir: appSupportDirectory
        )
        let effectiveUsername = newUsername?.isEmpty == false ? newUsername! : username
        storeUsername(effectiveUsername, libraryID: library.libraryId())
        try library.loadLocalState()
        return library
    }

    func delete(libraryID: FfiLibraryId) throws {
        try ffiDeleteLibrary(libraryId: libraryID, appDir: appSupportDirectory)
        UserDefaults.standard.removeObject(forKey: "lasco.lastUsername.\(libraryID.value)")
    }

    func clearSession(libraryID: FfiLibraryId) throws {
        let username = storedUsername(libraryID: libraryID) ?? ""
        try sessionClear(libraryId: libraryID, username: username, appDir: appSupportDirectory)
    }

    func testS3Remote(endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) throws {
        try ffiTestS3Remote(endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey)
    }

    func storedUsername(libraryID: FfiLibraryId) -> String? {
        UserDefaults.standard.string(forKey: "lasco.lastUsername.\(libraryID.value)")
    }

    func storeUsername(_ username: String, libraryID: FfiLibraryId) {
        UserDefaults.standard.set(username, forKey: "lasco.lastUsername.\(libraryID.value)")
    }
}
