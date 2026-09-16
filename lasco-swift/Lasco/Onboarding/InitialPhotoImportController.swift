import Foundation
import Observation
import LascoPhotoImportKit
#if canImport(UIKit)
import UIKit
#endif

#if canImport(UIKit)
@MainActor
@Observable
final class InitialPhotoImportController {
    typealias UploadProgress = @MainActor @Sendable (Double) -> Void
    typealias PushChunk = @MainActor (_ remoteID: FfiRemoteUuid, _ onUploadProgress: @escaping UploadProgress) async -> String?

    struct LibraryScan {
        let discovery: PhotosDiscovery
        let photoCount: Int
        let videoCount: Int
        let livePhotoVideoCount: Int
        let editMetadataCount: Int
        let estimatedBytes: Int64

        var assets: [PhotosAsset] { discovery.assets }
        var ignoredAssets: [PhotosIgnoredAsset] { discovery.ignoredAssets }

        init(discovery: PhotosDiscovery) {
            self.discovery = discovery
            photoCount = discovery.assets.filter { asset in
                guard let primary = asset.resources.first(where: { $0.role == .primary }) else { return false }
                return primary.type == .photo || primary.type == .fullSizePhoto
            }.count
            videoCount = discovery.assets.count - photoCount
            livePhotoVideoCount = discovery.resources.filter { $0.role == .pairedVideo }.count
            editMetadataCount = discovery.resources.filter { $0.role == .adjustmentData }.count
            estimatedBytes = discovery.resources.reduce(0) { $0 + $1.byteCount }
        }
    }

    struct ImportProgress: Equatable {
        let backedUp: Int
        let total: Int
        let phase: ImportPhase
    }

    enum ImportPhase: Equatable {
        case preparingLibrary
        case adding(range: ClosedRange<Int>, completed: Int)
        case uploading(range: ClosedRange<Int>, progress: Double)
        case finalizing(range: ClosedRange<Int>)
        case savingAlbums
    }

    private static let chunkSize = 32

    private(set) var scan: LibraryScan?
    private(set) var isScanning = false
    private(set) var isImporting = false
    private(set) var progress: ImportProgress?
    private(set) var result: (photos: Int, videos: Int)?
    private(set) var error: String?

    private let repository: any LibraryRepositoryProtocol
    private let pushChunk: PushChunk
    private var photoSession: ApplePhotosImportSession?
    private var importTask: Task<Void, Never>?

    init(repository: any LibraryRepositoryProtocol, pushChunk: @escaping PushChunk) {
        self.repository = repository
        self.pushChunk = pushChunk
    }

    /// Matches the desktop importer's remote-log preflight before shared PhotoKit discovery.
    func scanPhotoLibrary(remoteIDs: [FfiRemoteUuid]) async {
        guard !isScanning else { return }
        isScanning = true
        error = nil
        defer { isScanning = false }

        do {
            try await synchronizeRemotesBeforeDiscovery(remoteIDs)
            let session = ApplePhotosImportSession(
                photoLibrary: PhotoLibraryDiscovery(),
                remoteIDs: remoteIDs.map(\.value),
                chunkSize: Self.chunkSize
            )
            let discovery = try await session.discover()
            photoSession = session
            scan = LibraryScan(discovery: discovery)
        } catch {
            photoSession = nil
            scan = nil
            self.error = error.localizedDescription
        }
    }

    func start(remoteIDs: [FfiRemoteUuid]) async {
        guard importTask == nil, let scan, let photoSession else { return }
        guard !remoteIDs.isEmpty else {
            error = "Add a remote before importing your photo library."
            return
        }
        error = nil
        result = nil
        isImporting = true
        progress = ImportProgress(backedUp: 0, total: scan.assets.count, phase: .preparingLibrary)
        let idleTimerWasDisabled = UIApplication.shared.isIdleTimerDisabled
        UIApplication.shared.isIdleTimerDisabled = true
        defer { UIApplication.shared.isIdleTimerDisabled = idleTimerWasDisabled }
        let task = Task { await self.performImport(session: photoSession, scan: scan, remoteIDs: remoteIDs) }
        importTask = task
        await task.value
        importTask = nil
        isImporting = false
        progress = nil
    }

