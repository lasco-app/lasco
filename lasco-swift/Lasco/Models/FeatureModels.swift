import Foundation
import Observation

enum MediaDetailSource: Hashable, Sendable {
    case homeByDate
    case orphansByDate
    case albumByDate(albumID: FfiAlbumUuid, ascending: Bool)

    var currentAlbumID: FfiAlbumUuid? {
        guard case .albumByDate(let albumID, _) = self else { return nil }
        return albumID
    }
}

@MainActor
@Observable
final class RecentMediaModel {
    private(set) var media: [FfiMediaItem] = []
    private(set) var hasMore = false
    var showingOrphans = false
    private let repository: any LibraryRepositoryProtocol
    private(set) var totalCount = 0
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
                totalCount = try await repository.orphanMediaByDateCount()
                media = try await repository.orphanMediaByDate(offset: 0, limit: Self.pageSize)
            } else {
                totalCount = try await repository.mediaByDateCount()
                media = try await repository.mediaByDate(offset: 0, limit: Self.pageSize)
            }
            hasMore = media.count < totalCount
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
            hasMore = media.count < totalCount && !next.isEmpty
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "recent media page query failed: \(error)")
        }
    }

    func showMedia(id: FfiMediaUuid) async -> FfiMediaItem? {
        try? await repository.showMedia(id: id)
    }

    func albumsContainingMedia(id: FfiMediaUuid) async -> [FfiAlbum] {
        guard let ids = try? await repository.mediaAlbumIDs(mediaID: id) else { return [] }
        return (try? await repository.albums(withIDs: Set(ids))) ?? []
    }

    func thumbnail(id: FfiMediaUuid) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: id)
    }

}

@MainActor
@Observable
final class AlbumListModel {
    private let repository: any LibraryRepositoryProtocol
    private var albumsByParent: [FfiAlbumUuid?: [FfiAlbum]] = [:]
    private var totalsByParent: [FfiAlbumUuid?: Int] = [:]
    private var loadingParents = Set<FfiAlbumUuid?>()

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

    func albums(parentID: FfiAlbumUuid?) -> [FfiAlbum] {
        albumsByParent[parentID] ?? []
    }

    func load(parentID: FfiAlbumUuid?) async {
        guard !loadingParents.contains(parentID) else { return }
        loadingParents.insert(parentID)
        defer { loadingParents.remove(parentID) }
        do {
            totalsByParent[parentID] = try await repository.albumsCount(parentID: parentID)
            albumsByParent[parentID] = try await repository.albums(parentID: parentID, offset: 0, limit: Self.pageSize)
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album page query failed: \(error)")
        }
    }

    func loadMore(parentID: FfiAlbumUuid?) async {
        guard !loadingParents.contains(parentID), let current = albumsByParent[parentID], current.count < (totalsByParent[parentID] ?? 0) else { return }
        loadingParents.insert(parentID)
        defer { loadingParents.remove(parentID) }
        do {
            let next = try await repository.albums(parentID: parentID, offset: current.count, limit: Self.pageSize)
            albumsByParent[parentID, default: []].append(contentsOf: next)
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album next-page query failed: \(error)")
        }
    }

    private func reloadLoadedParents() async {
        let keys = albumsByParent.isEmpty ? [nil] : Array(albumsByParent.keys)
        albumsByParent = [:]
        totalsByParent = [:]
        for key in keys {
            await load(parentID: key)
        }
    }

    func createAlbum(name: String, parentID: FfiAlbumUuid?) {
        Task { try? await repository.createAlbum(name: name, parentID: parentID) }
    }

    func renameAlbum(id: FfiAlbumUuid, name: String) {
        Task { try? await repository.renameAlbum(id: id, name: name) }
    }

    func reparentAlbum(id: FfiAlbumUuid, parentID: FfiAlbumUuid?) {
        Task { try? await repository.reparentAlbum(id: id, parentID: parentID) }
    }

