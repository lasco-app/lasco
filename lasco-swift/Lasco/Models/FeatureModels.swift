import Foundation
import Observation

@MainActor
@Observable
final class RecentMediaModel {
    private(set) var media: [FfiMediaItem] = []
    var showingOrphans = false
    private let repository: any LibraryRepositoryProtocol

    init(repository: any LibraryRepositoryProtocol) {
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await load()
        for await change in stream {
            guard change == .all || change == .mediaList else { continue }
            await load()
        }
    }

    func load() async {
        do {
            media = try await (showingOrphans ? repository.orphanMediaByDate() : repository.mediaByDate())
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "recent media query failed: \(error)")
        }
    }

    func showMedia(id: String) async -> FfiMediaItem? {
        try? await repository.showMedia(id: id)
    }

    func albumsContainingMedia(id: String) async -> [FfiAlbum] {
        guard let ids = try? await repository.mediaAlbumIDs(mediaID: id), let albums = try? await repository.listAlbums() else { return [] }
        return albums.filter { ids.contains($0.albumId) }
    }

    func thumbnail(id: String) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: id)
    }

}

@MainActor
@Observable
final class AlbumListModel {
    private(set) var albums: [FfiAlbum] = []
    private let repository: any LibraryRepositoryProtocol

    init(repository: any LibraryRepositoryProtocol) {
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await load()
        for await change in stream {
            guard change == .all || change == .albumList || change == .mediaList else { continue }
            await load()
        }
    }

    func load() async {
        do {
            albums = try await repository.listAlbums()
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album list query failed: \(error)")
        }
    }

    func createAlbum(name: String, parentID: String?) {
        Task { try? await repository.createAlbum(name: name, parentID: parentID) }
    }

    func renameAlbum(id: String, name: String) {
        Task { try? await repository.renameAlbum(id: id, name: name) }
    }

    func reparentAlbum(id: String, parentID: String?) {
        Task { try? await repository.reparentAlbum(id: id, parentID: parentID) }
    }

    func deleteAlbum(id: String) {
        Task { try? await repository.deleteAlbum(id: id) }
    }

    func setAlbumThumbnail(albumID: String, mediaID: String?) {
        Task { try? await repository.setAlbumThumbnail(albumID: albumID, mediaID: mediaID) }
    }

    func addMedia(mediaID: String, albumID: String) async throws {
        try await repository.addMediaToAlbum(albumID: albumID, mediaID: mediaID)
    }

    func mediaInAlbum(albumID: String) async -> [FfiMediaItem] {
        (try? await repository.mediaInAlbum(albumID: albumID)) ?? []
    }

    func importMediaAsync(urls: [URL], albumID: String) async -> String? {
        let sources = urls.map { MediaImportSource(path: $0.path) }
        do {
            let ids = try await repository.importMediaBatch(sources, albumID: albumID)
            for (index, id) in ids.enumerated() {
                if let data = ThumbnailGenerator.generate(for: urls[index]) {
                    try? await repository.setMediaThumbnail(mediaID: id, data: data)
                }
            }
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    func addMediaToGroup(groupID: String, mediaID: String) {
        Task { try? await repository.addMediaToGroup(groupID: groupID, mediaID: mediaID) }
    }

    func removeMediaFromGroup(groupID: String, mediaID: String) {
        Task { try? await repository.removeMediaFromGroup(groupID: groupID, mediaID: mediaID) }
    }

    func createGroupFromSelectedMedia(mediaIDs: [String], albumID: String) {
        Task { try? await repository.createGroupFromSelectedMedia(mediaIDs: mediaIDs, albumID: albumID) }
    }

    func deleteGroup(groupID: String) {
        Task { try? await repository.deleteGroup(groupID: groupID) }
    }

    func removeMediaFromAlbum(albumID: String, mediaID: String) {
        Task { try? await repository.removeMediaFromAlbum(albumID: albumID, mediaID: mediaID) }
    }

    func moveMediaToAlbum(mediaID: String, fromAlbumID: String, toAlbumID: String) {
        Task { try? await repository.moveMedia(id: mediaID, from: fromAlbumID, to: toAlbumID) }
    }

    func albumListItemsSorted(albumID: String, ascending: Bool) -> [FfiAlbumItem] {
        []
    }

    func thumbnail(for mediaID: String) -> Data? {
        nil
    }

    func thumbnailAsync(for mediaID: String) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: mediaID)
    }

    func albumThumbnailMediaId(albumID: String) async -> String? {
        guard let album = albums.first(where: { $0.albumId == albumID }) else { return nil }
        return await thumbnailMediaID(for: album)
    }

    func thumbnailMediaID(for album: FfiAlbum) async -> String? {
        if let mediaID = album.thumbnailMediaId { return mediaID }
        let items = try? await repository.albumItems(albumID: album.albumId, ascending: false)
        return items?.compactMap { $0.media?.mediaId }.first
    }
}

