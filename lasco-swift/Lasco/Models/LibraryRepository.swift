import Foundation
import Observation

struct LibrarySessionSnapshot: Sendable, Equatable {
    let users: [String]
    let remotes: [FfiRemote]
    let mediaSourceOrder: [FfiRemoteUuid]
    let defaultFetchRemoteID: FfiRemoteUuid?
    let autoImportDeviceMedia: Bool
}

struct MediaImportSource: Sendable {
    let path: String
    let originalFilename: String?
    let appleAaeMediaID: FfiMediaUuid?
    let appleLivePhotoMediaID: FfiMediaUuid?

    init(
        path: String,
        originalFilename: String? = nil,
        appleAaeMediaID: FfiMediaUuid? = nil,
        appleLivePhotoMediaID: FfiMediaUuid? = nil
    ) {
        self.path = path
        self.originalFilename = originalFilename
        self.appleAaeMediaID = appleAaeMediaID
        self.appleLivePhotoMediaID = appleLivePhotoMediaID
    }
}

protocol LibraryRepositoryProtocol: Sendable {
    func changes() async -> AsyncStream<LibraryChange>
    func notifyChanged(_ change: LibraryChange) async
    func notifyPhotoImportChanged(initialImport: Bool) async

    func mediaByDateCount() async throws -> Int
    func mediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem]
    func mediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors
    func orphanMediaByDateCount() async throws -> Int
    func orphanMediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem]
    func orphanMediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors
    func albumsCount(parentID: FfiAlbumUuid?) async throws -> Int
    func albums(parentID: FfiAlbumUuid?, offset: Int, limit: Int) async throws -> [FfiAlbum]
    func albums(withIDs ids: Set<FfiAlbumUuid>) async throws -> [FfiAlbum]
    func albumItemsCount(albumID: FfiAlbumUuid) async throws -> Int
    func albumItems(albumID: FfiAlbumUuid, ascending: Bool, offset: Int, limit: Int) async throws -> [FfiAlbumItem]
    func albumItemsByDateNeighbors(albumID: FfiAlbumUuid, ascending: Bool, position: Int) async throws -> FfiMediaOrGroupNeighbors
    func showMedia(id: FfiMediaUuid) async throws -> FfiMediaItem
    func localStateStats() async throws -> FfiLocalStateStats
    func sessionSnapshot() async throws -> LibrarySessionSnapshot
    func mediaInAlbum(albumID: FfiAlbumUuid) async throws -> [FfiMediaItem]
    func mediaContainingAlbumIDs(mediaID: FfiMediaUuid, includeViaGroups: Bool) async throws -> [FfiAlbumUuid]
    func mediaAlbumIDs(mediaID: FfiMediaUuid) async throws -> [FfiAlbumUuid]
    func albumGroups(albumID: FfiAlbumUuid) async throws -> [FfiGroup]
    func groupMedia(groupID: FfiGroupUuid) async throws -> [FfiMediaItem]
    func listOperations(startPos: UInt64, endPosExclusive: UInt64) async throws -> [FfiCrdtOperation]

    func thumbnail(mediaID: FfiMediaUuid) async throws -> Data
    func mediaBytes(mediaID: FfiMediaUuid) async throws -> Data
    func thumbnailAsync(mediaID: FfiMediaUuid) async throws -> Data
    func mediaBytesAsync(mediaID: FfiMediaUuid) async throws -> Data
    func nativeMediaBytesAsync(mediaID: FfiMediaUuid) async throws -> Data
    func materializedMediaURL(mediaID: FfiMediaUuid, originalFilename: String) async throws -> URL

    func renameMedia(id: FfiMediaUuid, name: String?) async throws
    func deleteMedia(id: FfiMediaUuid) async throws
    func addMediaToAlbum(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws
    func addMediaToAlbumWithoutNotification(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws
    func removeMediaFromAlbum(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws
    func moveMedia(id: FfiMediaUuid, from: FfiAlbumUuid, to: FfiAlbumUuid) async throws
    func moveMediaWithoutNotification(id: FfiMediaUuid, from: FfiAlbumUuid, to: FfiAlbumUuid) async throws
    func createAlbum(name: String, parentID: FfiAlbumUuid?) async throws -> FfiAlbumUuid
    func createAlbumWithoutNotification(name: String, parentID: FfiAlbumUuid?) async throws -> FfiAlbumUuid
    func renameAlbum(id: FfiAlbumUuid, name: String) async throws
    func reparentAlbum(id: FfiAlbumUuid, parentID: FfiAlbumUuid?) async throws
    func deleteAlbum(id: FfiAlbumUuid) async throws
    func setAlbumThumbnail(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid?) async throws
    func createGroup(albumID: FfiAlbumUuid) async throws -> FfiGroupUuid
    func deleteGroup(groupID: FfiGroupUuid) async throws
    func addMediaToGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) async throws
    func removeMediaFromGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) async throws
    func createGroupFromSelectedMedia(mediaIDs: [FfiMediaUuid], albumID: FfiAlbumUuid) async throws

    func importMedia(source: MediaImportSource, albumID: FfiAlbumUuid?) async throws -> FfiMediaUuid
    func importMediaWithoutNotification(source: MediaImportSource, albumID: FfiAlbumUuid?) async throws -> FfiMediaUuid
    func importMediaBatch(_ sources: [MediaImportSource], albumID: FfiAlbumUuid?) async throws -> [FfiMediaUuid]
    func setMediaThumbnail(mediaID: FfiMediaUuid, data: Data) async throws
    func evictLocalData(mediaIDs: [FfiMediaUuid]) async throws
    func evictLocalThumbnails(mediaIDs: [FfiMediaUuid]) async throws
    func mediaCountLostIfLocalMediaCleared() async throws -> Int
    func mediaCountLostIfRemoteRemoved(remoteID: FfiRemoteUuid) async throws -> Int
    func remoteMediaShortfall(remoteID: FfiRemoteUuid) async throws -> FfiRemoteMediaShortfall
    func allMediaIDs() async -> [FfiMediaUuid]

    func setDefaultFetchRemote(remoteID: FfiRemoteUuid?) async throws
    func setRemoteAutoPush(remoteID: FfiRemoteUuid, enabled: Bool) async throws
    func setMediaSourceOrder(remoteIDs: [FfiRemoteUuid]) async throws
    func setAutoImportDeviceMedia(enabled: Bool) async throws
    func addUser(username: String, password: String) async throws
    func addRemoteFixedPath(name: String, path: String) async throws -> FfiRemoteUuid
    func addRemoteDebugLocalApple(name: String) async throws -> FfiRemoteUuid
    func addRemoteS3(id: String, endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws -> FfiRemoteUuid
    func removeRemote(id: FfiRemoteUuid) async throws
    func initializeRemote(id: FfiRemoteUuid) async throws
    func connectRemote(id: FfiRemoteUuid) async throws
    func hasUnpushedChanges(remoteID: FfiRemoteUuid) async -> Bool
    func inspectCompactionLock(remoteID: FfiRemoteUuid) async throws -> FfiCompactionLockInfo?
    func removeOwnCompactionLock(remoteID: FfiRemoteUuid) async throws -> Bool

    func push(remoteID: FfiRemoteUuid, progress: any PushProgressSink) async throws -> UInt64
    func fetch(remoteID: FfiRemoteUuid) async throws -> UInt64
    func confirmRemoteMedia(remoteID: FfiRemoteUuid) async throws -> UInt64
    func close() async
}

enum LibraryRepositoryError: LocalizedError {
    case closed
    case invalidNativeMediaBuffer
    case cloudRemoteAlreadyAssociated
    case cloudSignOutRequiresRemoteRemoval
    case cloudAlreadyConnected

    var errorDescription: String? {
        switch self {
        case .closed:
            "The library session is closed."
        case .invalidNativeMediaBuffer:
            "The native media buffer is invalid."
        case .cloudRemoteAlreadyAssociated:
            "Lasco Cloud storage is already associated with another library"
        case .cloudSignOutRequiresRemoteRemoval:
            "Remove the Lasco Cloud remotes before signing out"
        case .cloudAlreadyConnected:
            "Lasco Cloud is already connected for this library"
        }
    }
}

private actor LibraryRepositoryStorage: LibraryRepositoryProtocol {
    private let library: FfiLibrary
    private let changeHub = LibraryChangeHub()
    private var closed = false
    private var materializingMedia: [String: Task<URL, Error>] = [:]

    let libraryID: FfiLibraryId
    let appSupportDirectory: String?

    init(library: FfiLibrary, appSupportDirectory: String? = nil) {
        self.library = library
        self.libraryID = library.libraryId()
        self.appSupportDirectory = appSupportDirectory ?? Self.defaultAppSupportDirectory
    }

    func configureLascoCloud(_ remotes: [LascoCloudRemote]) async throws {
        try ensureOpen()
        let existing = try validateLascoCloud(remotes)
        for remote in remotes {
            let remoteID: FfiRemoteUuid
            if let configuredID = existing[remote.id] {
                remoteID = configuredID
            } else {
                remoteID = try library.addRemoteCloudS3(name: remote.name, cloudStorageId: remote.id)
            }
            try library.initializeRemote(remoteId: remoteID, appSupportDir: appSupportDirectory)
            try library.connectRemote(remoteId: remoteID, appSupportDir: appSupportDirectory)
        }
        await notify(.session)
    }

    func configureLascoCloudAuth() async throws {
        try ensureOpen()
        try await library.configureLascoCloudAuth(baseUrl: LascoCloudEndpoint.url)
    }

    func lascoCloudLogin(email: String, password: String) async throws {
        try ensureOpen()
        try await library.lascoCloudLogin(
            email: email,
            password: password,
            platform: "ios",
            appVersion: LascoCloudEndpoint.appVersion
        )
    }

    func lascoCloudRemotes() async throws -> [LascoCloudRemote] {
        try ensureOpen()
        return try await library.lascoCloudListRemotes().map { LascoCloudRemote(ffi: $0) }
    }

    func assignLascoCloudRemotesToThisLibrary(remoteIDs: [String]) async throws {
        try ensureOpen()
        try await library.lascoCloudAssignRemotesToThisLibrary(remoteIds: remoteIDs)
    }

    func lascoCloudAccount() async throws -> LascoCloudAccount {
        try ensureOpen()
        return LascoCloudAccount(ffi: try await library.lascoCloudSubscription())
    }

    func isLascoCloudAuthenticated() -> Bool {
        !closed && library.lascoCloudIsAuthenticated()
    }

    func revokeLascoCloudSession() async throws {
        try ensureOpen()
        try await library.lascoCloudRevokeSession()
    }

    func clearLascoCloudAuthAndCredentials() async throws {
        try ensureOpen()
        try await library.clearLascoCloudAuthAndCredentials()
    }

    func validateLascoCloud(_ remotes: [LascoCloudRemote]) throws -> [String: FfiRemoteUuid] {
        try ensureOpen()
        guard remotes.count == 2 else { throw LascoCloudError.invalidRemoteCount }
        let existing = Dictionary(uniqueKeysWithValues: library.listRemotes().filter { $0.kind == "lasco_cloud_s3" }.compactMap { remote in remote.path.map { ($0, remote.remoteId) } })
        guard remotes.allSatisfy({ remote in
            remote.libraryID == nil || (remote.libraryID == libraryID.value && existing[remote.id] != nil)
        }) else {
            throw LibraryRepositoryError.cloudRemoteAlreadyAssociated
        }
        return existing
    }

    nonisolated static var defaultAppSupportDirectory: String? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?.path
    }

    /// Converts Swift's offset/limit convention to the FFI's inclusive positions.
    /// Keeping this conversion here prevents off-by-one and empty-range calls from
    /// leaking into views and feature models.
    private func page<Element>(
        offset: Int,
        limit: Int,
        _ fetch: (UInt32, UInt32) throws -> [Element]
    ) throws -> [Element] {
        guard offset >= 0, limit > 0 else { return [] }
        let start = UInt64(offset)
        let end = start + UInt64(limit) - 1
        guard end <= UInt64(UInt32.max) else { return [] }
        return try fetch(UInt32(start), UInt32(end))
    }

    nonisolated private static let pageSize = 100

    func changes() async -> AsyncStream<LibraryChange> {
        await changeHub.changes()
    }

    func notifyChanged(_ change: LibraryChange) async {
        await changeHub.notify(change)
    }

    func notifyPhotoImportChanged(initialImport: Bool) async {
        await notify(.mediaList)
        await notify(.albumList)
        // Initial import is explicitly pushed chunk-by-chunk. Auto import is a normal
        // local mutation and intentionally goes through SyncCoordinator's debounce.
        if !initialImport {
            await notify(.localMutation)
        }
    }

    func mediaByDateCount() async throws -> Int {
        try ensureOpen()
        return Int(library.mediaByDateCount())
    }

    func mediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem] {
        try ensureOpen()
        return try page(offset: offset, limit: limit) { start, end in
            try library.mediaByDateRange(posStartInclusive: start, posEndInclusive: end)
        }
    }

    func mediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors {
        try ensureOpen()
        guard position >= 0, position <= Int(UInt32.max) else { throw LascoError.NotFound }
        return try library.mediaByDateNeighbors(position: UInt32(position))
    }

    func orphanMediaByDateCount() async throws -> Int {
        try ensureOpen()
        return Int(library.orphanMediaByDateCount())
    }

    func orphanMediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem] {
        try ensureOpen()
        return try page(offset: offset, limit: limit) { start, end in
            try library.orphanMediaByDateRange(posStartInclusive: start, posEndInclusive: end)
        }
    }

    func orphanMediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors {
        try ensureOpen()
        guard position >= 0, position <= Int(UInt32.max) else { throw LascoError.NotFound }
        return try library.orphanMediaByDateNeighbors(position: UInt32(position))
    }

    func albumsCount(parentID: FfiAlbumUuid?) async throws -> Int {
        try ensureOpen()
        return Int(try library.albumAlbumsCount(parentAlbumId: parentID))
    }

    func albums(parentID: FfiAlbumUuid?, offset: Int, limit: Int) async throws -> [FfiAlbum] {
        try ensureOpen()
        return try page(offset: offset, limit: limit) { start, end in
            try library.albumAlbumsRange(
                parentAlbumId: parentID,
                posStartInclusive: start,
                posEndInclusive: end
            )
        }
    }

    func albums(withIDs ids: Set<FfiAlbumUuid>) async throws -> [FfiAlbum] {
        try ensureOpen()
        guard !ids.isEmpty else { return [] }

        var results: [FfiAlbum] = []
        var parents: [FfiAlbumUuid?] = [nil]
        var visitedParents = Set<FfiAlbumUuid?>()
        while !parents.isEmpty {
            try Task.checkCancellation()
            let parentID = parents.removeLast()
            guard visitedParents.insert(parentID).inserted else { continue }
            let count = Int(try library.albumAlbumsCount(parentAlbumId: parentID))
            for offset in stride(from: 0, to: count, by: Self.pageSize) {
                try Task.checkCancellation()
                let children = try page(offset: offset, limit: Self.pageSize) { start, end in
                    try library.albumAlbumsRange(
                        parentAlbumId: parentID,
                        posStartInclusive: start,
                        posEndInclusive: end
                    )
                }
                for album in children {
                    if ids.contains(album.albumId) { results.append(album) }
                    parents.append(album.albumId)
                }
            }
            if results.count == ids.count { break }
        }
        return results
    }

    func albumItemsCount(albumID: FfiAlbumUuid) async throws -> Int {
        try ensureOpen()
        return Int(try library.albumItemsCount(albumId: albumID))
    }

    func albumItems(albumID: FfiAlbumUuid, ascending: Bool, offset: Int, limit: Int) async throws -> [FfiAlbumItem] {
        try ensureOpen()
        return try page(offset: offset, limit: limit) { start, end in
            try library.albumItemsByDateRange(
                albumId: albumID,
                ascending: ascending,
                posStartInclusive: start,
                posEndInclusive: end
            )
        }
    }

    func albumItemsByDateNeighbors(
        albumID: FfiAlbumUuid,
        ascending: Bool,
        position: Int
    ) async throws -> FfiMediaOrGroupNeighbors {
        try ensureOpen()
        guard position >= 0, position <= Int(UInt32.max) else { throw LascoError.NotFound }
        return try library.albumItemsByDateNeighbors(
            albumId: albumID,
            ascending: ascending,
            position: UInt32(position)
        )
    }

    func showMedia(id: FfiMediaUuid) async throws -> FfiMediaItem {
        try ensureOpen()
        return try library.showMedia(mediaId: id)
    }

    func localStateStats() async throws -> FfiLocalStateStats {
        try ensureOpen()
        return library.localStateStats()
    }

    func sessionSnapshot() async throws -> LibrarySessionSnapshot {
        try ensureOpen()
        return LibrarySessionSnapshot(
            users: try library.userList(),
            remotes: library.listRemotes(),
            mediaSourceOrder: try library.getMediaSourceOrder(),
            defaultFetchRemoteID: library.getDefaultFetchRemote(),
            autoImportDeviceMedia: library.getAutoImportDeviceMedia()
        )
    }

    func mediaInAlbum(albumID: FfiAlbumUuid) async throws -> [FfiMediaItem] {
        try ensureOpen()
        return try library.mediaInAlbum(albumId: albumID)
    }

    func mediaContainingAlbumIDs(mediaID: FfiMediaUuid, includeViaGroups: Bool) async throws -> [FfiAlbumUuid] {
        try ensureOpen()
        return try library.mediaContainingAlbumIds(mediaId: mediaID, includeViaGroups: includeViaGroups)
    }

    func mediaAlbumIDs(mediaID: FfiMediaUuid) async throws -> [FfiAlbumUuid] {
        try ensureOpen()
        return try library.mediaAlbumIds(mediaId: mediaID)
    }

    func albumGroups(albumID: FfiAlbumUuid) async throws -> [FfiGroup] {
        try ensureOpen()
        return try library.albumListGroups(albumId: albumID)
    }

    func groupMedia(groupID: FfiGroupUuid) async throws -> [FfiMediaItem] {
        try ensureOpen()
        return try library.groupListMedia(groupId: groupID)
    }

    func listOperations(startPos: UInt64, endPosExclusive: UInt64) async throws -> [FfiCrdtOperation] {
        try ensureOpen()
        return try library.listOperations(startPos: startPos, endPosExclusive: endPosExclusive)
    }

    func thumbnail(mediaID: FfiMediaUuid) async throws -> Data {
        try ensureOpen()
        return try library.getMediaThumbnail(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    func mediaBytes(mediaID: FfiMediaUuid) async throws -> Data {
        try ensureOpen()
        return try library.getMediaBytes(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    func thumbnailAsync(mediaID: FfiMediaUuid) async throws -> Data {
        try ensureOpen()
        return try await library.getMediaThumbnailAsync(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    func mediaBytesAsync(mediaID: FfiMediaUuid) async throws -> Data {
        try ensureOpen()
        return try await library.getMediaBytesAsync(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    /// Returns a no-copy Foundation view of Rust-owned media bytes. Foundation
    /// retains the UniFFI object through its custom deallocator, so Rust frees
    /// the backing Vec only after UIImage/NSImage has stopped using the data.
    func nativeMediaBytesAsync(mediaID: FfiMediaUuid) async throws -> Data {
        try ensureOpen()
        let nativeBytes = try await library.getMediaBytesNativeAsync(
            mediaId: mediaID,
            appSupportDir: appSupportDirectory
        )
        let length = nativeBytes.len()
        guard length > 0, length <= UInt64(Int.max),
              let pointer = UnsafeMutableRawPointer(bitPattern: UInt(nativeBytes.dataPointer())) else {
            throw LibraryRepositoryError.invalidNativeMediaBuffer
        }
        return Data(bytesNoCopy: pointer, count: Int(length), deallocator: .custom { _, _ in
            // UniFFI releases the Rust Arc in the opaque object's `deinit`.
            // The custom deallocator owns this capture until Foundation is
            // finished with the no-copy buffer.
            withExtendedLifetime(nativeBytes) {}
        })
    }

    /// Materializes plaintext media into the app cache without returning it as
    /// Foundation `Data`. This keeps large videos on the Rust side of the FFI
    /// boundary while AVFoundation and the share sheet consume a file URL.
    func materializedMediaURL(mediaID: FfiMediaUuid, originalFilename: String) async throws -> URL {
        try ensureOpen()
        let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("lasco-media", isDirectory: true)
        let fileExtension = URL(fileURLWithPath: originalFilename).pathExtension
        let filename = fileExtension.isEmpty ? mediaID.value : "\(mediaID.value).\(fileExtension)"
        let destination = directory.appendingPathComponent(filename, isDirectory: false)

        if FileManager.default.fileExists(atPath: destination.path()) {
            return destination
        }

        let key = destination.path()
        if let task = materializingMedia[key] {
            return try await task.value
        }

        let task = Task { [library, appSupportDirectory, destination] in
            let staging = destination.deletingLastPathComponent()
                .appendingPathComponent(".\(UUID().uuidString).part", isDirectory: false)
            defer { try? FileManager.default.removeItem(at: staging) }
            _ = try await library.materializeMediaToPathAsync(
                mediaId: mediaID,
                appSupportDir: appSupportDirectory,
                destinationPath: staging.path
            )
            try FileManager.default.moveItem(at: staging, to: destination)
            return destination
        }
        materializingMedia[key] = task
        do {
            let result = try await task.value
            materializingMedia[key] = nil
            return result
        } catch {
            materializingMedia[key] = nil
            throw error
        }
    }

    func renameMedia(id: FfiMediaUuid, name: String?) async throws {
        try ensureOpen()
        try library.renameMedia(mediaId: id, name: name)
        await notify(.media(id))
        await notify(.mediaList)
        await notify(.albumList)
        await notify(.localMutation)
    }

    func deleteMedia(id: FfiMediaUuid) async throws {
        try ensureOpen()
        try library.deleteMedia(mediaId: id)
        await notify(.media(id))
        await notify(.mediaList)
        await notify(.albumList)
        await notify(.localMutation)
    }

    func addMediaToAlbum(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws {
        try ensureOpen()
        try library.addMediaToAlbum(albumId: albumID, mediaId: mediaID)
        await notifyAlbumMediaChange([albumID])
    }

    func addMediaToAlbumWithoutNotification(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws {
        try ensureOpen()
        try library.addMediaToAlbum(albumId: albumID, mediaId: mediaID)
    }

    func removeMediaFromAlbum(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws {
        try ensureOpen()
        try library.removeMediaFromAlbum(albumId: albumID, mediaId: mediaID)
        await notifyAlbumMediaChange([albumID])
    }

    func moveMedia(id: FfiMediaUuid, from: FfiAlbumUuid, to: FfiAlbumUuid) async throws {
        try ensureOpen()
        try library.moveMediaToAlbum(mediaId: id, fromAlbumId: from, toAlbumId: to)
        await notifyAlbumMediaChange([from, to])
    }

    func moveMediaWithoutNotification(id: FfiMediaUuid, from: FfiAlbumUuid, to: FfiAlbumUuid) async throws {
        try ensureOpen()
        try library.moveMediaToAlbum(mediaId: id, fromAlbumId: from, toAlbumId: to)
    }

    func createAlbum(name: String, parentID: FfiAlbumUuid?) async throws -> FfiAlbumUuid {
        try ensureOpen()
        let id = try library.createAlbum(name: name, parentAlbumId: parentID)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
        return id
    }

    func createAlbumWithoutNotification(name: String, parentID: FfiAlbumUuid?) async throws -> FfiAlbumUuid {
        try ensureOpen()
        return try library.createAlbum(name: name, parentAlbumId: parentID)
    }

    func renameAlbum(id: FfiAlbumUuid, name: String) async throws {
        try ensureOpen()
        try library.renameAlbum(albumId: id, name: name)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
    }

    func reparentAlbum(id: FfiAlbumUuid, parentID: FfiAlbumUuid?) async throws {
        try ensureOpen()
        try library.reparentAlbum(albumId: id, newParentAlbumId: parentID)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
    }

    func deleteAlbum(id: FfiAlbumUuid) async throws {
        try ensureOpen()
        try library.deleteAlbum(albumId: id)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
    }

    func setAlbumThumbnail(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid?) async throws {
        try ensureOpen()
        try library.setAlbumThumbnail(albumId: albumID, mediaId: mediaID)
        await notify(.albumList)
        await notify(.album(albumID))
        await notify(.localMutation)
    }

    func createGroup(albumID: FfiAlbumUuid) async throws -> FfiGroupUuid {
        try ensureOpen()
        let id = try library.createGroup(albumId: albumID)
        await notifyGroupChange(albumID: albumID)
        return id
    }

    func deleteGroup(groupID: FfiGroupUuid) async throws {
        try ensureOpen()
        try library.deleteGroup(groupId: groupID)
        await notify(.all)
    }

    func addMediaToGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) async throws {
        try ensureOpen()
        try library.addMediaToGroup(groupId: groupID, mediaId: mediaID)
        await notify(.all)
    }

    func removeMediaFromGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) async throws {
        try ensureOpen()
        try library.removeMediaFromGroup(groupId: groupID, mediaId: mediaID)
        await notify(.all)
    }

    func createGroupFromSelectedMedia(mediaIDs: [FfiMediaUuid], albumID: FfiAlbumUuid) async throws {
        try ensureOpen()
        let groupID = try library.createGroup(albumId: albumID)
        for mediaID in mediaIDs {
            try library.addMediaToGroup(groupId: groupID, mediaId: mediaID)
            try library.removeMediaFromAlbum(albumId: albumID, mediaId: mediaID)
        }
        await notifyGroupChange(albumID: albumID)
    }

    func importMedia(source: MediaImportSource, albumID: FfiAlbumUuid?) async throws -> FfiMediaUuid {
        try ensureOpen()
        let id = try _importMediaWithoutNotification(source: source, albumID: albumID)
        if let albumID { await notifyImport(albumID: albumID) }
        return id
    }

    func importMediaWithoutNotification(source: MediaImportSource, albumID: FfiAlbumUuid?) async throws -> FfiMediaUuid {
        try ensureOpen()
        return try _importMediaWithoutNotification(source: source, albumID: albumID)
    }

    func importMediaBatch(_ sources: [MediaImportSource], albumID: FfiAlbumUuid?) async throws -> [FfiMediaUuid] {
        try ensureOpen()
        var ids: [FfiMediaUuid] = []
        for source in sources {
            ids.append(try _importMediaWithoutNotification(source: source, albumID: albumID))
        }
        await notifyImport(albumID: albumID)
        return ids
    }

    func setMediaThumbnail(mediaID: FfiMediaUuid, data: Data) async throws {
        try ensureOpen()
        try library.setMediaThumbnail(mediaId: mediaID, data: data)
    }

    func evictLocalData(mediaIDs: [FfiMediaUuid]) async throws {
        try ensureOpen()
        try library.evictLocalData(mediaIds: mediaIDs)
    }

    func evictLocalThumbnails(mediaIDs: [FfiMediaUuid]) async throws {
        try ensureOpen()
        try library.evictLocalThumbnails(mediaIds: mediaIDs)
    }

    func mediaCountLostIfLocalMediaCleared() async throws -> Int {
        try ensureOpen()
        return Int(try library.mediaCountLostIfLocalMediaCleared())
    }

    func mediaCountLostIfRemoteRemoved(remoteID: FfiRemoteUuid) async throws -> Int {
        try ensureOpen()
        return Int(try library.mediaCountLostIfRemoteRemoved(remoteId: remoteID))
    }

    func remoteMediaShortfall(remoteID: FfiRemoteUuid) async throws -> FfiRemoteMediaShortfall {
        try ensureOpen()
        return try library.remoteMediaShortfall(remoteId: remoteID)
    }

    func allMediaIDs() async -> [FfiMediaUuid] {
        guard !closed else { return [] }
        return library.allMediaIds()
    }

    func setDefaultFetchRemote(remoteID: FfiRemoteUuid?) async throws {
        try ensureOpen()
        try library.setDefaultFetchRemote(remoteId: remoteID)
        await notify(.session)
    }

    func setRemoteAutoPush(remoteID: FfiRemoteUuid, enabled: Bool) async throws {
        try ensureOpen()
        try library.setRemoteAutoPush(remoteId: remoteID, enabled: enabled)
        await notify(.session)
    }

    func setMediaSourceOrder(remoteIDs: [FfiRemoteUuid]) async throws {
        try ensureOpen()
        try library.setMediaSourceOrder(remoteIds: remoteIDs)
        await notify(.session)
    }

    func setAutoImportDeviceMedia(enabled: Bool) async throws {
        try ensureOpen()
        try library.setAutoImportDeviceMedia(enabled: enabled)
        await notify(.session)
    }

    func addUser(username: String, password: String) async throws {
        try ensureOpen()
        try library.userAdd(username: username, password: password)
        await notify(.session)
    }

    func addRemoteFixedPath(name: String, path: String) async throws -> FfiRemoteUuid {
        try ensureOpen()
        let id = try library.addRemoteFixedPath(name: name, path: path)
        await notify(.session)
        return id
    }

    func addRemoteDebugLocalApple(name: String) async throws -> FfiRemoteUuid {
        try ensureOpen()
        let id = try library.addRemoteDebugLocalApple(name: name)
        await notify(.session)
        return id
    }

    func addRemoteS3(id: String, endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws -> FfiRemoteUuid {
        try ensureOpen()
        let remoteID = try library.addRemoteS3(name: id, endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey)
        await notify(.session)
        return remoteID
    }

    func removeRemote(id: FfiRemoteUuid) async throws {
        try ensureOpen()
        try library.removeRemote(remoteId: id)
        await notify(.session)
    }

    func hasLascoCloudRemotes() throws -> Bool {
        try ensureOpen()
        return library.listRemotes().contains { $0.kind == "lasco_cloud_s3" }
    }

    func initializeRemote(id: FfiRemoteUuid) async throws {
        try ensureOpen()
        try library.initializeRemote(remoteId: id, appSupportDir: appSupportDirectory)
    }

    func connectRemote(id: FfiRemoteUuid) async throws {
        try ensureOpen()
        try library.connectRemote(remoteId: id, appSupportDir: appSupportDirectory)
    }

    func hasUnpushedChanges(remoteID: FfiRemoteUuid) async -> Bool {
        guard !closed else { return false }
        return library.hasUnpushedChanges(remoteId: remoteID)
    }

    func inspectCompactionLock(remoteID: FfiRemoteUuid) async throws -> FfiCompactionLockInfo? {
        try ensureOpen()
        return try library.inspectCompactionLock(remoteId: remoteID, appSupportDir: appSupportDirectory)
    }

    func removeOwnCompactionLock(remoteID: FfiRemoteUuid) async throws -> Bool {
        try ensureOpen()
        return try library.removeOwnCompactionLock(remoteId: remoteID, appSupportDir: appSupportDirectory)
    }

    func push(remoteID: FfiRemoteUuid, progress: any PushProgressSink) async throws -> UInt64 {
        try ensureOpen()
        let result = try await library.pushRemoteUsingConfiguredMediaSourcesAsync(
            targetRemoteId: remoteID,
            appSupportDir: appSupportDirectory,
            progress: progress
        )
        // A successful push changes the per-remote local/remote state. Publish it
        // so status consumers don't have to be recreated before showing it.
        await notify(.all)
        return result
    }

    func fetch(remoteID: FfiRemoteUuid) async throws -> UInt64 {
        try ensureOpen()
        let result = try await library.fetchRemoteAsync(remoteId: remoteID, appSupportDir: appSupportDirectory)
        await notify(.all)
        return result
    }

    /// Refreshes what this client knows of the media a remote holds, without fetching.
    /// Returns how many blobs it newly confirmed.
    func confirmRemoteMedia(remoteID: FfiRemoteUuid) async throws -> UInt64 {
        try ensureOpen()
        let result = try await library.confirmRemoteMediaAsync(
            remoteId: remoteID,
            appSupportDir: appSupportDirectory
        )
        // The inventory drives push planning and the backup-coverage figures.
        await notify(.all)
        return result
    }

    func close() async {
        guard !closed else { return }
        closed = true
        await changeHub.finish()
    }

    private func ensureOpen() throws {
        if closed { throw LibraryRepositoryError.closed }
    }

    private func _importMediaWithoutNotification(source: MediaImportSource, albumID: FfiAlbumUuid?) throws -> FfiMediaUuid {
        try library.importMedia(
            path: source.path,
            albumId: albumID,
            originalFilename: source.originalFilename,
            appleAaeMediaId: source.appleAaeMediaID,
            appleLivePhotoMediaId: source.appleLivePhotoMediaID
        ).mediaId
    }

    private func notifyImport(albumID: FfiAlbumUuid?) async {
        if let albumID {
            await notify(.album(albumID))
        }
        await notify(.albumList)
        await notify(.mediaList)
        await notify(.localMutation)
    }

    private func notifyAlbumMediaChange(_ albumIDs: [FfiAlbumUuid]) async {
        for albumID in Set(albumIDs) {
            await notify(.album(albumID))
        }
        await notify(.albumList)
        await notify(.mediaList)
        await notify(.localMutation)
    }

    private func notifyGroupChange(albumID: FfiAlbumUuid) async {
        await notify(.album(albumID))
        await notify(.albumList)
        await notify(.localMutation)
    }

    private func notifyGroupChange(albumID: FfiAlbumUuid?) async {
        if let albumID {
            await notifyGroupChange(albumID: albumID)
        } else {
            await notify(.albumList)
            await notify(.localMutation)
        }
    }

    private func notify(_ change: LibraryChange) async {
        await changeHub.notify(change)
    }
}

@MainActor
@Observable
final class LibraryRepository: LibraryRepositoryProtocol {
    @ObservationIgnored private let storage: LibraryRepositoryStorage

    init(library: FfiLibrary, appSupportDirectory: String? = nil) {
        storage = LibraryRepositoryStorage(library: library, appSupportDirectory: appSupportDirectory)
    }

    enum LascoCloudConnectionStep: Sendable {
        case authenticated
        case remotesValidated
        case remotesConfigured
    }

    func authenticateLascoCloud(
        email: String,
        password: String,
        libraryID: FfiLibraryId,
        onProgress: @MainActor @escaping (LascoCloudConnectionStep) -> Void = { _ in }
    ) async throws {
        try await storage.configureLascoCloudAuth()
        if await storage.isLascoCloudAuthenticated() {
            throw LibraryRepositoryError.cloudAlreadyConnected
        }
        try await storage.lascoCloudLogin(email: email, password: password)
        do {
            onProgress(.authenticated)
            let remotes = try await storage.lascoCloudRemotes()
            _ = try await storage.validateLascoCloud(remotes)
            onProgress(.remotesValidated)
            try await storage.configureLascoCloud(remotes)
            try await storage.assignLascoCloudRemotesToThisLibrary(remoteIDs: remotes.map(\.id))
            onProgress(.remotesConfigured)
        } catch {
            try? await storage.revokeLascoCloudSession()
            try? await storage.clearLascoCloudAuthAndCredentials()
            throw error
        }
    }

    func isLascoCloudConnected(libraryID: FfiLibraryId) async -> Bool {
        await storage.isLascoCloudAuthenticated()
    }

    /// Configures the Rust-owned Lasco Cloud auth manager after opening a library.
    /// It loads the session from secure storage; storage credentials stay lazy.
    func restoreLascoCloudSession(libraryID: FfiLibraryId) async {
        try? await storage.configureLascoCloudAuth()
    }

    func signOutLascoCloud(libraryID: FfiLibraryId) async throws {
        if try await storage.hasLascoCloudRemotes() {
            throw LibraryRepositoryError.cloudSignOutRequiresRemoteRemoval
        }
        try await storage.revokeLascoCloudSession()
    }

    func lascoCloudSubscription(libraryID: FfiLibraryId) async throws -> LascoCloudAccount {
        try await storage.lascoCloudAccount()
    }

    func changes() async -> AsyncStream<LibraryChange> { await storage.changes() }
    func notifyChanged(_ change: LibraryChange) async { await storage.notifyChanged(change) }
    func notifyPhotoImportChanged(initialImport: Bool) async { await storage.notifyPhotoImportChanged(initialImport: initialImport) }
    func mediaByDateCount() async throws -> Int { try await storage.mediaByDateCount() }
    func mediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem] { try await storage.mediaByDate(offset: offset, limit: limit) }
    func mediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors { try await storage.mediaByDateNeighbors(position: position) }
    func orphanMediaByDateCount() async throws -> Int { try await storage.orphanMediaByDateCount() }
    func orphanMediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem] { try await storage.orphanMediaByDate(offset: offset, limit: limit) }
    func orphanMediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors { try await storage.orphanMediaByDateNeighbors(position: position) }
    func albumsCount(parentID: FfiAlbumUuid?) async throws -> Int { try await storage.albumsCount(parentID: parentID) }
    func albums(parentID: FfiAlbumUuid?, offset: Int, limit: Int) async throws -> [FfiAlbum] { try await storage.albums(parentID: parentID, offset: offset, limit: limit) }
    func albums(withIDs ids: Set<FfiAlbumUuid>) async throws -> [FfiAlbum] { try await storage.albums(withIDs: ids) }
    func albumItemsCount(albumID: FfiAlbumUuid) async throws -> Int { try await storage.albumItemsCount(albumID: albumID) }
    func albumItems(albumID: FfiAlbumUuid, ascending: Bool, offset: Int, limit: Int) async throws -> [FfiAlbumItem] { try await storage.albumItems(albumID: albumID, ascending: ascending, offset: offset, limit: limit) }
    func albumItemsByDateNeighbors(albumID: FfiAlbumUuid, ascending: Bool, position: Int) async throws -> FfiMediaOrGroupNeighbors { try await storage.albumItemsByDateNeighbors(albumID: albumID, ascending: ascending, position: position) }
    func showMedia(id: FfiMediaUuid) async throws -> FfiMediaItem { try await storage.showMedia(id: id) }
    func localStateStats() async throws -> FfiLocalStateStats { try await storage.localStateStats() }
    func sessionSnapshot() async throws -> LibrarySessionSnapshot { try await storage.sessionSnapshot() }
    func mediaInAlbum(albumID: FfiAlbumUuid) async throws -> [FfiMediaItem] { try await storage.mediaInAlbum(albumID: albumID) }
    func mediaContainingAlbumIDs(mediaID: FfiMediaUuid, includeViaGroups: Bool) async throws -> [FfiAlbumUuid] { try await storage.mediaContainingAlbumIDs(mediaID: mediaID, includeViaGroups: includeViaGroups) }
    func mediaAlbumIDs(mediaID: FfiMediaUuid) async throws -> [FfiAlbumUuid] { try await storage.mediaAlbumIDs(mediaID: mediaID) }
    func albumGroups(albumID: FfiAlbumUuid) async throws -> [FfiGroup] { try await storage.albumGroups(albumID: albumID) }
    func groupMedia(groupID: FfiGroupUuid) async throws -> [FfiMediaItem] { try await storage.groupMedia(groupID: groupID) }
    func listOperations(startPos: UInt64, endPosExclusive: UInt64) async throws -> [FfiCrdtOperation] {
        try await storage.listOperations(startPos: startPos, endPosExclusive: endPosExclusive)
    }
    func thumbnail(mediaID: FfiMediaUuid) async throws -> Data { try await storage.thumbnail(mediaID: mediaID) }
    func mediaBytes(mediaID: FfiMediaUuid) async throws -> Data { try await storage.mediaBytes(mediaID: mediaID) }
    func thumbnailAsync(mediaID: FfiMediaUuid) async throws -> Data { try await storage.thumbnailAsync(mediaID: mediaID) }
    func mediaBytesAsync(mediaID: FfiMediaUuid) async throws -> Data { try await storage.mediaBytesAsync(mediaID: mediaID) }
    func nativeMediaBytesAsync(mediaID: FfiMediaUuid) async throws -> Data { try await storage.nativeMediaBytesAsync(mediaID: mediaID) }
    func materializedMediaURL(mediaID: FfiMediaUuid, originalFilename: String) async throws -> URL {
        try await storage.materializedMediaURL(mediaID: mediaID, originalFilename: originalFilename)
    }
    func renameMedia(id: FfiMediaUuid, name: String?) async throws { try await storage.renameMedia(id: id, name: name) }
    func deleteMedia(id: FfiMediaUuid) async throws { try await storage.deleteMedia(id: id) }
    func addMediaToAlbum(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws { try await storage.addMediaToAlbum(albumID: albumID, mediaID: mediaID) }
    func addMediaToAlbumWithoutNotification(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws { try await storage.addMediaToAlbumWithoutNotification(albumID: albumID, mediaID: mediaID) }
    func removeMediaFromAlbum(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) async throws { try await storage.removeMediaFromAlbum(albumID: albumID, mediaID: mediaID) }
    func moveMedia(id: FfiMediaUuid, from: FfiAlbumUuid, to: FfiAlbumUuid) async throws { try await storage.moveMedia(id: id, from: from, to: to) }
    func moveMediaWithoutNotification(id: FfiMediaUuid, from: FfiAlbumUuid, to: FfiAlbumUuid) async throws { try await storage.moveMediaWithoutNotification(id: id, from: from, to: to) }
    func createAlbum(name: String, parentID: FfiAlbumUuid?) async throws -> FfiAlbumUuid { try await storage.createAlbum(name: name, parentID: parentID) }
    func createAlbumWithoutNotification(name: String, parentID: FfiAlbumUuid?) async throws -> FfiAlbumUuid { try await storage.createAlbumWithoutNotification(name: name, parentID: parentID) }
    func renameAlbum(id: FfiAlbumUuid, name: String) async throws { try await storage.renameAlbum(id: id, name: name) }
    func reparentAlbum(id: FfiAlbumUuid, parentID: FfiAlbumUuid?) async throws { try await storage.reparentAlbum(id: id, parentID: parentID) }
    func deleteAlbum(id: FfiAlbumUuid) async throws { try await storage.deleteAlbum(id: id) }
    func setAlbumThumbnail(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid?) async throws { try await storage.setAlbumThumbnail(albumID: albumID, mediaID: mediaID) }
    func createGroup(albumID: FfiAlbumUuid) async throws -> FfiGroupUuid { try await storage.createGroup(albumID: albumID) }
    func deleteGroup(groupID: FfiGroupUuid) async throws { try await storage.deleteGroup(groupID: groupID) }
    func addMediaToGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) async throws { try await storage.addMediaToGroup(groupID: groupID, mediaID: mediaID) }
    func removeMediaFromGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) async throws { try await storage.removeMediaFromGroup(groupID: groupID, mediaID: mediaID) }
    func createGroupFromSelectedMedia(mediaIDs: [FfiMediaUuid], albumID: FfiAlbumUuid) async throws { try await storage.createGroupFromSelectedMedia(mediaIDs: mediaIDs, albumID: albumID) }
    func importMedia(source: MediaImportSource, albumID: FfiAlbumUuid?) async throws -> FfiMediaUuid { try await storage.importMedia(source: source, albumID: albumID) }
    func importMediaWithoutNotification(source: MediaImportSource, albumID: FfiAlbumUuid?) async throws -> FfiMediaUuid { try await storage.importMediaWithoutNotification(source: source, albumID: albumID) }
    func importMediaBatch(_ sources: [MediaImportSource], albumID: FfiAlbumUuid?) async throws -> [FfiMediaUuid] { try await storage.importMediaBatch(sources, albumID: albumID) }
    func setMediaThumbnail(mediaID: FfiMediaUuid, data: Data) async throws { try await storage.setMediaThumbnail(mediaID: mediaID, data: data) }
    func evictLocalData(mediaIDs: [FfiMediaUuid]) async throws { try await storage.evictLocalData(mediaIDs: mediaIDs) }
    func evictLocalThumbnails(mediaIDs: [FfiMediaUuid]) async throws { try await storage.evictLocalThumbnails(mediaIDs: mediaIDs) }
    func mediaCountLostIfLocalMediaCleared() async throws -> Int { try await storage.mediaCountLostIfLocalMediaCleared() }
    func mediaCountLostIfRemoteRemoved(remoteID: FfiRemoteUuid) async throws -> Int { try await storage.mediaCountLostIfRemoteRemoved(remoteID: remoteID) }
    func remoteMediaShortfall(remoteID: FfiRemoteUuid) async throws -> FfiRemoteMediaShortfall { try await storage.remoteMediaShortfall(remoteID: remoteID) }
    func allMediaIDs() async -> [FfiMediaUuid] { await storage.allMediaIDs() }
    func setDefaultFetchRemote(remoteID: FfiRemoteUuid?) async throws { try await storage.setDefaultFetchRemote(remoteID: remoteID) }
    func setRemoteAutoPush(remoteID: FfiRemoteUuid, enabled: Bool) async throws { try await storage.setRemoteAutoPush(remoteID: remoteID, enabled: enabled) }
    func setMediaSourceOrder(remoteIDs: [FfiRemoteUuid]) async throws { try await storage.setMediaSourceOrder(remoteIDs: remoteIDs) }
    func setAutoImportDeviceMedia(enabled: Bool) async throws { try await storage.setAutoImportDeviceMedia(enabled: enabled) }
    func addUser(username: String, password: String) async throws { try await storage.addUser(username: username, password: password) }
    func addRemoteFixedPath(name: String, path: String) async throws -> FfiRemoteUuid { try await storage.addRemoteFixedPath(name: name, path: path) }
    func addRemoteDebugLocalApple(name: String) async throws -> FfiRemoteUuid { try await storage.addRemoteDebugLocalApple(name: name) }
    func addRemoteS3(id: String, endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws -> FfiRemoteUuid { try await storage.addRemoteS3(id: id, endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey) }
    func removeRemote(id: FfiRemoteUuid) async throws { try await storage.removeRemote(id: id) }
    func initializeRemote(id: FfiRemoteUuid) async throws { try await storage.initializeRemote(id: id) }
    func connectRemote(id: FfiRemoteUuid) async throws { try await storage.connectRemote(id: id) }
    func hasUnpushedChanges(remoteID: FfiRemoteUuid) async -> Bool { await storage.hasUnpushedChanges(remoteID: remoteID) }
    func inspectCompactionLock(remoteID: FfiRemoteUuid) async throws -> FfiCompactionLockInfo? { try await storage.inspectCompactionLock(remoteID: remoteID) }
    func removeOwnCompactionLock(remoteID: FfiRemoteUuid) async throws -> Bool { try await storage.removeOwnCompactionLock(remoteID: remoteID) }
    func push(remoteID: FfiRemoteUuid, progress: any PushProgressSink) async throws -> UInt64 {
        try await storage.push(remoteID: remoteID, progress: progress)
    }
    func fetch(remoteID: FfiRemoteUuid) async throws -> UInt64 { try await storage.fetch(remoteID: remoteID) }
    func confirmRemoteMedia(remoteID: FfiRemoteUuid) async throws -> UInt64 { try await storage.confirmRemoteMedia(remoteID: remoteID) }
    func close() async { await storage.close() }
}
