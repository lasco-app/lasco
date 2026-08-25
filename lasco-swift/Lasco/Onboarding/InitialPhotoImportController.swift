import Foundation
import Observation
#if canImport(UIKit)
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
        scan = await photoImporter.scanLibrary()
        isScanning = false
    }

    func start(remoteID: FfiRemoteUuid?) async {
        guard importTask == nil, let scan else { return }
        guard let remoteID else {
            error = "Add a remote before importing your photo library."
            return
        }
        error = nil
        result = nil
        isImporting = true
        progress = ImportProgress(backedUp: 0, total: scan.assets.count, phase: .preparingLibrary)
        let task = Task { await self.performImport(scan: scan, remoteID: remoteID) }
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

    private func performImport(scan: PhotoLibraryImporter.LibraryScan, remoteID: FfiRemoteUuid) async {
        let nodes = await photoImporter.scanAlbumTree()
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }

        let albumIDMap = await createAlbumStructure(nodes)
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }

        var assetMediaMap: [String: [FfiMediaUuid]] = [:]
        var backedUp = 0
        for (_, chunkStart) in stride(from: 0, to: scan.assets.count, by: Self.chunkSize).enumerated() {
            guard !Task.isCancelled else {
                await repository.notifyPhotoImportChanged(initialImport: true)
                return
            }
            let chunkEnd = min(chunkStart + Self.chunkSize, scan.assets.count)
            let chunk = Array(scan.assets[chunkStart..<chunkEnd])
            let range = (chunkStart + 1)...chunkEnd
            var chunkMediaIDs: [FfiMediaUuid] = []
            var importedInChunk = 0

            for (offset, asset) in chunk.enumerated() {
                guard !Task.isCancelled else {
                    await repository.notifyPhotoImportChanged(initialImport: true)
                    return
                }
                do {
                    let imported = try await photoImporter.importPHAssetResources(asset, into: nil, repository: repository)
                    if !imported.linkableMediaIDs.isEmpty {
                        assetMediaMap[asset.localIdentifier] = imported.linkableMediaIDs
                    }
                    chunkMediaIDs.append(contentsOf: imported.allMediaIDs)
                    if !imported.allMediaIDs.isEmpty {
                        importedInChunk += 1
                    }
                } catch {
                    AppLogger.log(.error, "initial photo import failed for \(asset.localIdentifier): \(error)")
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
            if let error = await pushChunk(remoteID, { [weak self] fraction in
                guard let self else { return }
                let phase: ImportPhase
                if fraction < 1 {
                    phase = .uploading(range: range, progress: min(max(fraction, 0), 1))
                } else {
                    // The upload callback covers originals only. Thumbnails, operations,
                    // and compaction still need to finish before the push returns.
                    phase = .finalizing(range: range)
                }
                self.progress = ImportProgress(
                    backedUp: backedUpBeforeChunk,
                    total: scan.assets.count,
                    phase: phase
                )
            }) {
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
            guard !Task.isCancelled else {
                await repository.notifyPhotoImportChanged(initialImport: true)
                return
            }
            do {
                try await repository.evictLocalData(mediaIDs: chunkMediaIDs)
                await repository.notifyPhotoImportChanged(initialImport: true)
            } catch {
                self.error = error.localizedDescription
                return
            }
        }

        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }
        progress = ImportProgress(backedUp: backedUp, total: scan.assets.count, phase: .savingAlbums)
        await linkAlbumMemberships(nodes: nodes, albumIDMap: albumIDMap, assetMediaMap: assetMediaMap)
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }
        if let error = await pushChunk(remoteID, { _ in }) {
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

    private func createAlbumStructure(_ nodes: [PhotoLibraryImporter.AlbumNode]) async -> [String: FfiAlbumUuid] {
        var albumIDMap: [String: FfiAlbumUuid] = [:]
        for node in nodes {
            guard !Task.isCancelled else { return albumIDMap }
            do {
                albumIDMap[node.iosId] = try await repository.createAlbumWithoutNotification(
                    name: node.name,
                    parentID: node.parentIosId.flatMap { albumIDMap[$0] }
                )
            } catch {
                AppLogger.log(.error, "initial photo import: create album '\(node.name)' failed: \(error)")
            }
        }
        return albumIDMap
    }

    private func linkAlbumMemberships(
        nodes: [PhotoLibraryImporter.AlbumNode],
        albumIDMap: [String: FfiAlbumUuid],
        assetMediaMap: [String: [FfiMediaUuid]]
    ) async {
        var assetAlbumIDs: [String: [FfiAlbumUuid]] = [:]
        for node in nodes {
            guard let albumID = albumIDMap[node.iosId] else { continue }
            for assetID in node.memberAssetIds {
                assetAlbumIDs[assetID, default: []].append(albumID)
            }
        }
        for (assetID, albumIDs) in assetAlbumIDs {
            guard !Task.isCancelled, let primaryAlbumID = albumIDs.first, let mediaIDs = assetMediaMap[assetID] else { return }
            for mediaID in mediaIDs {
                try? await repository.addMediaToAlbumWithoutNotification(albumID: primaryAlbumID, mediaID: mediaID)
            }
            for additionalAlbumID in albumIDs.dropFirst() {
                for mediaID in mediaIDs {
                    try? await repository.addMediaToAlbumWithoutNotification(albumID: additionalAlbumID, mediaID: mediaID)
                }
            }
        }
    }
}
#endif
