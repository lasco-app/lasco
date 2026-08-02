import Foundation
import Observation

@MainActor
@Observable
final class RecentMediaModel {
    private(set) var media: [FfiMediaItem] = []
    private(set) var hasMore = false
    var showingOrphans = false
    private let repository: any LibraryRepositoryProtocol
    private var total = 0
    private var isLoading = false

    private static let pageSize = 100

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
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            if showingOrphans {
                total = try await repository.orphanMediaByDateCount()
                media = try await repository.orphanMediaByDate(offset: 0, limit: Self.pageSize)
            } else {
                total = try await repository.mediaByDateCount()
                media = try await repository.mediaByDate(offset: 0, limit: Self.pageSize)
            }
            hasMore = media.count < total
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "recent media query failed: \(error)")
        }
    }

    func loadMore() async {
        guard hasMore, !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let next: [FfiMediaItem]
            if showingOrphans {
                next = try await repository.orphanMediaByDate(offset: media.count, limit: Self.pageSize)
            } else {
                next = try await repository.mediaByDate(offset: media.count, limit: Self.pageSize)
            }
            media.append(contentsOf: next)
            hasMore = media.count < total && !next.isEmpty
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "recent media page query failed: \(error)")
        }
    }

    func showMedia(id: String) async -> FfiMediaItem? {
        try? await repository.showMedia(id: id)
    }

    func albumsContainingMedia(id: String) async -> [FfiAlbum] {
        guard let ids = try? await repository.mediaAlbumIDs(mediaID: id) else { return [] }
        return (try? await repository.albums(withIDs: Set(ids))) ?? []
    }

    func thumbnail(id: String) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: id)
    }

}

@MainActor
@Observable
final class AlbumListModel {
    private let repository: any LibraryRepositoryProtocol
    private var albumsByParent: [String: [FfiAlbum]] = [:]
    private var totalsByParent: [String: Int] = [:]
    private var loadingParents = Set<String>()

    private static let pageSize = 100

    init(repository: any LibraryRepositoryProtocol) {
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await reloadLoadedParents()
        for await change in stream {
            guard change == .all || change == .albumList || change == .mediaList else { continue }
            await reloadLoadedParents()
        }
    }

    func albums(parentID: String?) -> [FfiAlbum] {
        albumsByParent[key(for: parentID)] ?? []
    }

    func load(parentID: String?) async {
        let key = key(for: parentID)
        guard !loadingParents.contains(key) else { return }
        loadingParents.insert(key)
        defer { loadingParents.remove(key) }
        do {
            totalsByParent[key] = try await repository.albumsCount(parentID: parentID)
            albumsByParent[key] = try await repository.albums(parentID: parentID, offset: 0, limit: Self.pageSize)
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album page query failed: \(error)")
        }
    }

    func loadMore(parentID: String?) async {
        let key = key(for: parentID)
        guard !loadingParents.contains(key), let current = albumsByParent[key], current.count < (totalsByParent[key] ?? 0) else { return }
        loadingParents.insert(key)
        defer { loadingParents.remove(key) }
        do {
            let next = try await repository.albums(parentID: parentID, offset: current.count, limit: Self.pageSize)
            albumsByParent[key, default: []].append(contentsOf: next)
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album next-page query failed: \(error)")
        }
    }

    private func reloadLoadedParents() async {
        let keys = albumsByParent.isEmpty ? [Self.rootKey] : Array(albumsByParent.keys)
        albumsByParent = [:]
        totalsByParent = [:]
        for key in keys {
            await load(parentID: key == Self.rootKey ? nil : key)
        }
    }

    private func key(for parentID: String?) -> String { parentID ?? Self.rootKey }
    private static let rootKey = "<root>"

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

    func thumbnailMediaID(for album: FfiAlbum) async -> String? {
        if let mediaID = album.thumbnailMediaId { return mediaID }
        let items = try? await repository.albumItems(albumID: album.albumId, ascending: false, offset: 0, limit: 1)
        return items?.compactMap { $0.media?.mediaId }.first
    }
}

@MainActor
@Observable
final class AlbumDetailModel {
    let albumID: String
    private(set) var items: [FfiAlbumItem] = []
    private(set) var groups: [FfiGroup] = []
    private(set) var hasMore = false
    var ascending = false
    private let repository: any LibraryRepositoryProtocol
    private var total = 0
    private var isLoading = false

    private static let pageSize = 100

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
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            async let loadedCount = repository.albumItemsCount(albumID: albumID)
            async let loadedItems = repository.albumItems(albumID: albumID, ascending: ascending, offset: 0, limit: Self.pageSize)
            async let loadedGroups = repository.albumGroups(albumID: albumID)
            total = try await loadedCount
            items = try await loadedItems
            groups = try await loadedGroups
            hasMore = items.count < total
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album \(albumID) query failed: \(error)")
        }
    }

    func loadMore() async {
        guard hasMore, !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let next = try await repository.albumItems(
                albumID: albumID,
                ascending: ascending,
                offset: items.count,
                limit: Self.pageSize
            )
            items.append(contentsOf: next)
            hasMore = items.count < total && !next.isEmpty
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album \(albumID) next-page query failed: \(error)")
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
            containingAlbums = try await repository.albums(withIDs: Set(albumIDs))
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
    private(set) var mediaCount = 0
    private(set) var localStateStats: FfiLocalStateStats?
    private(set) var mediaCountWithoutRemoteBackup: Int?
    private(set) var syncedByRemoteID: [String: Bool] = [:]
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
            async let mediaCountQuery = repository.mediaByDateCount()
            async let statsQuery = repository.localStateStats()
            async let backupQuery = repository.mediaIDsWithoutRemoteBackup()
            async let sessionQuery = repository.sessionSnapshot()
            mediaCount = try await mediaCountQuery
            localStateStats = try await statsQuery
            let unbacked = try await backupQuery
            mediaCountWithoutRemoteBackup = unbacked.isEmpty ? nil : unbacked.count
            let session = try await sessionQuery
            var syncStatus: [String: Bool] = [:]
            for remote in session.remotes {
                let hasUnpushedChanges = await repository.hasUnpushedChanges(remoteID: remote.id)
                syncStatus[remote.id] = !hasUnpushedChanges
            }
            syncedByRemoteID = syncStatus
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

    func isSynced(remoteID: String) -> Bool {
        syncedByRemoteID[remoteID] ?? true
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