    func deleteAlbum(id: FfiAlbumUuid) {
        Task { try? await repository.deleteAlbum(id: id) }
    }

    func setAlbumThumbnail(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid?) {
        Task { try? await repository.setAlbumThumbnail(albumID: albumID, mediaID: mediaID) }
    }

    func addMedia(mediaID: FfiMediaUuid, albumID: FfiAlbumUuid) async throws {
        try await repository.addMediaToAlbum(albumID: albumID, mediaID: mediaID)
    }

    func mediaInAlbum(albumID: FfiAlbumUuid) async -> [FfiMediaItem] {
        (try? await repository.mediaInAlbum(albumID: albumID)) ?? []
    }

    func importMediaAsync(urls: [URL], albumID: FfiAlbumUuid) async -> String? {
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

    func addMediaToGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) {
        Task { try? await repository.addMediaToGroup(groupID: groupID, mediaID: mediaID) }
    }

    func removeMediaFromGroup(groupID: FfiGroupUuid, mediaID: FfiMediaUuid) {
        Task { try? await repository.removeMediaFromGroup(groupID: groupID, mediaID: mediaID) }
    }

    func createGroupFromSelectedMedia(mediaIDs: [FfiMediaUuid], albumID: FfiAlbumUuid) {
        Task { try? await repository.createGroupFromSelectedMedia(mediaIDs: mediaIDs, albumID: albumID) }
    }

    func deleteGroup(groupID: FfiGroupUuid) {
        Task { try? await repository.deleteGroup(groupID: groupID) }
    }

    func removeMediaFromAlbum(albumID: FfiAlbumUuid, mediaID: FfiMediaUuid) {
        Task { try? await repository.removeMediaFromAlbum(albumID: albumID, mediaID: mediaID) }
    }

    func moveMediaToAlbum(mediaID: FfiMediaUuid, fromAlbumID: FfiAlbumUuid, toAlbumID: FfiAlbumUuid) {
        Task { try? await repository.moveMedia(id: mediaID, from: fromAlbumID, to: toAlbumID) }
    }

    func albumListItemsSorted(albumID: FfiAlbumUuid, ascending: Bool) -> [FfiAlbumItem] {
        []
    }

    func thumbnail(for mediaID: FfiMediaUuid) -> Data? {
        nil
    }

    func thumbnailAsync(for mediaID: FfiMediaUuid) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: mediaID)
    }

    func thumbnailMediaID(for album: FfiAlbum) async -> FfiMediaUuid? {
        if let mediaID = album.thumbnailMediaId { return mediaID }
        let items = try? await repository.albumItems(albumID: album.albumId, ascending: false, offset: 0, limit: 1)
        return items?.compactMap { $0.media?.mediaId }.first
    }
}

@MainActor
@Observable
final class AlbumDetailModel {
    let albumID: FfiAlbumUuid
    private(set) var items: [FfiAlbumItem] = []
    private(set) var groups: [FfiGroup] = []
    private(set) var hasMore = false
    var ascending = false
    private let repository: any LibraryRepositoryProtocol
    private(set) var totalCount = 0
    private var isLoading = false

    private static let pageSize = 100

    init(albumID: FfiAlbumUuid, repository: any LibraryRepositoryProtocol) {
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
            totalCount = try await loadedCount
            items = try await loadedItems
            groups = try await loadedGroups
            hasMore = items.count < totalCount
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
            hasMore = items.count < totalCount && !next.isEmpty
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "album \(albumID) next-page query failed: \(error)")
        }
    }

    func setSortAscending(_ ascending: Bool) async {
        self.ascending = ascending
        await load()
    }

    func thumbnail(id: FfiMediaUuid) async -> Data? {
        try? await repository.thumbnailAsync(mediaID: id)
    }

    func addMedia(mediaID: FfiMediaUuid) async throws {
        try await repository.addMediaToAlbum(albumID: albumID, mediaID: mediaID)
    }

    func removeMedia(mediaID: FfiMediaUuid) async throws {
        try await repository.removeMediaFromAlbum(albumID: albumID, mediaID: mediaID)
    }

    func moveMedia(mediaID: FfiMediaUuid, to albumID: FfiAlbumUuid) async throws {
        try await repository.moveMedia(id: mediaID, from: self.albumID, to: albumID)
    }

    func createGroupFromSelectedMedia(mediaIDs: [FfiMediaUuid]) async throws {
        try await repository.createGroupFromSelectedMedia(mediaIDs: mediaIDs, albumID: albumID)
    }

    func deleteGroup(groupID: FfiGroupUuid) async throws {
        try await repository.deleteGroup(groupID: groupID)
    }
}

