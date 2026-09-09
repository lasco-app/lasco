import Foundation
import Observation

struct MediaGallerySeed: Hashable {
    let source: MediaDetailSource
    let position: Int
    let item: AlbumItem
    let totalCount: Int
}

enum MediaGalleryPageState {
    case loading
    case loaded(AlbumItem)
    case failed(String)
}

protocol MediaGalleryDataLoading: Sendable {
    func changes() async -> AsyncStream<LibraryChange>
    func count() async throws -> Int
    func items(in range: Range<Int>) async throws -> [AlbumItem]
    func position(of itemID: AlbumItem.ID, count: Int) async throws -> Int?
    func groupMedia(groupID: FfiGroupUuid) async throws -> [FfiMediaItem]
    func media(id: FfiMediaUuid) async throws -> FfiMediaItem
    func containingAlbumIDs(mediaID: FfiMediaUuid) async throws -> [FfiAlbumUuid]
    func albums(withIDs ids: Set<FfiAlbumUuid>) async throws -> [FfiAlbum]
}

struct MediaGalleryDataSource: MediaGalleryDataLoading {
    let source: MediaDetailSource
    let repository: any LibraryRepositoryProtocol

    func changes() async -> AsyncStream<LibraryChange> {
        await repository.changes()
    }

    func count() async throws -> Int {
        switch source {
        case .homeByDate:
            return try await repository.mediaByDateCount()
        case .orphansByDate:
            return try await repository.orphanMediaByDateCount()
        case .albumByDate(let albumID, _):
            return try await repository.albumItemsCount(albumID: albumID)
        }
    }

    func items(in range: Range<Int>) async throws -> [AlbumItem] {
        guard !range.isEmpty else { return [] }
        switch source {
        case .homeByDate:
            return try await repository.mediaByDate(offset: range.lowerBound, limit: range.count)
                .map(AlbumItem.media)
        case .orphansByDate:
            return try await repository.orphanMediaByDate(offset: range.lowerBound, limit: range.count)
                .map(AlbumItem.media)
        case .albumByDate(let albumID, let ascending):
            return try await repository.albumItems(
                albumID: albumID,
                ascending: ascending,
                offset: range.lowerBound,
                limit: range.count
            ).map { item in
                if let media = item.media { return .media(media) }
                if let group = item.group { return .group(group) }
                throw LascoError.NotFound
            }
        }
    }

    /// Reconciliation is deliberately ID-based. It is only used after a library
    /// mutation, so scanning lightweight, paged records does not affect normal
    /// gallery navigation or its bounded retained window.
    func position(of itemID: AlbumItem.ID, count: Int) async throws -> Int? {
        let reconciliationPageSize = 100
        for offset in stride(from: 0, to: count, by: reconciliationPageSize) {
            try Task.checkCancellation()
            let end = min(offset + reconciliationPageSize, count)
            let items = try await items(in: offset..<end)
            if let localIndex = items.firstIndex(where: { $0.id == itemID }) {
                return offset + localIndex
            }
        }
        return nil
    }

    func groupMedia(groupID: FfiGroupUuid) async throws -> [FfiMediaItem] {
        try await repository.groupMedia(groupID: groupID)
    }

    func media(id: FfiMediaUuid) async throws -> FfiMediaItem {
        try await repository.showMedia(id: id)
    }

    func containingAlbumIDs(mediaID: FfiMediaUuid) async throws -> [FfiAlbumUuid] {
        try await repository.mediaContainingAlbumIDs(mediaID: mediaID, includeViaGroups: true)
    }

    func albums(withIDs ids: Set<FfiAlbumUuid>) async throws -> [FfiAlbum] {
        try await repository.albums(withIDs: ids)
    }
}

@MainActor
@Observable
final class MediaGallerySession {
    let source: MediaDetailSource
    private(set) var totalCount: Int
    private(set) var selectedPosition: Int
    private(set) var pages: [Int: MediaGalleryPageState]
    private(set) var currentMedia: FfiMediaItem?
    private(set) var containingAlbums: [FfiAlbum] = []
    private(set) var groupMedia: [FfiGroupUuid: [FfiMediaItem]] = [:]

    private let dataSource: any MediaGalleryDataLoading
    private var loadRequests: [Int: LoadRequest] = [:]
    private var groupLoads = Set<FfiGroupUuid>()
    private var metadataRequestID = UUID()
    private var selectionRevision = 0

    private static let batchSize = 8
    private static let prefetchRadius = 2
    private static let retainedPageRadius = 6

    private struct LoadRequest {
        let id: UUID
        let range: Range<Int>
        let task: Task<Void, Never>
    }

    convenience init(seed: MediaGallerySeed, repository: any LibraryRepositoryProtocol) {
        self.init(
            seed: seed,
            dataSource: MediaGalleryDataSource(source: seed.source, repository: repository)
        )
    }