    func cancelAndWait() async {
        importTask?.cancel()
        if let importTask { await importTask.value }
        importTask = nil
        isImporting = false
        progress = nil
    }

    private func synchronizeRemotesBeforeDiscovery(_ remoteIDs: [FfiRemoteUuid]) async throws {
        for remoteID in remoteIDs {
            try await repository.fetch(remoteID: remoteID)
        }
        for remoteID in remoteIDs where await repository.hasUnpushedChanges(remoteID: remoteID) {
            throw InitialImportError.remotesOutOfSync
        }
    }

    private func performImport(session: ApplePhotosImportSession, scan: LibraryScan, remoteIDs: [FfiRemoteUuid]) async {
        do {
            var step = try await session.advance([])
            var backedUp = 0

            while !Task.isCancelled {
                switch step {
                case .lookupRevisions(let manifests):
                    step = try await session.advance([.revisionMatches(try await revisionMatches(for: manifests))])

                case .lookupCollectionLinks(let collectionIDs):
                    step = try await session.advance([.collectionLinks(try await collectionLinks(for: collectionIDs, discovery: scan.discovery))])

                case .lookupAlbumMemberships(let mediaIDs):
                    step = try await session.advance([.albumMemberships(try await albumMemberships(for: mediaIDs))])

                case .confirmRemoteInventory(let ids, let mediaIDs):
                    step = try await session.advance([.remoteInventories(try await remoteInventories(remoteIDs: ids, mediaIDs: mediaIDs))])

                case .createCollection(let collection):
                    progress = ImportProgress(backedUp: backedUp, total: scan.assets.count, phase: .preparingLibrary)
                    let albumID = try await repository.createAlbumWithoutNotification(
                        name: collection.name,
                        parentID: try await parentAlbumID(for: collection, discovery: scan.discovery)
                    )
                    try await repository.recordApplePhotosCollectionLink(.init(
                        albumId: albumID,
                        cloudCollectionId: collection.cloudCollectionID,
                        kind: ffiCollectionKind(collection.kind)
                    ))
                    step = try await session.advance([.collectionCreated(cloudCollectionID: collection.cloudCollectionID, albumID: albumID.value)])

                case .importResource(let action):
                    let range = range(for: action.resource.assetTicket, discovery: scan.discovery)
                    let completed = completedCount(for: action.resource.assetTicket, range: range, discovery: scan.discovery)
                    progress = ImportProgress(backedUp: backedUp, total: scan.assets.count, phase: .adding(range: range, completed: completed))
                    let mediaID = try await importResource(action, session: session)
                    step = try await session.advance([.resourceImported(resourceTicket: action.resource.ticket, mediaID: mediaID.value)])

                case .addMembership(let action):
                    progress = ImportProgress(backedUp: backedUp, total: scan.assets.count, phase: .savingAlbums)
                    try await repository.addMediaToAlbumWithoutNotification(
                        albumID: .init(value: action.albumID),
                        mediaID: .init(value: action.mediaID)
                    )
                    step = try await session.advance([.membershipAdded(action)])

                case .chunkReady(let chunk):
                    let range = range(for: chunk, discovery: scan.discovery)
                    progress = ImportProgress(backedUp: backedUp, total: scan.assets.count, phase: .uploading(range: range, progress: 0))
                    if let pushError = await pushAll(remoteIDs, range: range, backedUp: backedUp) {
                        throw InitialImportError.pushFailed(pushError)
                    }
                    backedUp += chunk.assetTickets.count
                    progress = ImportProgress(backedUp: backedUp, total: scan.assets.count, phase: .finalizing(range: range))
                    await repository.notifyPhotoImportChanged(initialImport: true)
                    step = try await session.advance([.chunkFinished(.succeeded)])

                case .paused:
                    return

                case .finished:
                    await repository.notifyPhotoImportChanged(initialImport: true)
                    result = (photos: scan.photoCount, videos: scan.videoCount)
                    return
                }
            }
        } catch {
            if !Task.isCancelled { await failImport(error.localizedDescription) }
        }
        await repository.notifyPhotoImportChanged(initialImport: true)
    }

