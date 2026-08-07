import Foundation
import Observation

struct LibrarySessionSnapshot: Sendable, Equatable {
    let users: [String]
    let remotes: [FfiRemote]
    let defaultFetchRemoteID: String?
    let autoImportDeviceMedia: Bool
}

struct MediaImportSource: Sendable {
    let path: String
    let originalFilename: String?
    let appleAaeMediaID: String?
    let appleLivePhotoMediaID: String?

    init(
        path: String,
        originalFilename: String? = nil,
        appleAaeMediaID: String? = nil,
        appleLivePhotoMediaID: String? = nil
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
    func albumsCount(parentID: String?) async throws -> Int
    func albums(parentID: String?, offset: Int, limit: Int) async throws -> [FfiAlbum]
    func albums(withIDs ids: Set<String>) async throws -> [FfiAlbum]
    func albumItemsCount(albumID: String) async throws -> Int
    func albumItems(albumID: String, ascending: Bool, offset: Int, limit: Int) async throws -> [FfiAlbumItem]
    func albumItemsByDateNeighbors(albumID: String, ascending: Bool, position: Int) async throws -> FfiMediaOrGroupNeighbors
    func showMedia(id: String) async throws -> FfiMediaItem
    func localStateStats() async throws -> FfiLocalStateStats
    func sessionSnapshot() async throws -> LibrarySessionSnapshot
    func mediaInAlbum(albumID: String) async throws -> [FfiMediaItem]
    func mediaContainingAlbumIDs(mediaID: String, includeViaGroups: Bool) async throws -> [String]
    func mediaAlbumIDs(mediaID: String) async throws -> [String]
    func albumGroups(albumID: String) async throws -> [FfiGroup]
    func groupMedia(groupID: String) async throws -> [FfiMediaItem]
    func listOperationGroups() async throws -> [FfiOperationGroup]

    func thumbnail(mediaID: String) async throws -> Data
    func mediaBytes(mediaID: String) async throws -> Data
    func thumbnailAsync(mediaID: String) async throws -> Data
    func mediaBytesAsync(mediaID: String) async throws -> Data

    func renameMedia(id: String, name: String?) async throws
    func deleteMedia(id: String) async throws
    func addMediaToAlbum(albumID: String, mediaID: String) async throws
    func addMediaToAlbumWithoutNotification(albumID: String, mediaID: String) async throws
    func removeMediaFromAlbum(albumID: String, mediaID: String) async throws
    func moveMedia(id: String, from: String, to: String) async throws
    func moveMediaWithoutNotification(id: String, from: String, to: String) async throws
    func createAlbum(name: String, parentID: String?) async throws -> String
    func createAlbumWithoutNotification(name: String, parentID: String?) async throws -> String
    func renameAlbum(id: String, name: String) async throws
    func reparentAlbum(id: String, parentID: String?) async throws
    func deleteAlbum(id: String) async throws
    func setAlbumThumbnail(albumID: String, mediaID: String?) async throws
    func createGroup(albumID: String) async throws -> String
    func deleteGroup(groupID: String) async throws
    func addMediaToGroup(groupID: String, mediaID: String) async throws
    func removeMediaFromGroup(groupID: String, mediaID: String) async throws
    func createGroupFromSelectedMedia(mediaIDs: [String], albumID: String) async throws

    func importMedia(source: MediaImportSource, albumID: String?) async throws -> String
    func importMediaWithoutNotification(source: MediaImportSource, albumID: String?) async throws -> String
    func importMediaBatch(_ sources: [MediaImportSource], albumID: String?) async throws -> [String]
    func setMediaThumbnail(mediaID: String, data: Data) async throws
    func evictLocalData(mediaIDs: [String]) async throws
    func evictLocalThumbnails(mediaIDs: [String]) async throws
    func mediaIDsWithoutRemoteBackup() async throws -> [String]
    func allMediaIDs() async -> [String]

    func setDefaultFetchRemote(remoteID: String?) async throws
    func setRemoteAutoPush(remoteID: String, enabled: Bool) async throws
    func setAutoImportDeviceMedia(enabled: Bool) async throws
    func addUser(username: String, password: String) async throws
    func addRemoteFixedPath(name: String, path: String) async throws -> String
    func addRemoteDebugLocalApple(name: String) async throws -> String
    func addRemoteS3(id: String, endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws -> String
    func removeRemote(id: String) async throws
    func initializeRemote(id: String) async throws
    func connectRemote(id: String) async throws
    func hasUnpushedChanges(remoteID: String) async -> Bool

    func push(remoteID: String) async throws -> UInt32
    func fetch(remoteID: String) async throws -> UInt32
    func sync() async throws -> FfiSyncResult
    func close() async
}

enum LibraryRepositoryError: LocalizedError {
    case closed

    var errorDescription: String? {
        "The library session is closed."
    }
}

private actor LibraryRepositoryStorage: LibraryRepositoryProtocol {
    private let library: FfiLibrary
    private let changeHub = LibraryChangeHub()
    private var closed = false

    let libraryID: String
    let appSupportDirectory: String?

    init(library: FfiLibrary, appSupportDirectory: String? = nil) {
        self.library = library
        self.libraryID = library.libraryId().value
        self.appSupportDirectory = appSupportDirectory ?? Self.defaultAppSupportDirectory
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

    func albumsCount(parentID: String?) async throws -> Int {
        try ensureOpen()
        return Int(try library.albumAlbumsCount(parentAlbumId: parentID))
    }

    func albums(parentID: String?, offset: Int, limit: Int) async throws -> [FfiAlbum] {
        try ensureOpen()
        return try page(offset: offset, limit: limit) { start, end in
            try library.albumAlbumsRange(
                parentAlbumId: parentID,
                posStartInclusive: start,
                posEndInclusive: end
            )
        }
    }

    func albums(withIDs ids: Set<String>) async throws -> [FfiAlbum] {
        try ensureOpen()
        guard !ids.isEmpty else { return [] }

        var results: [FfiAlbum] = []
        var parents: [String?] = [nil]
        var visitedParents = Set<String>()
        while !parents.isEmpty {
            try Task.checkCancellation()
            let parentID = parents.removeLast()
            let key = parentID ?? "<root>"
            guard visitedParents.insert(key).inserted else { continue }
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

    func albumItemsCount(albumID: String) async throws -> Int {
        try ensureOpen()
        return Int(try library.albumItemsCount(albumId: albumID))
    }

    func albumItems(albumID: String, ascending: Bool, offset: Int, limit: Int) async throws -> [FfiAlbumItem] {
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
        albumID: String,
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

    func showMedia(id: String) async throws -> FfiMediaItem {
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
            defaultFetchRemoteID: library.getDefaultFetchRemote(),
            autoImportDeviceMedia: library.getAutoImportDeviceMedia()
        )
    }

    func mediaInAlbum(albumID: String) async throws -> [FfiMediaItem] {
        try ensureOpen()
        return try library.mediaInAlbum(albumId: albumID)
    }

    func mediaContainingAlbumIDs(mediaID: String, includeViaGroups: Bool) async throws -> [String] {
        try ensureOpen()
        return try library.mediaContainingAlbumIds(mediaId: mediaID, includeViaGroups: includeViaGroups)
    }

    func mediaAlbumIDs(mediaID: String) async throws -> [String] {
        try ensureOpen()
        return try library.mediaAlbumIds(mediaId: mediaID)
    }

    func albumGroups(albumID: String) async throws -> [FfiGroup] {
        try ensureOpen()
        return try library.albumListGroups(albumId: albumID)
    }

    func groupMedia(groupID: String) async throws -> [FfiMediaItem] {
        try ensureOpen()
        return try library.groupListMedia(groupId: groupID)
    }

    func listOperationGroups() async throws -> [FfiOperationGroup] {
        try ensureOpen()
        return try library.listOperationGroups()
    }

    func thumbnail(mediaID: String) async throws -> Data {
        try ensureOpen()
        return try library.getMediaThumbnail(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    func mediaBytes(mediaID: String) async throws -> Data {
        try ensureOpen()
        return try library.getMediaBytes(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    func thumbnailAsync(mediaID: String) async throws -> Data {
        try ensureOpen()
        return try await library.getMediaThumbnailAsync(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    func mediaBytesAsync(mediaID: String) async throws -> Data {
        try ensureOpen()
        return try await library.getMediaBytesAsync(mediaId: mediaID, appSupportDir: appSupportDirectory)
    }

    func renameMedia(id: String, name: String?) async throws {
        try ensureOpen()
        try library.renameMedia(mediaId: id, name: name)
        await notify(.media(id))
        await notify(.mediaList)
        await notify(.albumList)
        await notify(.localMutation)
    }

    func deleteMedia(id: String) async throws {
        try ensureOpen()
        try library.deleteMedia(mediaId: id)
        await notify(.media(id))
        await notify(.mediaList)
        await notify(.albumList)
        await notify(.localMutation)
    }

    func addMediaToAlbum(albumID: String, mediaID: String) async throws {
        try ensureOpen()
        try library.addMediaToAlbum(albumId: albumID, mediaId: mediaID)
        await notifyAlbumMediaChange([albumID])
    }

    func addMediaToAlbumWithoutNotification(albumID: String, mediaID: String) async throws {
        try ensureOpen()
        try library.addMediaToAlbum(albumId: albumID, mediaId: mediaID)
    }

    func removeMediaFromAlbum(albumID: String, mediaID: String) async throws {
        try ensureOpen()
        try library.removeMediaFromAlbum(albumId: albumID, mediaId: mediaID)
        await notifyAlbumMediaChange([albumID])
    }

    func moveMedia(id: String, from: String, to: String) async throws {
        try ensureOpen()
        try library.moveMediaToAlbum(mediaId: id, fromAlbumId: from, toAlbumId: to)
        await notifyAlbumMediaChange([from, to])
    }

    func moveMediaWithoutNotification(id: String, from: String, to: String) async throws {
        try ensureOpen()
        try library.moveMediaToAlbum(mediaId: id, fromAlbumId: from, toAlbumId: to)
    }

    func createAlbum(name: String, parentID: String?) async throws -> String {
        try ensureOpen()
        let id = try library.createAlbum(name: name, parentAlbumId: parentID)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
        return id
    }

    func createAlbumWithoutNotification(name: String, parentID: String?) async throws -> String {
        try ensureOpen()
        return try library.createAlbum(name: name, parentAlbumId: parentID)
    }

    func renameAlbum(id: String, name: String) async throws {
        try ensureOpen()
        try library.renameAlbum(albumId: id, name: name)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
    }

    func reparentAlbum(id: String, parentID: String?) async throws {
        try ensureOpen()
        try library.reparentAlbum(albumId: id, newParentAlbumId: parentID)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
    }

    func deleteAlbum(id: String) async throws {
        try ensureOpen()
        try library.deleteAlbum(albumId: id)
        await notify(.albumList)
        await notify(.album(id))
        await notify(.localMutation)
    }

    func setAlbumThumbnail(albumID: String, mediaID: String?) async throws {
        try ensureOpen()
        try library.setAlbumThumbnail(albumId: albumID, mediaId: mediaID)
        await notify(.albumList)
        await notify(.album(albumID))
        await notify(.localMutation)
    }

    func createGroup(albumID: String) async throws -> String {
        try ensureOpen()
        let id = try library.createGroup(albumId: albumID)
        await notifyGroupChange(albumID: albumID)
        return id
    }

    func deleteGroup(groupID: String) async throws {
        try ensureOpen()
        try library.deleteGroup(groupId: groupID)
        await notify(.all)
    }

    func addMediaToGroup(groupID: String, mediaID: String) async throws {
        try ensureOpen()
        try library.addMediaToGroup(groupId: groupID, mediaId: mediaID)
        await notify(.all)
    }

    func removeMediaFromGroup(groupID: String, mediaID: String) async throws {
        try ensureOpen()
        try library.removeMediaFromGroup(groupId: groupID, mediaId: mediaID)
        await notify(.all)
    }

    func createGroupFromSelectedMedia(mediaIDs: [String], albumID: String) async throws {
        try ensureOpen()
        let groupID = try library.createGroup(albumId: albumID)
        for mediaID in mediaIDs {
            try library.addMediaToGroup(groupId: groupID, mediaId: mediaID)
            try library.removeMediaFromAlbum(albumId: albumID, mediaId: mediaID)
        }
        await notifyGroupChange(albumID: albumID)
    }

    func importMedia(source: MediaImportSource, albumID: String?) async throws -> String {
        try ensureOpen()
        let id = try _importMediaWithoutNotification(source: source, albumID: albumID)
        if let albumID { await notifyImport(albumID: albumID) }
        return id
    }

    func importMediaWithoutNotification(source: MediaImportSource, albumID: String?) async throws -> String {
        try ensureOpen()
        return try _importMediaWithoutNotification(source: source, albumID: albumID)
    }

    func importMediaBatch(_ sources: [MediaImportSource], albumID: String?) async throws -> [String] {
        try ensureOpen()
        var ids: [String] = []
        for source in sources {
            ids.append(try _importMediaWithoutNotification(source: source, albumID: albumID))
        }
        await notifyImport(albumID: albumID)
        return ids
    }

    func setMediaThumbnail(mediaID: String, data: Data) async throws {
        try ensureOpen()
        try library.setMediaThumbnail(mediaId: mediaID, data: data)
    }

    func evictLocalData(mediaIDs: [String]) async throws {
        try ensureOpen()
        try library.evictLocalData(mediaIds: mediaIDs)
    }

    func evictLocalThumbnails(mediaIDs: [String]) async throws {
        try ensureOpen()
        try library.evictLocalThumbnails(mediaIds: mediaIDs)
    }

    func mediaIDsWithoutRemoteBackup() async throws -> [String] {
        try ensureOpen()
        return try library.mediaIdsWithoutRemoteBackup()
    }

    func allMediaIDs() async -> [String] {
        guard !closed else { return [] }
        return library.allMediaIds()
    }

    func setDefaultFetchRemote(remoteID: String?) async throws {
        try ensureOpen()
        try library.setDefaultFetchRemote(remoteId: remoteID)
        await notify(.session)
    }

    func setRemoteAutoPush(remoteID: String, enabled: Bool) async throws {
        try ensureOpen()
        try library.setRemoteAutoPush(remoteId: remoteID, enabled: enabled)
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

    func addRemoteFixedPath(name: String, path: String) async throws -> String {
        try ensureOpen()
        let id = try library.addRemoteFixedPath(name: name, path: path)
        await notify(.session)
        return id
    }

    func addRemoteDebugLocalApple(name: String) async throws -> String {
        try ensureOpen()
        let id = try library.addRemoteDebugLocalApple(name: name)
        await notify(.session)
        return id
    }

    func addRemoteS3(id: String, endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws -> String {
        try ensureOpen()
        let remoteID = try library.addRemoteS3(name: id, endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey)
        await notify(.session)
        return remoteID
    }

    func removeRemote(id: String) async throws {
        try ensureOpen()
        try library.removeRemote(remoteId: id)
        await notify(.session)
    }

    func initializeRemote(id: String) async throws {
        try ensureOpen()
        try library.initializeRemote(remoteId: id, appSupportDir: appSupportDirectory)
    }

    func connectRemote(id: String) async throws {
        try ensureOpen()
        try library.connectRemote(remoteId: id, appSupportDir: appSupportDirectory)
    }

    func hasUnpushedChanges(remoteID: String) async -> Bool {
        guard !closed else { return false }
        return library.hasUnpushedChanges(remoteId: remoteID)
    }

    func push(remoteID: String) async throws -> UInt32 {
        try ensureOpen()
        let result = try await library.pushRemoteAsync(remoteId: remoteID, appSupportDir: appSupportDirectory)
        // A successful push changes the per-remote local/remote state. Publish it
        // so status consumers don't have to be recreated before showing it.
        await notify(.all)
        return result
    }

    func fetch(remoteID: String) async throws -> UInt32 {
        try ensureOpen()
        let result = try await library.fetchRemoteAsync(remoteId: remoteID, appSupportDir: appSupportDirectory)
        await notify(.all)
        return result
    }

    func sync() async throws -> FfiSyncResult {
        try ensureOpen()
        let result = try await library.syncAsync(appSupportDir: appSupportDirectory)
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

    private func _importMediaWithoutNotification(source: MediaImportSource, albumID: String?) throws -> String {
        try library.importMedia(
            path: source.path,
            albumId: albumID,
            originalFilename: source.originalFilename,
            appleAaeMediaId: source.appleAaeMediaID,
            appleLivePhotoMediaId: source.appleLivePhotoMediaID
        ).mediaId
    }

    private func notifyImport(albumID: String?) async {
        if let albumID {
            await notify(.album(albumID))
        }
        await notify(.albumList)
        await notify(.mediaList)
        await notify(.localMutation)
    }

    private func notifyAlbumMediaChange(_ albumIDs: [String]) async {
        for albumID in Set(albumIDs) {
            await notify(.album(albumID))
        }
        await notify(.albumList)
        await notify(.mediaList)
        await notify(.localMutation)
    }

    private func notifyGroupChange(albumID: String) async {
        await notify(.album(albumID))
        await notify(.albumList)
        await notify(.localMutation)
    }

    private func notifyGroupChange(albumID: String?) async {
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

    func changes() async -> AsyncStream<LibraryChange> { await storage.changes() }
    func notifyChanged(_ change: LibraryChange) async { await storage.notifyChanged(change) }
    func notifyPhotoImportChanged(initialImport: Bool) async { await storage.notifyPhotoImportChanged(initialImport: initialImport) }
    func mediaByDateCount() async throws -> Int { try await storage.mediaByDateCount() }
    func mediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem] { try await storage.mediaByDate(offset: offset, limit: limit) }
    func mediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors { try await storage.mediaByDateNeighbors(position: position) }
    func orphanMediaByDateCount() async throws -> Int { try await storage.orphanMediaByDateCount() }
    func orphanMediaByDate(offset: Int, limit: Int) async throws -> [FfiMediaItem] { try await storage.orphanMediaByDate(offset: offset, limit: limit) }
    func orphanMediaByDateNeighbors(position: Int) async throws -> FfiMediaNeighbors { try await storage.orphanMediaByDateNeighbors(position: position) }
    func albumsCount(parentID: String?) async throws -> Int { try await storage.albumsCount(parentID: parentID) }
    func albums(parentID: String?, offset: Int, limit: Int) async throws -> [FfiAlbum] { try await storage.albums(parentID: parentID, offset: offset, limit: limit) }
    func albums(withIDs ids: Set<String>) async throws -> [FfiAlbum] { try await storage.albums(withIDs: ids) }
    func albumItemsCount(albumID: String) async throws -> Int { try await storage.albumItemsCount(albumID: albumID) }
    func albumItems(albumID: String, ascending: Bool, offset: Int, limit: Int) async throws -> [FfiAlbumItem] { try await storage.albumItems(albumID: albumID, ascending: ascending, offset: offset, limit: limit) }
    func albumItemsByDateNeighbors(albumID: String, ascending: Bool, position: Int) async throws -> FfiMediaOrGroupNeighbors { try await storage.albumItemsByDateNeighbors(albumID: albumID, ascending: ascending, position: position) }
    func showMedia(id: String) async throws -> FfiMediaItem { try await storage.showMedia(id: id) }
    func localStateStats() async throws -> FfiLocalStateStats { try await storage.localStateStats() }
    func sessionSnapshot() async throws -> LibrarySessionSnapshot { try await storage.sessionSnapshot() }
    func mediaInAlbum(albumID: String) async throws -> [FfiMediaItem] { try await storage.mediaInAlbum(albumID: albumID) }
    func mediaContainingAlbumIDs(mediaID: String, includeViaGroups: Bool) async throws -> [String] { try await storage.mediaContainingAlbumIDs(mediaID: mediaID, includeViaGroups: includeViaGroups) }
    func mediaAlbumIDs(mediaID: String) async throws -> [String] { try await storage.mediaAlbumIDs(mediaID: mediaID) }
    func albumGroups(albumID: String) async throws -> [FfiGroup] { try await storage.albumGroups(albumID: albumID) }
    func groupMedia(groupID: String) async throws -> [FfiMediaItem] { try await storage.groupMedia(groupID: groupID) }
    func listOperationGroups() async throws -> [FfiOperationGroup] { try await storage.listOperationGroups() }
    func thumbnail(mediaID: String) async throws -> Data { try await storage.thumbnail(mediaID: mediaID) }
    func mediaBytes(mediaID: String) async throws -> Data { try await storage.mediaBytes(mediaID: mediaID) }
    func thumbnailAsync(mediaID: String) async throws -> Data { try await storage.thumbnailAsync(mediaID: mediaID) }
    func mediaBytesAsync(mediaID: String) async throws -> Data { try await storage.mediaBytesAsync(mediaID: mediaID) }
    func renameMedia(id: String, name: String?) async throws { try await storage.renameMedia(id: id, name: name) }
    func deleteMedia(id: String) async throws { try await storage.deleteMedia(id: id) }
    func addMediaToAlbum(albumID: String, mediaID: String) async throws { try await storage.addMediaToAlbum(albumID: albumID, mediaID: mediaID) }
    func addMediaToAlbumWithoutNotification(albumID: String, mediaID: String) async throws { try await storage.addMediaToAlbumWithoutNotification(albumID: albumID, mediaID: mediaID) }
    func removeMediaFromAlbum(albumID: String, mediaID: String) async throws { try await storage.removeMediaFromAlbum(albumID: albumID, mediaID: mediaID) }
    func moveMedia(id: String, from: String, to: String) async throws { try await storage.moveMedia(id: id, from: from, to: to) }
    func moveMediaWithoutNotification(id: String, from: String, to: String) async throws { try await storage.moveMediaWithoutNotification(id: id, from: from, to: to) }
    func createAlbum(name: String, parentID: String?) async throws -> String { try await storage.createAlbum(name: name, parentID: parentID) }
    func createAlbumWithoutNotification(name: String, parentID: String?) async throws -> String { try await storage.createAlbumWithoutNotification(name: name, parentID: parentID) }
    func renameAlbum(id: String, name: String) async throws { try await storage.renameAlbum(id: id, name: name) }
    func reparentAlbum(id: String, parentID: String?) async throws { try await storage.reparentAlbum(id: id, parentID: parentID) }
    func deleteAlbum(id: String) async throws { try await storage.deleteAlbum(id: id) }
    func setAlbumThumbnail(albumID: String, mediaID: String?) async throws { try await storage.setAlbumThumbnail(albumID: albumID, mediaID: mediaID) }
    func createGroup(albumID: String) async throws -> String { try await storage.createGroup(albumID: albumID) }
    func deleteGroup(groupID: String) async throws { try await storage.deleteGroup(groupID: groupID) }
    func addMediaToGroup(groupID: String, mediaID: String) async throws { try await storage.addMediaToGroup(groupID: groupID, mediaID: mediaID) }
    func removeMediaFromGroup(groupID: String, mediaID: String) async throws { try await storage.removeMediaFromGroup(groupID: groupID, mediaID: mediaID) }
    func createGroupFromSelectedMedia(mediaIDs: [String], albumID: String) async throws { try await storage.createGroupFromSelectedMedia(mediaIDs: mediaIDs, albumID: albumID) }
    func importMedia(source: MediaImportSource, albumID: String?) async throws -> String { try await storage.importMedia(source: source, albumID: albumID) }
    func importMediaWithoutNotification(source: MediaImportSource, albumID: String?) async throws -> String { try await storage.importMediaWithoutNotification(source: source, albumID: albumID) }
    func importMediaBatch(_ sources: [MediaImportSource], albumID: String?) async throws -> [String] { try await storage.importMediaBatch(sources, albumID: albumID) }
    func setMediaThumbnail(mediaID: String, data: Data) async throws { try await storage.setMediaThumbnail(mediaID: mediaID, data: data) }
    func evictLocalData(mediaIDs: [String]) async throws { try await storage.evictLocalData(mediaIDs: mediaIDs) }
    func evictLocalThumbnails(mediaIDs: [String]) async throws { try await storage.evictLocalThumbnails(mediaIDs: mediaIDs) }
    func mediaIDsWithoutRemoteBackup() async throws -> [String] { try await storage.mediaIDsWithoutRemoteBackup() }
    func allMediaIDs() async -> [String] { await storage.allMediaIDs() }
    func setDefaultFetchRemote(remoteID: String?) async throws { try await storage.setDefaultFetchRemote(remoteID: remoteID) }
    func setRemoteAutoPush(remoteID: String, enabled: Bool) async throws { try await storage.setRemoteAutoPush(remoteID: remoteID, enabled: enabled) }
    func setAutoImportDeviceMedia(enabled: Bool) async throws { try await storage.setAutoImportDeviceMedia(enabled: enabled) }
    func addUser(username: String, password: String) async throws { try await storage.addUser(username: username, password: password) }
    func addRemoteFixedPath(name: String, path: String) async throws -> String { try await storage.addRemoteFixedPath(name: name, path: path) }
    func addRemoteDebugLocalApple(name: String) async throws -> String { try await storage.addRemoteDebugLocalApple(name: name) }
    func addRemoteS3(id: String, endpoint: String, bucket: String, region: String, pathPrefix: String, accessKey: String, secretKey: String) async throws -> String { try await storage.addRemoteS3(id: id, endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey) }
    func removeRemote(id: String) async throws { try await storage.removeRemote(id: id) }
    func initializeRemote(id: String) async throws { try await storage.initializeRemote(id: id) }
    func connectRemote(id: String) async throws { try await storage.connectRemote(id: id) }
    func hasUnpushedChanges(remoteID: String) async -> Bool { await storage.hasUnpushedChanges(remoteID: remoteID) }
    func push(remoteID: String) async throws -> UInt32 { try await storage.push(remoteID: remoteID) }
    func fetch(remoteID: String) async throws -> UInt32 { try await storage.fetch(remoteID: remoteID) }
    func sync() async throws -> FfiSyncResult { try await storage.sync() }
    func close() async { await storage.close() }
}
