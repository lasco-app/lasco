import Foundation
import Observation
#if canImport(UIKit)
import Photos
#endif

#if canImport(UIKit)
@MainActor
@Observable
final class InitialPhotoImportController {
    typealias PushChunk = @MainActor (_ remoteID: String) async -> String?

    private static let chunkSize = 32

    private(set) var scan: PhotoLibraryImporter.LibraryScan?
    private(set) var isScanning = false
    private(set) var isImporting = false
    private(set) var progress: (done: Int, total: Int)?
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

    func start(remoteID: String?) async {
        guard importTask == nil, let scan else { return }
        guard let remoteID else {
            error = "Add a remote before importing your photo library."
            return
        }
        error = nil
        result = nil
        isImporting = true
        progress = (0, scan.assets.count)
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

    private func performImport(scan: PhotoLibraryImporter.LibraryScan, remoteID: String) async {
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

        var assetMediaMap: [String: [String]] = [:]
        for chunkStart in stride(from: 0, to: scan.assets.count, by: Self.chunkSize) {
            guard !Task.isCancelled else {
                await repository.notifyPhotoImportChanged(initialImport: true)
                return
            }
            let chunkEnd = min(chunkStart + Self.chunkSize, scan.assets.count)
            let chunk = Array(scan.assets[chunkStart..<chunkEnd])
            var chunkMediaIDs: [String] = []

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
                } catch {
                    AppLogger.log(.error, "initial photo import failed for \(asset.localIdentifier): \(error)")
                }
                progress = (chunkStart + offset + 1, scan.assets.count)
            }

            guard !Task.isCancelled else {
                await repository.notifyPhotoImportChanged(initialImport: true)
                return
            }
            if let error = await pushChunk(remoteID) {
                self.error = error
                await repository.notifyPhotoImportChanged(initialImport: true)
                return // The failed chunk remains local for recovery.
            }
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
        await linkAlbumMemberships(nodes: nodes, albumIDMap: albumIDMap, assetMediaMap: assetMediaMap)
        guard !Task.isCancelled else {
            await repository.notifyPhotoImportChanged(initialImport: true)
            return
        }
        if let error = await pushChunk(remoteID) {
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

    private func createAlbumStructure(_ nodes: [PhotoLibraryImporter.AlbumNode]) async -> [String: String] {
        var albumIDMap: [String: String] = [:]
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
        albumIDMap: [String: String],
        assetMediaMap: [String: [String]]
    ) async {
        var assetAlbumIDs: [String: [String]] = [:]
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
