#if canImport(UIKit)
import Foundation
import UIKit
import Photos

extension LibraryModel {

    func scanPhotoLibrary() async -> PhotoLibraryImporter.LibraryScan? {
        await photoImporter.scanLibrary()
    }

    private static let importChunkSize = 32

    @discardableResult
    func bulkImportFromPhotoLibrary(assets: [PHAsset], remoteId: String? = nil) async -> [String: [String]] {
        guard !isImporting else { return [:] }
        guard let lib else { return [:] }
        guard let resolvedRemoteId = remoteId ?? defaultFetchRemoteId else {
            AppLogger.log(.error, "bulkImport: no remote configured")
            return [:]
        }
        guard let albumId = defaultUploadAlbumId else {
            AppLogger.log(.error, "bulkImport: no default upload album")
            return [:]
        }

        isImporting = true
        bulkImportProgress = (done: 0, total: assets.count)

        var assetMediaMap: [String: [String]] = [:]

        let chunks = stride(from: 0, to: assets.count, by: Self.importChunkSize).map {
            Array(assets[$0..<min($0 + Self.importChunkSize, assets.count)])
        }

        for chunk in chunks {
            var chunkMediaIds: [String] = []
            for asset in chunk {
                do {
                    let ids = try await photoImporter.importPHAsset(asset, into: albumId, lib: lib)
                    chunkMediaIds.append(contentsOf: ids)
                    if !ids.isEmpty { assetMediaMap[asset.localIdentifier] = ids }
                } catch {
                    AppLogger.log(.error, "bulkImport: asset failed: \(error)")
                }
                bulkImportProgress = (done: (bulkImportProgress?.done ?? 0) + 1, total: assets.count)
            }

            _ = await pushRemote(remoteId: resolvedRemoteId)

            do {
                try await runOnBackground { try lib.evictLocalData(mediaIds: chunkMediaIds) }
            } catch {
                AppLogger.log(.error, "bulkImport: eviction failed: \(error)")
            }
        }

        isImporting = false
        bulkImportProgress = nil
        reload()
        AppLogger.log(.info, "bulkImport complete — \(assets.count) asset(s)")
        return assetMediaMap
    }

    private func runOnBackground<T>(_ work: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                do { continuation.resume(returning: try work()) }
                catch { continuation.resume(throwing: error) }
            }
        }
    }

    func importSinglePHAsset(_ asset: PHAsset) async throws {
        guard let lib, let albumId = defaultUploadAlbumId else { return }
        try await photoImporter.importPHAsset(asset, into: albumId, lib: lib)
    }

    // MARK: - Initial import with albums

    private func importAlbumStructure(_ nodes: [PhotoLibraryImporter.AlbumNode], lib: FfiLibrary) async -> [String: String] {
        var albumIdMap: [String: String] = [:]
        for node in nodes {
            let parentAlbumId = node.parentIosId.flatMap { albumIdMap[$0] }
            do {
                let albumId = try await runOnBackground { try lib.createAlbum(name: node.name, parentAlbumId: parentAlbumId) }
                albumIdMap[node.iosId] = albumId
            } catch {
                AppLogger.log(.error, "importAlbumStructure: create '\(node.name)' failed: \(error)")
            }
        }
        return albumIdMap
    }

    /// Moves media out of the default upload album and into the iOS album(s) it actually
    /// belonged to. Assets with no iOS album stay in the default album untouched.
    private func linkAlbumMemberships(nodes: [PhotoLibraryImporter.AlbumNode], albumIdMap: [String: String], assetMediaMap: [String: [String]], defaultAlbumId: String, lib: FfiLibrary) async {
        var assetAlbumIds: [String: [String]] = [:]
        for node in nodes {
            guard let albumId = albumIdMap[node.iosId], !node.memberAssetIds.isEmpty else { continue }
            for assetId in node.memberAssetIds {
                assetAlbumIds[assetId, default: []].append(albumId)
            }
        }

        let entries = Array(assetAlbumIds)
        let chunks = stride(from: 0, to: entries.count, by: Self.importChunkSize).map {
            Array(entries[$0..<min($0 + Self.importChunkSize, entries.count)])
        }

        for chunk in chunks {
            for (assetId, albumIds) in chunk {
                guard let mediaIds = assetMediaMap[assetId], let homeAlbumId = albumIds.first else { continue }
                for mediaId in mediaIds {
                    do {
                        try await runOnBackground { try lib.moveMediaToAlbum(mediaId: mediaId, fromAlbumId: defaultAlbumId, toAlbumId: homeAlbumId) }
                    } catch {
                        AppLogger.log(.error, "linkAlbumMemberships: move media \(mediaId) to album \(homeAlbumId) failed: \(error)")
                    }
                }
                for extraAlbumId in albumIds.dropFirst() {
                    for mediaId in mediaIds {
                        do {
                            try await runOnBackground { try lib.addMediaToAlbum(albumId: extraAlbumId, mediaId: mediaId) }
                        } catch {
                            AppLogger.log(.error, "linkAlbumMemberships: add media \(mediaId) to album \(extraAlbumId) failed: \(error)")
                        }
                    }
                }
            }
        }
    }

    func importFromPhotoLibraryWithAlbums(assets: [PHAsset], remoteId: String? = nil) async {
        guard let lib, let defaultAlbumId = defaultUploadAlbumId else { return }
        let nodes = await photoImporter.scanAlbumTree()
        let albumIdMap = await importAlbumStructure(nodes, lib: lib)

        if let resolvedRemoteId = remoteId ?? defaultFetchRemoteId {
            _ = await pushRemote(remoteId: resolvedRemoteId)
        }

        let assetMediaMap = await bulkImportFromPhotoLibrary(assets: assets, remoteId: remoteId)
        await linkAlbumMemberships(nodes: nodes, albumIdMap: albumIdMap, assetMediaMap: assetMediaMap, defaultAlbumId: defaultAlbumId, lib: lib)
        reload()
    }

    func setAutoImportDeviceMedia(_ enabled: Bool) {
        do { try lib?.setAutoImportDeviceMedia(enabled: enabled) }
        catch { AppLogger.log(.error, "setAutoImportDeviceMedia failed: \(error)") }
        autoImportDeviceMedia = enabled
    }

    func autoImportFromPhotoLibrary() async {
        guard autoImportDeviceMedia == true else { return }
        guard let lib, let albumId = defaultUploadAlbumId else { return }
        let libId = lib.libraryId()
        guard !isAutoImporting else { return }
        pushDebounceTask?.cancel()
        isAutoImporting = true
        let count = await photoImporter.importNewAssets(libraryId: libId, albumId: albumId, lib: lib)
        isAutoImporting = false
        if count > 0 {
            AppLogger.log(.info, "auto-import complete — \(count) new asset(s)")
            reload()
            schedulePush()
        }
    }
}

#endif