@MainActor
@Observable
final class StatusModel {
    private(set) var mediaCount = 0
    private(set) var localStateStats: FfiLocalStateStats?
    private(set) var syncedByRemoteID: [FfiRemoteUuid: Bool] = [:]
    private(set) var shortfallByRemoteID: [FfiRemoteUuid: FfiRemoteMediaShortfall] = [:]
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
            async let sessionQuery = repository.sessionSnapshot()
            mediaCount = try await mediaCountQuery
            localStateStats = try await statsQuery
            let session = try await sessionQuery
            var syncStatus: [FfiRemoteUuid: Bool] = [:]
            var shortfalls: [FfiRemoteUuid: FfiRemoteMediaShortfall] = [:]
            for remote in session.remotes {
                let hasUnpushedChanges = await repository.hasUnpushedChanges(remoteID: remote.remoteId)
                syncStatus[remote.remoteId] = !hasUnpushedChanges
                // Media a remote has never been told about cannot be expected on it, so the
                // shortfall is only worth reading once every operation has reached it. That
                // also keeps this off the hot path during an import, where nothing is pushed.
                guard !hasUnpushedChanges else { continue }
                shortfalls[remote.remoteId] = try? await repository.remoteMediaShortfall(remoteID: remote.remoteId)
            }
            syncedByRemoteID = syncStatus
            shortfallByRemoteID = shortfalls
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "status query failed: \(error)")
        }
    }

    /// The number of media that clearing local media would leave with no known copy anywhere.
    /// Queried when the user reaches for the action rather than on every refresh, because it
    /// stats a file per media and is only ever read to answer that one question.
    func mediaCountLostIfLocalMediaCleared() async -> Int {
        do {
            return try await repository.mediaCountLostIfLocalMediaCleared()
        } catch {
            AppLogger.log(.error, "lost-media count failed: \(error)")
            return 0
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

    func isSynced(remoteID: FfiRemoteUuid) -> Bool {
        syncedByRemoteID[remoteID] ?? true
    }

    func shortfall(remoteID: FfiRemoteUuid) -> FfiRemoteMediaShortfall? {
        shortfallByRemoteID[remoteID]
    }
}

@MainActor
@Observable
final class OperationsModel {
    private(set) var operations: [FfiCrdtOperation] = []
    private(set) var isLoading = false
    private(set) var hasMore = true
    private let repository: any LibraryRepositoryProtocol
    private var nextStartPos: UInt64 = 0
    private let pageSize: UInt64 = 50

    init(repository: any LibraryRepositoryProtocol) {
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await refresh()
        for await change in stream {
            guard change == .all || change == .localMutation else { continue }
            await refresh()
        }
    }

    func refresh() async {
        operations = []
        nextStartPos = 0
        hasMore = true
        await loadMore()
    }

    func loadMore() async {
        guard hasMore, !isLoading else { return }
        isLoading = true
        defer { isLoading = false }

        let endPosExclusive = nextStartPos + pageSize
        let page = (try? await repository.listOperations(
            startPos: nextStartPos,
            endPosExclusive: endPosExclusive
        )) ?? []
        operations.append(contentsOf: page)
        nextStartPos = endPosExclusive
        hasMore = page.count == Int(pageSize)
    }
}
