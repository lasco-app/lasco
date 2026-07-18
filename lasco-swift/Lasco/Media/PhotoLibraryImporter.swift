#if canImport(UIKit)
import Photos
import UIKit

actor PhotoLibraryImporter {
    private static func lastImportDateKey(libraryId: String) -> String {
        "lasco.lastPhotoImport.\(libraryId)"
    }

    private func lastImportDate(libraryId: String) -> Date? {
        UserDefaults.standard.object(forKey: Self.lastImportDateKey(libraryId: libraryId)) as? Date
    }

    private func setLastImportDate(_ date: Date, libraryId: String) {
        UserDefaults.standard.set(date, forKey: Self.lastImportDateKey(libraryId: libraryId))
    }

    struct LibraryScan {
        let photoCount: Int
        let videoCount: Int
        let estimatedBytes: Int64
        let assets: [PHAsset]
    }

    struct AlbumNode {
        let iosId: String
        let name: String
        let parentIosId: String?
        let memberAssetIds: [String]
    }

    /// Walks the iOS album/folder tree and returns a flat, parent-before-child list.
    /// Folders become nodes with no direct members, real user albums carry their asset identifiers.
    func scanAlbumTree() async -> [AlbumNode] {
        var nodes: [AlbumNode] = []
        let topLevel = PHCollectionList.fetchTopLevelUserCollections(with: nil)
        walkCollections(topLevel, parentIosId: nil, into: &nodes)
        return nodes
    }

    private func walkCollections(_ result: PHFetchResult<PHCollection>, parentIosId: String?, into nodes: inout [AlbumNode]) {
        for i in 0..<result.count {
            let collection = result.object(at: i)
            if let folder = collection as? PHCollectionList {
                nodes.append(AlbumNode(iosId: folder.localIdentifier, name: folder.localizedTitle ?? "", parentIosId: parentIosId, memberAssetIds: []))
                let children = PHCollection.fetchCollections(in: folder, options: nil)
                walkCollections(children, parentIosId: folder.localIdentifier, into: &nodes)
            } else if let album = collection as? PHAssetCollection,
                      album.assetCollectionType == .album, album.assetCollectionSubtype == .albumRegular {
                let assets = PHAsset.fetchAssets(in: album, options: nil)
                var memberAssetIds: [String] = []
                memberAssetIds.reserveCapacity(assets.count)
                for j in 0..<assets.count {
                    memberAssetIds.append(assets.object(at: j).localIdentifier)
                }
                nodes.append(AlbumNode(iosId: album.localIdentifier, name: album.localizedTitle ?? "", parentIosId: parentIosId, memberAssetIds: memberAssetIds))
            }
        }
    }

    /// Scans the photo library and returns counts, estimated size, and the asset list without importing anything.
    func scanLibrary() async -> LibraryScan? {
        let status = await PHPhotoLibrary.requestAuthorization(for: .readWrite)
        guard status == .authorized || status == .limited else { return nil }

        let allAssets = PHAsset.fetchAssets(with: nil)
        var photoCount = 0
        var videoCount = 0
        var totalBytes: Int64 = 0
        var assets: [PHAsset] = []
        assets.reserveCapacity(allAssets.count)

        for i in 0..<allAssets.count {
            let asset = allAssets.object(at: i)
            assets.append(asset)
            switch asset.mediaType {
            case .image: photoCount += 1
            case .video: videoCount += 1
            default: break
            }
            let resources = PHAssetResource.assetResources(for: asset)
            for resource in resources {
                if let size = resource.value(forKey: "fileSize") as? Int64 {
                    totalBytes += size
                    break
                }
            }
        }

        return LibraryScan(photoCount: photoCount, videoCount: videoCount, estimatedBytes: totalBytes, assets: assets)
    }

    /// Imports PHAssets created after the last recorded watermark date.
    /// On first run, stores the current date and imports nothing.
    /// Returns the number of newly imported assets.
    func importNewAssets(libraryId: String, albumId: String, lib: FfiLibrary) async -> Int {
        let status = await PHPhotoLibrary.requestAuthorization(for: .readWrite)
        guard status == .authorized || status == .limited else { return 0 }

        let now = Date()

        guard let since = lastImportDate(libraryId: libraryId) else {
            setLastImportDate(now, libraryId: libraryId)
            return 0
        }

        let fetchOptions = PHFetchOptions()
        fetchOptions.predicate = NSPredicate(format: "creationDate > %@", since as NSDate)
        fetchOptions.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: true)]

        let result = PHAsset.fetchAssets(with: fetchOptions)
        guard result.count > 0 else {
            setLastImportDate(now, libraryId: libraryId)
            return 0
        }

        var imported = 0
        for i in 0..<result.count {
            let asset = result.object(at: i)
            do {
                try await importPHAsset(asset, into: albumId, lib: lib)
                imported += 1
            } catch {
                AppLogger.log(.error, "auto-import asset \(asset.localIdentifier) failed: \(error)")
            }
        }

        setLastImportDate(now, libraryId: libraryId)
        return imported
    }

    // MARK: - Single asset import

    @discardableResult
    func importPHAsset(_ asset: PHAsset, into albumId: String, lib: FfiLibrary) async throws -> [String] {
        var photoResource: PHAssetResource?
        var fullSizePhotoResource: PHAssetResource?
        var adjustmentDataResource: PHAssetResource?
        // A Live Photo (Apple's name for a still with a short paired motion video captured at
        // the same moment) surfaces its video half as .pairedVideo/.fullSizePairedVideo.
        var livePhotoVideoResource: PHAssetResource?
        var otherResources: [PHAssetResource] = []

        for r in PHAssetResource.assetResources(for: asset) as [PHAssetResource] {
            switch r.type as PHAssetResourceType {
            case .photo: photoResource = r
            case .fullSizePhoto: fullSizePhotoResource = r
            case .adjustmentData: adjustmentDataResource = r
            case .pairedVideo, .fullSizePairedVideo: livePhotoVideoResource = r
            case .video, .fullSizeVideo: otherResources.append(r)
            default: break
            }
        }

        guard photoResource != nil || fullSizePhotoResource != nil || livePhotoVideoResource != nil || !otherResources.isEmpty else { return [] }

        // When both are present, fullSizePhoto is a rendered duplicate of photo with the
        // edits baked in. We only want the original on disk plus a link to its AAE sidecar.
        let isEdited = photoResource != nil && fullSizePhotoResource != nil
        if isEdited && adjustmentDataResource == nil {
            AppLogger.log(.error, "asset \(asset.localIdentifier) is edited but has no adjustment data resource, importing original photo without an AAE link")
        }

        let hasStill = photoResource != nil || fullSizePhotoResource != nil
        var mediaIds: [String] = []
        var didSetThumbnail = false

        func importResource(_ resource: PHAssetResource, albumId: String?, appleAaeMediaId: String?, appleLivePhotoMediaId: String?, allowThumbnail: Bool) async throws -> String {
            let filePath = try await downloadResource(resource)
            defer { try? FileManager.default.removeItem(at: filePath) }

            let mediaId = try await runOnBackground {
                try lib.importMedia(path: filePath.path, albumId: albumId, originalFilename: resource.originalFilename, appleAaeMediaId: appleAaeMediaId, appleLivePhotoMediaId: appleLivePhotoMediaId)
            }

            if allowThumbnail, !didSetThumbnail, let thumbData = ThumbnailGenerator.generate(for: filePath) {
                try? await runOnBackground { try lib.setMediaThumbnail(mediaId: mediaId, data: thumbData) }
                didSetThumbnail = true
            }

            return mediaId
        }

        var aaeMediaId: String?
        if isEdited, let adjustmentDataResource {
            aaeMediaId = try await importResource(adjustmentDataResource, albumId: nil, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: false)
        }

        // The Live Photo's motion video is imported first, kept out of the default album, and
        // linked from the still below. It never becomes a standalone album item.
        var livePhotoMediaId: String?
        if hasStill, let livePhotoVideoResource {
            livePhotoMediaId = try await importResource(livePhotoVideoResource, albumId: nil, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: false)
        }

        if let photoResource {
            let id = try await importResource(photoResource, albumId: albumId, appleAaeMediaId: aaeMediaId, appleLivePhotoMediaId: livePhotoMediaId, allowThumbnail: true)
            mediaIds.append(id)
        } else if let fullSizePhotoResource {
            let id = try await importResource(fullSizePhotoResource, albumId: albumId, appleAaeMediaId: nil, appleLivePhotoMediaId: livePhotoMediaId, allowThumbnail: true)
            mediaIds.append(id)
        } else if let livePhotoVideoResource, !hasStill {
            // No still was present, so the paired video is not part of a Live Photo pairing.
            // Import it as a normal standalone video.
            let id = try await importResource(livePhotoVideoResource, albumId: albumId, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: true)
            mediaIds.append(id)
        }

        for resource in otherResources {
            let t: PHAssetResourceType = resource.type
            let isVideoOnly = !hasStill && (t == .video || t == .fullSizeVideo)
            let id = try await importResource(resource, albumId: albumId, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: isVideoOnly)
            mediaIds.append(id)
        }

        return mediaIds
    }

    private func downloadResource(_ resource: PHAssetResource) async throws -> URL {
        let filePath = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(UUID().uuidString)_\(resource.originalFilename)")
        try? FileManager.default.removeItem(at: filePath)
        let options = PHAssetResourceRequestOptions()
        options.isNetworkAccessAllowed = true
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            PHAssetResourceManager.default().writeData(for: resource, toFile: filePath, options: options) { error in
                if let error { continuation.resume(throwing: error) }
                else { continuation.resume() }
            }
        }
        return filePath
    }

    private func runOnBackground<T>(_ work: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                do { continuation.resume(returning: try work()) }
                catch { continuation.resume(throwing: error) }
            }
        }
    }
}
#endif