    init(seed: MediaGallerySeed, dataSource: any MediaGalleryDataLoading) {
        source = seed.source
        totalCount = max(seed.totalCount, seed.position + 1)
        selectedPosition = seed.position
        pages = [seed.position: .loaded(seed.item)]
        self.dataSource = dataSource
    }

    var selectedItem: AlbumItem? {
        item(at: selectedPosition)
    }

    func item(at position: Int) -> AlbumItem? {
        guard case .loaded(let item) = pages[position] else { return nil }
        return item
    }

    func state(at position: Int) -> MediaGalleryPageState {
        pages[position] ?? .loading
    }

    func start() async {
        let changes = await dataSource.changes()
        await refreshCount()
        prepareWindow()

        for await change in changes {
            guard !Task.isCancelled else { return }
            if shouldReconcile(for: change) {
                await reconcileAfterMutation()
            } else if case .media(let mediaID) = change,
                      currentMedia?.mediaId == mediaID {
                await refreshMedia(id: mediaID)
            }
        }
    }

    func stop() {
        cancelAllLoads()
        metadataRequestID = UUID()
    }

    func select(_ position: Int) {
        guard (0..<totalCount).contains(position) else { return }
        guard selectedPosition != position else {
            prepareWindow()
            return
        }

        selectedPosition = position
        selectionRevision += 1
        currentMedia = nil
        containingAlbums = []
        metadataRequestID = UUID()
        trimRetainedState()
        cancelObsoleteLoads()
        prepareWindow()
    }

    func move(by delta: Int) {
        select(selectedPosition + delta)
    }

    func ensureLoaded(at position: Int) async {
        guard (0..<totalCount).contains(position) else { return }
        let batchStart = batchStart(containing: position)
        if let request = loadRequests[batchStart] {
            await request.task.value
            return
        }
        guard item(at: position) == nil else { return }
        startLoad(batchStart: batchStart, priority: position == selectedPosition ? .userInitiated : .utility)
        await loadRequests[batchStart]?.task.value
    }

    func retry(position: Int) {
        guard (0..<totalCount).contains(position) else { return }
        pages[position] = .loading
        let start = batchStart(containing: position)
        loadRequests[start]?.task.cancel()
        loadRequests[start] = nil
        startLoad(batchStart: start, priority: position == selectedPosition ? .userInitiated : .utility)
    }