@MainActor
@Observable
final class AlbumDetailModel {
    let albumID: String
    private(set) var items: [FfiAlbumItem] = []
    private(set) var groups: [FfiGroup] = []
    var ascending = false
    private let repository: any LibraryRepositoryProtocol

    init(albumID: String, repository: any LibraryRepositoryProtocol) {
        self.albumID = albumID
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await load()
        for await change in stream {
            guard change == .all || change == .albumList || change == .album(albumID) else { continue }
            await load()
        }
    }

    func load() async {
        do {
            async let loadedItems = repository.albumItems(albumID: albumID, ascending: ascending)
            async let loadedGroups = repository.albumGroups(albumID: albumID)
            items = try await loadedItems
            groups = try await loadedGroups
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album \(albumID) query failed: \(error)")
        }
    }

    func setSortAscending(_ ascending: Bool) async {
        self.ascending = ascending
        await load()
    }

    func thumbnail(id: String) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: id)
    }

    func addMedia(mediaID: String) async throws {
        try await repository.addMediaToAlbum(albumID: albumID, mediaID: mediaID)
    }

    func removeMedia(mediaID: String) async throws {
        try await repository.removeMediaFromAlbum(albumID: albumID, mediaID: mediaID)
    }

    func moveMedia(mediaID: String, to albumID: String) async throws {
        try await repository.moveMedia(id: mediaID, from: self.albumID, to: albumID)
    }

    func createGroupFromSelectedMedia(mediaIDs: [String]) async throws {
        try await repository.createGroupFromSelectedMedia(mediaIDs: mediaIDs, albumID: albumID)
    }

    func deleteGroup(groupID: String) async throws {
        try await repository.deleteGroup(groupID: groupID)
    }
}

@MainActor
@Observable
final class MediaDetailModel {
    let mediaID: String
    private(set) var media: FfiMediaItem?
    private(set) var containingAlbums: [FfiAlbum] = []
    private(set) var groupMedia: [String: [FfiMediaItem]] = [:]
    private let repository: any LibraryRepositoryProtocol

    init(mediaID: String, repository: any LibraryRepositoryProtocol) {
        self.mediaID = mediaID
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await load()
        for await change in stream {
            guard change == .all || change == .mediaList || change == .media(mediaID) || change == .albumList else { continue }
            await load()
        }
    }

    func load() async {
        do {
            media = try await repository.showMedia(id: mediaID)
            let albumIDs = try await repository.mediaContainingAlbumIDs(mediaID: mediaID, includeViaGroups: true)
            let albums = try await repository.listAlbums()
            containingAlbums = albums.filter { albumIDs.contains($0.albumId) }
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "media \(mediaID) query failed: \(error)")
        }
    }

    func rename(name: String?) async throws {
        try await repository.renameMedia(id: mediaID, name: name)
    }

    func thumbnail() async -> Data? {
        try? await repository.thumbnailAsync(mediaID: mediaID)
    }

    func mediaBytes() async -> Data? {
        try? await repository.mediaBytesAsync(mediaID: mediaID)
    }

    func thumbnail(id: String) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: id)
    }

    func mediaBytes(id: String) async -> Data? {
        try? await repository.mediaBytesAsync(mediaID: id)
    }
}

@MainActor
@Observable
final class StatusModel {
    private(set) var media: [FfiMediaItem] = []
    private(set) var localStateStats: FfiLocalStateStats?
    private(set) var mediaCountWithoutRemoteBackup: Int?
    private let repository: any LibraryRepositoryProtocol

    init(repository: any LibraryRepositoryProtocol) {
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await load()
        for await change in stream {
            guard change == .all || change == .localMutation else { continue }
            await load()
        }
    }

    func load() async {
        do {
            async let mediaQuery = repository.mediaByDate()
            async let statsQuery = repository.localStateStats()
            async let backupQuery = repository.mediaIDsWithoutRemoteBackup()
            media = try await mediaQuery
            localStateStats = try await statsQuery
            let unbacked = try await backupQuery
            mediaCountWithoutRemoteBackup = unbacked.isEmpty ? nil : unbacked.count
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "status query failed: \(error)")
        }
    }

    func cleanLocalMedia() async throws {
        try await repository.evictLocalData(mediaIDs: await repository.allMediaIDs())
        await load()
    }

    func cleanLocalThumbnails() async throws {
        try await repository.evictLocalThumbnails(mediaIDs: await repository.allMediaIDs())
        await load()
    }

    func hasUnpushedChanges(remoteID: String) async -> Bool {
        await repository.hasUnpushedChanges(remoteID: remoteID)
    }
}

@MainActor
@Observable
final class OperationsModel {
    private(set) var operationGroups: [FfiOperationGroup] = []
    private let repository: any LibraryRepositoryProtocol

    init(repository: any LibraryRepositoryProtocol) {
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await load()
        for await change in stream {
            guard change == .all || change == .localMutation else { continue }
            await load()
        }
    }

    func load() async {
        operationGroups = (try? await repository.listOperationGroups()) ?? []
    }
}
