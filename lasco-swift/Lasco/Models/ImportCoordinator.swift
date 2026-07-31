import Foundation
import Observation
#if canImport(UIKit)
import Photos
#endif

@MainActor
@Observable
final class ImportCoordinator {
    private(set) var isImporting = false
    private(set) var isAutoImporting = false
    private(set) var progress: (done: Int, total: Int)?

    private let repository: any LibraryRepositoryProtocol
    private let session: LibrarySessionState
    private var cancelImportTask: (@Sendable () -> Void)?
    #if canImport(UIKit)
    private let photoImporter = PhotoLibraryImporter()
    #endif

    init(repository: any LibraryRepositoryProtocol, session: LibrarySessionState) {
        self.repository = repository
        self.session = session
    }

    func importMedia(urls: [URL], albumID: String) async -> String? {
        guard !isImporting else { return nil }
        isImporting = true
        progress = (0, urls.count)
        let task = Task { [weak self] in
            await self?.performImportMedia(urls: urls, albumID: albumID)
        }
        cancelImportTask = { task.cancel() }
        defer { cancelImportTask = nil }
        return await task.value
    }

    private func performImportMedia(urls: [URL], albumID: String) async -> String? {
        defer {
            isImporting = false
            progress = nil
        }

        var sources: [MediaImportSource] = []
        for url in urls {
            let accessed = url.startAccessingSecurityScopedResource()
            defer { if accessed { url.stopAccessingSecurityScopedResource() } }
            sources.append(MediaImportSource(path: url.path))
        }
        do {
            let ids = try await repository.importMediaBatch(sources, albumID: albumID)
            for (index, id) in ids.enumerated() {
                guard !Task.isCancelled else { return nil }
                if let data = ThumbnailGenerator.generate(for: urls[index]) {
                    try? await repository.setMediaThumbnail(mediaID: id, data: data)
                }
                progress = (index + 1, urls.count)
            }
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    func setAutoImportDeviceMedia(_ enabled: Bool) async throws {
        try await repository.setAutoImportDeviceMedia(enabled: enabled)
    }

    func close() {
        cancelImportTask?()
        cancelImportTask = nil
        isImporting = false
        isAutoImporting = false
        progress = nil
    }

    #if canImport(UIKit)
    private static let importChunkSize = 32

    func scanPhotoLibrary() async -> PhotoLibraryImporter.LibraryScan? {
        await photoImporter.scanLibrary()
    }

    func importFromPhotoLibraryWithAlbums(assets: [PHAsset]) async {
        guard let albumID = session.defaultUploadAlbumID else { return }
        isImporting = true
        progress = (0, assets.count)
        let task = Task { [weak self] in
            await self?.performPhotoLibraryImport(assets: assets, defaultAlbumID: albumID)
        }
        cancelImportTask = { task.cancel() }
        defer {
            cancelImportTask = nil
            isImporting = false
            progress = nil
        }
        await task.value
    }

    private func performPhotoLibraryImport(assets: [PHAsset], defaultAlbumID: String) async {
        let nodes = await photoImporter.scanAlbumTree()
        guard !Task.isCancelled else { return }

        let albumIDMap = await importAlbumStructure(nodes)
        guard !Task.isCancelled else { return }

        let assetMediaMap = await importAssets(assets, into: defaultAlbumID)
        guard !Task.isCancelled else { return }

        await linkAlbumMemberships(
            nodes: nodes,
            albumIDMap: albumIDMap,
            assetMediaMap: assetMediaMap,
            defaultAlbumID: defaultAlbumID
        )
        await repository.notifyChanged(.all)
    }

    private func importAssets(_ assets: [PHAsset], into albumID: String) async -> [String: [String]] {
        var assetMediaMap: [String: [String]] = [:]

        for (index, asset) in assets.enumerated() {
            guard !Task.isCancelled else { return assetMediaMap }
            do {
                let mediaIDs = try await photoImporter.importPHAsset(asset, into: albumID, repository: repository)
                if !mediaIDs.isEmpty {
                    assetMediaMap[asset.localIdentifier] = mediaIDs
                }
            } catch {
                AppLogger.log(.error, "photo import failed: \(error)")
            }
            progress = (index + 1, assets.count)

            if (index + 1).isMultiple(of: Self.importChunkSize) || index == assets.count - 1 {
                await repository.notifyChanged(.all)
            }
        }

        return assetMediaMap
    }

    /// Recreates the Photos folder/album hierarchy in parent-before-child order.
    private func importAlbumStructure(_ nodes: [PhotoLibraryImporter.AlbumNode]) async -> [String: String] {
        var albumIDMap: [String: String] = [:]

        for node in nodes {
            guard !Task.isCancelled else { return albumIDMap }
            let parentID = node.parentIosId.flatMap { albumIDMap[$0] }
            do {
                albumIDMap[node.iosId] = try await repository.createAlbum(name: node.name, parentID: parentID)
            } catch {
                AppLogger.log(.error, "importAlbumStructure: create '\(node.name)' failed: \(error)")
            }
        }

        return albumIDMap
    }

    /// Moves imported media to its first Photos album, then links it to every additional
    /// Photos album that contains the same asset. Assets without album membership remain in
    /// the default upload album.
    private func linkAlbumMemberships(
        nodes: [PhotoLibraryImporter.AlbumNode],
        albumIDMap: [String: String],
        assetMediaMap: [String: [String]],
        defaultAlbumID: String
    ) async {
        var assetAlbumIDs: [String: [String]] = [:]
        for node in nodes {
            guard let albumID = albumIDMap[node.iosId], !node.memberAssetIds.isEmpty else { continue }
            for assetID in node.memberAssetIds {
                assetAlbumIDs[assetID, default: []].append(albumID)
            }
        }

        for (assetID, albumIDs) in assetAlbumIDs {
            guard !Task.isCancelled else { return }
            guard let mediaIDs = assetMediaMap[assetID], let primaryAlbumID = albumIDs.first else { continue }

            for mediaID in mediaIDs {
                do {
                    try await repository.moveMedia(id: mediaID, from: defaultAlbumID, to: primaryAlbumID)
                } catch {
                    AppLogger.log(.error, "linkAlbumMemberships: move media \(mediaID) to album \(primaryAlbumID) failed: \(error)")
                }
            }

            for additionalAlbumID in albumIDs.dropFirst() {
                for mediaID in mediaIDs {
                    do {
                        try await repository.addMediaToAlbum(albumID: additionalAlbumID, mediaID: mediaID)
                    } catch {
                        AppLogger.log(.error, "linkAlbumMemberships: add media \(mediaID) to album \(additionalAlbumID) failed: \(error)")
                    }
                }
            }
        }
    }

    func autoImportFromPhotoLibrary() async {
        guard session.autoImportDeviceMedia,
              let albumID = session.defaultUploadAlbumID,
              !isAutoImporting,
              !isImporting else { return }
        isAutoImporting = true
        let task = Task { [weak self] in
            guard let self else { return 0 }
            return await self.photoImporter.importNewAssets(libraryId: self.session.libraryID, albumId: albumID, repository: self.repository)
        }
        cancelImportTask = { task.cancel() }
        defer {
            cancelImportTask = nil
            isAutoImporting = false
        }
        let imported = await task.value
        if imported > 0 {
            await repository.notifyChanged(.all)
        }
    }
    #endif
}