    func loadGroupMediaIfNeeded(groupID: FfiGroupUuid) async {
        guard groupMedia[groupID] == nil, groupLoads.insert(groupID).inserted else { return }
        defer { groupLoads.remove(groupID) }
        do {
            let media = try await dataSource.groupMedia(groupID: groupID)
            guard !Task.isCancelled,
                  pages.values.contains(where: {
                      if case .loaded(.group(let group)) = $0 { return group.groupId == groupID }
                      return false
                  }) else { return }
            groupMedia[groupID] = media
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "group \(groupID) query failed: \(error)")
        }
    }

    func refreshMedia(id: FfiMediaUuid) async {
        let requestID = UUID()
        metadataRequestID = requestID
        do {
            let media = try await dataSource.media(id: id)
            let albumIDs = try await dataSource.containingAlbumIDs(mediaID: id)
            let albums = try await dataSource.albums(withIDs: Set(albumIDs))
            guard !Task.isCancelled,
                  requestID == metadataRequestID,
                  selectedPageContains(mediaID: id) else { return }
            currentMedia = media
            containingAlbums = albums
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "media \(id) metadata query failed: \(error)")
        }
    }

    private func selectedPageContains(mediaID: FfiMediaUuid) -> Bool {
        switch selectedItem {
        case .media(let media):
            return media.mediaId == mediaID
        case .group(let group):
            return groupMedia[group.groupId]?.contains(where: { $0.mediaId == mediaID }) == true
        case nil:
            return false
        }
    }

    private func refreshCount() async {
        do {
            let count = try await dataSource.count()
            guard !Task.isCancelled else { return }
            totalCount = max(count, selectedPosition + 1)
        } catch is CancellationError {
        } catch {
            // The seed remains renderable and navigable within its known bounds.
            AppLogger.log(.error, "gallery count query failed: \(error)")
        }
    }

    private func prepareWindow() {
        guard totalCount > 0 else { return }
        let lower = max(0, selectedPosition - Self.prefetchRadius)
        let upper = min(totalCount - 1, selectedPosition + Self.prefetchRadius)
        let selectedBatch = batchStart(containing: selectedPosition)
        startLoad(batchStart: selectedBatch, priority: .userInitiated)

        for position in lower...upper {
            let start = batchStart(containing: position)
            guard start != selectedBatch else { continue }
            startLoad(batchStart: start, priority: .utility)
        }
    }

    private func startLoad(batchStart: Int, priority: TaskPriority) {
        guard loadRequests[batchStart] == nil else { return }
        let range = batchRange(startingAt: batchStart)
        guard !range.isEmpty,
              range.contains(where: { item(at: $0) == nil }) else { return }

        for position in range where item(at: position) == nil {
            pages[position] = .loading
        }

        let requestID = UUID()
        let task = Task(priority: priority) { [weak self] in
            guard let self else { return }
            await self.performLoad(range: range, batchStart: batchStart, requestID: requestID)
        }
        loadRequests[batchStart] = LoadRequest(id: requestID, range: range, task: task)
    }

    private func performLoad(range: Range<Int>, batchStart: Int, requestID: UUID) async {
        do {
            let loaded = try await dataSource.items(in: range)
            guard !Task.isCancelled,
                  loadRequests[batchStart]?.id == requestID else { return }

            for (position, item) in zip(range, loaded) {
                pages[position] = .loaded(item)
            }
            if loaded.count < range.count {
                for position in (range.lowerBound + loaded.count)..<range.upperBound {
                    pages[position] = .failed("This item is no longer available.")
                }
            }
            loadRequests[batchStart] = nil
            trimRetainedState()
        } catch is CancellationError {
            guard loadRequests[batchStart]?.id == requestID else { return }
            loadRequests[batchStart] = nil
        } catch {
            guard loadRequests[batchStart]?.id == requestID else { return }
            for position in range where item(at: position) == nil {
                pages[position] = .failed(error.localizedDescription)
            }
            loadRequests[batchStart] = nil
            AppLogger.log(.error, "gallery page query failed: \(error)")
        }
    }

    private func reconcileAfterMutation() async {
        while !Task.isCancelled {
            let revision = selectionRevision
            let selectedID = selectedItem?.id
            let oldPosition = selectedPosition
            do {
                let count = try await dataSource.count()
                guard !Task.isCancelled else { return }
                guard revision == selectionRevision,
                      selectedID == selectedItem?.id else { continue }
                guard count > 0 else {
                    cancelAllLoads()
                    totalCount = 0
                    pages = [:]
                    currentMedia = nil
                    containingAlbums = []
                    return
                }

                let reconciledPosition: Int
                if let selectedID,
                   let position = try await dataSource.position(of: selectedID, count: count) {
                    reconciledPosition = position
                } else {
                    reconciledPosition = min(oldPosition, count - 1)
                }
                guard !Task.isCancelled else { return }
                guard revision == selectionRevision,
                      selectedID == selectedItem?.id else { continue }

                let retainedSelected = selectedID.flatMap { id in
                    pages.values.compactMap { state -> AlbumItem? in
                        guard case .loaded(let item) = state, item.id == id else { return nil }
                        return item
                    }.first
                }
                cancelAllLoads()
                totalCount = count
                selectedPosition = reconciledPosition
                pages = retainedSelected.map { [reconciledPosition: .loaded($0)] } ?? [:]
                currentMedia = nil
                containingAlbums = []
                metadataRequestID = UUID()
                prepareWindow()
                return
            } catch is CancellationError {
                return
            } catch {
                AppLogger.log(.error, "gallery reconciliation failed: \(error)")
                return
            }
        }
    }

    private func shouldReconcile(for change: LibraryChange) -> Bool {
        switch source {
        case .homeByDate, .orphansByDate:
            return change == .all || change == .mediaList || change == .albumList
        case .albumByDate(let albumID, _):
            return change == .all || change == .mediaList || change == .albumList || change == .album(albumID)
        }
    }

    private func batchStart(containing position: Int) -> Int {
        position / Self.batchSize * Self.batchSize
    }

    private func batchRange(startingAt start: Int) -> Range<Int> {
        start..<min(start + Self.batchSize, totalCount)
    }

    private func trimRetainedState() {
        let keep = (selectedPosition - Self.retainedPageRadius)...(selectedPosition + Self.retainedPageRadius)
        pages = pages.filter { keep.contains($0.key) }
        let retainedGroups = Set(pages.values.compactMap { state -> FfiGroupUuid? in
            guard case .loaded(.group(let group)) = state else { return nil }
            return group.groupId
        })
        groupMedia = groupMedia.filter { retainedGroups.contains($0.key) }
    }

    private func cancelObsoleteLoads() {
        let keep = (selectedPosition - Self.retainedPageRadius)...(selectedPosition + Self.retainedPageRadius)
        let obsoleteStarts = loadRequests.compactMap { start, request in
            request.range.allSatisfy({ !keep.contains($0) }) ? start : nil
        }
        for start in obsoleteStarts {
            loadRequests[start]?.task.cancel()
            loadRequests[start] = nil
        }
    }

    private func cancelAllLoads() {
        for request in loadRequests.values { request.task.cancel() }
        loadRequests = [:]
    }
}
