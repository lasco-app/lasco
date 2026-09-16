import Foundation
import Observation
#if canImport(UIKit)
import UIKit
import Photos
#endif

#if canImport(UIKit)
@MainActor
@Observable
final class InitialPhotoImportController {
    typealias UploadProgress = @MainActor @Sendable (Double) -> Void
    typealias PushChunk = @MainActor (_ remoteID: FfiRemoteUuid, _ onUploadProgress: @escaping UploadProgress) async -> String?

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

    private(set) var scan: PhotoLibraryImporter.LibraryScan?
    private(set) var isScanning = false
    private(set) var isImporting = false
    private(set) var progress: ImportProgress?
    private(set) var result: (photos: Int, videos: Int)?
    private(set) var error: String?

    private let repository: any LibraryRepositoryProtocol
    private let photoImporter = PhotoLibraryImporter()
    private let pushChunk: PushChunk
    private var importTask: Task<Void, Never>?

    init(repository: any LibraryRepositoryProtocol, pushChunk: @escaping PushChunk) {
        self.repository = repository
        self.pushChunk = pushChunk
    }

    func scanPhotoLibrary() async {
        guard !isScanning else { return }
        isScanning = true
        scan = await photoImporter.scanLibrary(repository: repository)
        isScanning = false
    }

    func start(remoteIDs: [FfiRemoteUuid]) async {
        guard importTask == nil, let scan else { return }
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
        let task = Task { await self.performImport(scan: scan, remoteIDs: remoteIDs) }
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

    private func performImport(scan: PhotoLibraryImporter.LibraryScan, remoteIDs: [FfiRemoteUuid]) async {
        let nodes = scan.albums
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }

        let albumIDMap: [String: FfiAlbumUuid]
        do {
            albumIDMap = try await createAlbumStructure(nodes)
        } catch {
            await failImport("Could not create the Apple Photos album structure: \(error.localizedDescription)")
            return
        }
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }

        var assetMediaMap: [PhotoLibraryImporter.AlbumMembershipIdentity: [FfiMediaUuid]] = [:]
        var backedUp = 0
        for (_, chunkStart) in stride(from: 0, to: scan.assets.count, by: Self.chunkSize).enumerated() {
            guard !Task.isCancelled else {
                await repository.notifyPhotoImportChanged(initialImport: true)
                return
            }
            let chunkEnd = min(chunkStart + Self.chunkSize, scan.assets.count)
            let chunk = Array(scan.assets[chunkStart..<chunkEnd])
            let range = (chunkStart + 1)...chunkEnd
            var importedInChunk = 0

            for (offset, asset) in chunk.enumerated() {
                guard !Task.isCancelled else {
                    await repository.notifyPhotoImportChanged(initialImport: true)
                    return
                }
                do {
                    let imported = try await photoImporter.importPHAssetResources(asset.asset, cloudAssetId: asset.cloudAssetId, into: nil, repository: repository)
                    if !imported.linkableMediaIDs.isEmpty {
                        assetMediaMap[asset.membershipIdentity] = imported.linkableMediaIDs
                    }
                    if !imported.allMediaIDs.isEmpty {
                        importedInChunk += 1
                    }
                } catch {
                    if Task.isCancelled {
                        await repository.notifyPhotoImportChanged(initialImport: true)
                        return
                    }
                    AppLogger.log(.error, "initial photo import item \(offset + 1) failed: \(error)")
                    await failImport("Could not import item \(offset + 1): \(error.localizedDescription)")
                    return
                }
                progress = ImportProgress(
                    backedUp: backedUp,
                    total: scan.assets.count,
                    phase: .adding(range: range, completed: offset + 1)
                )
            }

            guard !Task.isCancelled else {
                await repository.notifyPhotoImportChanged(initialImport: true)
                return
            }
            progress = ImportProgress(
                backedUp: backedUp,
                total: scan.assets.count,
                phase: .uploading(range: range, progress: 0)
            )
            let backedUpBeforeChunk = backedUp
            if let error = await pushAll(remoteIDs, range: range, backedUp: backedUpBeforeChunk) {
                self.error = error
                await repository.notifyPhotoImportChanged(initialImport: true)
                return // The failed chunk remains local for recovery.
            }
            backedUp += importedInChunk
            progress = ImportProgress(
                backedUp: backedUp,
                total: scan.assets.count,
                phase: .finalizing(range: range)
            )
            // This is the user's normal Lasco library. Retain imported media locally after every
            // successful push; a later destination repair must not require another Photos download.
            await repository.notifyPhotoImportChanged(initialImport: true)
        }

        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }
        progress = ImportProgress(backedUp: backedUp, total: scan.assets.count, phase: .savingAlbums)
        do {
            try await linkAlbumMemberships(nodes: nodes, albumIDMap: albumIDMap, assetMediaMap: assetMediaMap)
        } catch {
            await failImport("Could not save Apple Photos album membership: \(error.localizedDescription)")
            return
        }
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }
        if let error = await pushAll(remoteIDs, range: 1...max(scan.assets.count, 1), backedUp: backedUp) {
            self.error = error
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }
        await repository.notifyPhotoImportChanged(initialImport: true)
        result = (photos: scan.photoCount, videos: scan.videoCount)
    }

    /// Completion is only declared after every configured destination accepts the final state.
    private func pushAll(_ remoteIDs: [FfiRemoteUuid], range: ClosedRange<Int>, backedUp: Int) async -> String? {
        for remoteID in remoteIDs {
            guard !Task.isCancelled else { return "Import cancelled." }
            if let error = await pushChunk(remoteID, { [weak self] fraction in
                guard let self else { return }
                let phase: ImportPhase = fraction < 1
                    ? .uploading(range: range, progress: min(max(fraction, 0), 1))
                    : .finalizing(range: range)
                self.progress = ImportProgress(backedUp: backedUp, total: self.scan?.assets.count ?? 0, phase: phase)
            }) {
                return error
            }
        }
        return nil
    }

    private func createAlbumStructure(_ nodes: [PhotoLibraryImporter.AlbumNode]) async throws -> [String: FfiAlbumUuid] {
        var albumIDMap: [String: FfiAlbumUuid] = [:]
        let identities = nodes.map {
            FfiApplePhotosCollectionIdentity(cloudCollectionId: $0.cloudCollectionId, kind: $0.kind)
        }
        let existing = (try? await repository.applePhotosCollectionLinks(identities)) ?? Array(repeating: nil, count: nodes.count)
        for (node, linkedAlbumID) in zip(nodes, existing) {
            guard !Task.isCancelled else { return albumIDMap }
            if let linkedAlbumID {
                albumIDMap[node.cloudCollectionId] = linkedAlbumID
                continue
            }
            let albumID = try await repository.createAlbumWithoutNotification(
                name: node.name,
                parentID: node.parentCloudCollectionId.flatMap { albumIDMap[$0] }
            )
            try await repository.recordApplePhotosCollectionLink(
                FfiApplePhotosCollectionLink(albumId: albumID, cloudCollectionId: node.cloudCollectionId, kind: node.kind)
            )
            albumIDMap[node.cloudCollectionId] = albumID
        }
        return albumIDMap
    }

    private func linkAlbumMemberships(
        nodes: [PhotoLibraryImporter.AlbumNode],
        albumIDMap: [String: FfiAlbumUuid],
        assetMediaMap: [PhotoLibraryImporter.AlbumMembershipIdentity: [FfiMediaUuid]]
    ) async throws {
        var assetAlbumIDs: [PhotoLibraryImporter.AlbumMembershipIdentity: [FfiAlbumUuid]] = [:]
        for node in nodes {
            guard let albumID = albumIDMap[node.cloudCollectionId] else { continue }
            for assetID in node.memberAssetIdentities {
                assetAlbumIDs[assetID, default: []].append(albumID)
            }
        }
        for (assetID, albumIDs) in assetAlbumIDs {
            guard !Task.isCancelled else { return }
            guard let mediaIDs = assetMediaMap[assetID] else { continue }
            for albumID in albumIDs {
                for mediaID in mediaIDs {
                    try await repository.addMediaToAlbumWithoutNotification(albumID: albumID, mediaID: mediaID)
                }
            }
        }
    }

    private func failImport(_ message: String) async {
        error = message
        await repository.notifyPhotoImportChanged(initialImport: true)
    }
}
#endif
