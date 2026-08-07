import Foundation
import Observation

enum MediaDetailSource: Hashable {
    case homeByDate
    case orphansByDate
    case albumByDate(albumID: FfiAlbumUuid, ascending: Bool)

    var currentAlbumID: FfiAlbumUuid? {
        guard case .albumByDate(let albumID, _) = self else { return nil }
        return albumID
    }
}

struct MediaDetailNeighbors {
    let previous: AlbumItem?
    let current: AlbumItem
    let next: AlbumItem?
    let currentPosition: Int

    var items: [AlbumItem] { [previous, current, next].compactMap { $0 } }
    var currentIndex: Int { previous == nil ? 0 : 1 }
}

enum MediaDetailLoadState: Equatable {
    case loading
    case content
    case empty
    case error
}

private func detailItem(from item: FfiAlbumItem) throws -> AlbumItem {
    if let media = item.media { return .media(media) }
    if let group = item.group { return .group(group) }
    throw LascoError.NotFound
}

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
    private var total = 0
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
final class MediaDetailModel {
    let source: MediaDetailSource
    let startPosition: Int
    private(set) var state: MediaDetailLoadState = .loading
    private(set) var neighbors: MediaDetailNeighbors?
    private(set) var totalCount: Int?
    private(set) var currentMedia: FfiMediaItem?
    private(set) var containingAlbums: [FfiAlbum] = []
    private(set) var groupMedia: [FfiGroupUuid: [FfiMediaItem]] = [:]
    private let repository: any LibraryRepositoryProtocol
    private var groupLoads = Set<FfiGroupUuid>()
    private var requestID = 0
    private var navigationTask: Task<Void, Never>?

    init(source: MediaDetailSource, startPosition: Int, repository: any LibraryRepositoryProtocol) {
        self.source = source
        self.startPosition = startPosition
        self.repository = repository
    }

    func start() async {
        let stream = await repository.changes()
        await loadNeighbors(at: startPosition)
        for await change in stream {
            guard shouldReload(for: change) else { continue }
            await loadNeighbors(at: neighbors?.currentPosition ?? startPosition)
        }
    }

    func move(by delta: Int) {
        guard let neighbors, delta != 0 else { return }
        if delta < 0, neighbors.previous == nil { return }
        if delta > 0, neighbors.next == nil { return }
        navigationTask?.cancel()
        let position = neighbors.currentPosition + delta
        navigationTask = Task { [weak self] in
            await self?.loadNeighbors(at: position)
        }
    }

    func loadGroupMediaIfNeeded(groupID: FfiGroupUuid) async {
        guard groupMedia[groupID] == nil, groupLoads.insert(groupID).inserted else { return }
        defer { groupLoads.remove(groupID) }
        do {
            let media = try await repository.groupMedia(groupID: groupID)
            guard !Task.isCancelled, neighbors?.items.contains(where: { if case .group(groupID) = $0.id { return true }; return false }) == true else { return }
            groupMedia[groupID] = media
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "group \(groupID) query failed: \(error)")
        }
    }

    func refreshMedia(id: FfiMediaUuid) async {
        do {
            let media = try await repository.showMedia(id: id)
            let albumIDs = try await repository.mediaContainingAlbumIDs(
                mediaID: id, includeViaGroups: true
            )
            let albums = try await repository.albums(withIDs: Set(albumIDs))
            guard !Task.isCancelled else { return }
            currentMedia = media
            containingAlbums = albums
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "media \(id) metadata query failed: \(error)")
        }
    }

    private func shouldReload(for change: LibraryChange) -> Bool {
        switch source {
        case .homeByDate, .orphansByDate:
            return change == .all || change == .mediaList || change == .albumList
        case .albumByDate(let albumID, _):
            return change == .all || change == .mediaList || change == .albumList || change == .album(albumID)
        }
    }

    private func sourceCount() async throws -> Int {
        switch source {
        case .homeByDate:
            return try await repository.mediaByDateCount()
        case .orphansByDate:
            return try await repository.orphanMediaByDateCount()
        case .albumByDate(let albumID, _):
            return try await repository.albumItemsCount(albumID: albumID)
        }
    }

    private func loadNeighbors(at position: Int) async {
        guard position >= 0 else { return }
        requestID += 1
        let requestID = requestID
        state = neighbors == nil ? .loading : .content
        do {
            async let total = sourceCount()
            let loaded: MediaDetailNeighbors
            switch source {
            case .homeByDate:
                let value = try await repository.mediaByDateNeighbors(position: position)
                loaded = MediaDetailNeighbors(
                    previous: value.previous.map(AlbumItem.media), current: .media(value.current),
                    next: value.next.map(AlbumItem.media), currentPosition: position
                )
            case .orphansByDate:
                let value = try await repository.orphanMediaByDateNeighbors(position: position)
                loaded = MediaDetailNeighbors(
                    previous: value.previous.map(AlbumItem.media), current: .media(value.current),
                    next: value.next.map(AlbumItem.media), currentPosition: position
                )
            case .albumByDate(let albumID, let ascending):
                let value = try await repository.albumItemsByDateNeighbors(
                    albumID: albumID, ascending: ascending, position: position
                )
                loaded = MediaDetailNeighbors(
                    previous: try value.previous.map(detailItem(from:)), current: try detailItem(from: value.current),
                    next: try value.next.map(detailItem(from:)), currentPosition: position
                )
            }
            let loadedTotal = try await total
            guard !Task.isCancelled, requestID == self.requestID else { return }
            neighbors = loaded
            totalCount = loadedTotal
            groupMedia = groupMedia.filter { key, _ in
                loaded.items.contains { if case .group(key) = $0.id { return true }; return false }
            }
            currentMedia = nil
            containingAlbums = []
            var refreshedMedia: FfiMediaItem?
            var refreshedAlbums: [FfiAlbum] = []
            if case .media(let item) = loaded.current {
                refreshedMedia = try await repository.showMedia(id: item.mediaId)
                let albumIDs = try await repository.mediaContainingAlbumIDs(
                    mediaID: item.mediaId, includeViaGroups: true
                )
                refreshedAlbums = try await repository.albums(withIDs: Set(albumIDs))
            }
            guard !Task.isCancelled, requestID == self.requestID else { return }
            currentMedia = refreshedMedia
            containingAlbums = refreshedAlbums
            state = .content
        } catch is CancellationError {
        } catch LascoError.NotFound {
            guard requestID == self.requestID else { return }
            neighbors = nil
            totalCount = nil
            currentMedia = nil
            containingAlbums = []
            state = .empty
        } catch {
            guard requestID == self.requestID else { return }
            AppLogger.log(.error, "media detail query failed: \(error)")
            state = .error
        }
    }

    func rename(mediaID: FfiMediaUuid, name: String?) async throws {
        try await repository.renameMedia(id: mediaID, name: name)
    }
}

@MainActor
@Observable
final class StatusModel {
    private(set) var mediaCount = 0
    private(set) var localStateStats: FfiLocalStateStats?
    private(set) var mediaCountWithoutRemoteBackup: Int?
    private(set) var syncedByRemoteID: [FfiRemoteUuid: Bool] = [:]
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
            var syncStatus: [FfiRemoteUuid: Bool] = [:]
            for remote in session.remotes {
                let hasUnpushedChanges = await repository.hasUnpushedChanges(remoteID: remote.remoteId)
                syncStatus[remote.remoteId] = !hasUnpushedChanges
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

    func isSynced(remoteID: FfiRemoteUuid) -> Bool {
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