    private func revisionMatches(for manifests: [AssetManifest]) async throws -> [RevisionMatch] {
        var matches: [RevisionMatch] = []
        matches.reserveCapacity(manifests.count)
        for manifest in manifests {
            try Task.checkCancellation()
            let revision = FfiApplePhotosAssetRevision(
                cloudAssetId: manifest.cloudAssetID,
                modificationDate: manifest.modificationDate,
                resources: manifest.resources.map { .init(resourceType: ffiResourceType($0.type), filename: $0.filename) }
            )
            let mediaIDs = try await repository.applePhotosAssetRevisionMediaIDs(revision)?.map(\.value)
            matches.append(.init(manifest: manifest, mediaIDs: mediaIDs))
        }
        return matches
    }

    private func collectionLinks(for collectionIDs: [String], discovery: PhotosDiscovery) async throws -> [CollectionLink] {
        let collectionsByID = Dictionary(uniqueKeysWithValues: discovery.collections.map { ($0.cloudCollectionID, $0) })
        let collections = try collectionIDs.map { id -> PhotosCollection in
            guard let collection = collectionsByID[id] else { throw InitialImportError.unknownCollection(id) }
            return collection
        }
        let identities = collections.map { FfiApplePhotosCollectionIdentity(cloudCollectionId: $0.cloudCollectionID, kind: ffiCollectionKind($0.kind)) }
        let albums = try await repository.applePhotosCollectionLinks(identities)
        return zip(collections, albums).map { CollectionLink(cloudCollectionID: $0.0.cloudCollectionID, albumID: $0.1?.value) }
    }

    private func albumMemberships(for mediaIDs: [String]) async throws -> [AlbumMembership] {
        var memberships: [AlbumMembership] = []
        memberships.reserveCapacity(mediaIDs.count)
        for mediaID in mediaIDs {
            let albums = try await repository.mediaAlbumIDs(mediaID: .init(value: mediaID))
            memberships.append(.init(mediaID: mediaID, albumIDs: Set(albums.map(\.value))))
        }
        return memberships
    }

    private func remoteInventories(remoteIDs: [String], mediaIDs: [String]) async throws -> [RemoteInventory] {
        let ffiMediaIDs = mediaIDs.map { FfiMediaUuid(value: $0) }
        var inventories: [RemoteInventory] = []
        inventories.reserveCapacity(remoteIDs.count)
        for remoteID in remoteIDs {
            let id = FfiRemoteUuid(value: remoteID)
            try await repository.confirmRemoteMedia(remoteID: id)
            let confirmed = try await repository.confirmedRemoteMediaIDs(remoteID: id, mediaIDs: ffiMediaIDs)
            inventories.append(.init(remoteID: remoteID, confirmedMediaIDs: Set(confirmed.map(\.value))))
        }
        return inventories
    }

    private func parentAlbumID(for collection: PhotosCollection, discovery: PhotosDiscovery) async throws -> FfiAlbumUuid? {
        guard let parentID = collection.parentCloudCollectionID else { return nil }
        guard let parent = discovery.collections.first(where: { $0.cloudCollectionID == parentID }) else {
            throw InitialImportError.unknownCollection(parentID)
        }
        let linked = try await repository.applePhotosCollectionLinks([
            .init(cloudCollectionId: parent.cloudCollectionID, kind: ffiCollectionKind(parent.kind))
        ])
        return linked.first ?? nil
    }

    private func importResource(_ action: PhotosImportAction, session: ApplePhotosImportSession) async throws -> FfiMediaUuid {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("lasco-photos-import", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let staged = try await session.stage(resourceTicket: action.resource.ticket, destinationDirectory: directory)
        defer { try? FileManager.default.removeItem(at: staged.path) }

        let mediaID = try await repository.importMediaWithoutNotification(
            source: MediaImportSource(
                path: staged.path.path,
                originalFilename: action.resource.filename,
                appleAaeMediaID: action.adjustmentDataMediaID.map { .init(value: $0) },
                appleLivePhotoMediaID: action.pairedVideoMediaID.map { .init(value: $0) }
            ),
            albumID: nil
        )

        if action.resource.role == .primary, let thumbnail = ThumbnailGenerator.generate(for: staged.path) {
            try? await repository.setMediaThumbnail(mediaID: mediaID, data: thumbnail)
        }
        if let cloudAssetID = action.resource.cloudAssetID {
            try await repository.recordApplePhotosResourceOrigin(.init(
                mediaId: mediaID,
                cloudAssetId: cloudAssetID,
                modificationDate: action.resource.sourceMetadata.modifiedAt,
                resourceType: ffiResourceType(action.resource.type),
                filename: action.resource.filename
            ))
        }
        return mediaID
    }

    private func pushAll(_ remoteIDs: [FfiRemoteUuid], range: ClosedRange<Int>, backedUp: Int) async -> String? {
        for remoteID in remoteIDs {
            guard !Task.isCancelled else { return "Import cancelled." }
            if let error = await pushChunk(remoteID, { [weak self] fraction in
                guard let self else { return }
                let phase: ImportPhase = fraction < 1
                    ? .uploading(range: range, progress: min(max(fraction, 0), 1))
                    : .finalizing(range: range)
                self.progress = ImportProgress(backedUp: backedUp, total: self.scan?.assets.count ?? 0, phase: phase)
            }) { return error }
        }
        return nil
    }

    private func range(for assetTicket: String, discovery: PhotosDiscovery) -> ClosedRange<Int> {
        guard let index = discovery.assets.firstIndex(where: { $0.ticket == assetTicket }) else { return 1...1 }
        let start = (index / Self.chunkSize) * Self.chunkSize
        return (start + 1)...min(start + Self.chunkSize, discovery.assets.count)
    }

    private func range(for chunk: PhotosChunk, discovery: PhotosDiscovery) -> ClosedRange<Int> {
        guard let first = chunk.assetTickets.first else { return 1...max(discovery.assets.count, 1) }
        return range(for: first, discovery: discovery)
    }

    private func completedCount(for assetTicket: String, range: ClosedRange<Int>, discovery: PhotosDiscovery) -> Int {
        guard let index = discovery.assets.firstIndex(where: { $0.ticket == assetTicket }) else { return 0 }
        return min(index - range.lowerBound + 2, range.count)
    }

    private func ffiResourceType(_ type: PhotosResourceType) -> FfiApplePhotosResourceType {
        switch type {
        case .photo: .photo
        case .fullSizePhoto: .fullSizePhoto
        case .video: .video
        case .fullSizeVideo: .fullSizeVideo
        case .adjustmentData: .adjustmentData
        case .pairedVideo: .pairedVideo
        case .fullSizePairedVideo: .fullSizePairedVideo
        }
    }

    private func ffiCollectionKind(_ kind: PhotosCollectionKind) -> FfiApplePhotosCollectionKind {
        kind == .folder ? .folder : .album
    }

    private func failImport(_ message: String) async {
        error = message
        await repository.notifyPhotoImportChanged(initialImport: true)
    }
}

private enum InitialImportError: LocalizedError {
    case remotesOutOfSync
    case unknownCollection(String)
    case pushFailed(String)

    var errorDescription: String? {
        switch self {
        case .remotesOutOfSync:
            "The configured remotes do not contain the same library operation-log state. Sync the library with every remote, then try again."
        case .unknownCollection(let id):
            "The Photos collection \(id) is no longer available. Rescan the photo library and try again."
        case .pushFailed(let message): message
        }
    }
}
#endif
