#if canImport(UIKit)
import Photos
import UIKit

actor PhotoLibraryImporter {
    private static func lastImportDateKey(libraryId: FfiLibraryId) -> String {
        "lasco.lastPhotoImport.\(libraryId.value)"
    }

    private func lastImportDate(libraryId: FfiLibraryId) -> Date? {
        UserDefaults.standard.object(forKey: Self.lastImportDateKey(libraryId: libraryId)) as? Date
    }

    private func setLastImportDate(_ date: Date, libraryId: FfiLibraryId) {
        UserDefaults.standard.set(date, forKey: Self.lastImportDateKey(libraryId: libraryId))
    }

    struct LibraryScan {
        let photoCount: Int
        let videoCount: Int
        let livePhotoVideoCount: Int
        let editMetadataCount: Int
        let ignoredAssets: [IgnoredAsset]
        let estimatedBytes: Int64
        let assets: [PHAsset]
    }

    struct IgnoredAsset {
        let localIdentifier: String
        let mediaType: PHAssetMediaType
        let creationDate: Date?
    }

    enum ImportKind {
        case photo
        case video
    }

    struct ImportedAsset {
        let linkableMediaIDs: [FfiMediaUuid]
        let allMediaIDs: [FfiMediaUuid]
    }

    private struct PlannedResource {
        let resource: PHAssetResource
        let type: FfiApplePhotosResourceType
        let isLinkable: Bool
    }

    private struct ApplePhotosImportPlan {
        let revision: FfiApplePhotosAssetRevision
        let resources: [PlannedResource]
    }

    struct AssetAnalysis {
        let photoResource: PHAssetResource?
        let fullSizePhotoResource: PHAssetResource?
        let adjustmentDataResource: PHAssetResource?
        let livePhotoVideoResource: PHAssetResource?
        let videoResource: PHAssetResource?
        let fullSizeVideoResource: PHAssetResource?

        var hasStill: Bool { photoResource != nil || fullSizePhotoResource != nil }
        var isEdited: Bool { photoResource != nil && fullSizePhotoResource != nil }
        var isImportable: Bool { hasStill || livePhotoVideoResource != nil || videoResource != nil || fullSizeVideoResource != nil }

        // Matches the conditions importPHAsset uses to decide whether it links a live photo
        // video or an edit's AAE sidecar alongside the still, so the scan count and the
        // actual import stay in sync.
        var importsLivePhotoVideo: Bool { hasStill && livePhotoVideoResource != nil }
        var importsEditMetadata: Bool { isEdited && adjustmentDataResource != nil }

        var kind: ImportKind? {
            guard isImportable else { return nil }
            return hasStill ? .photo : .video
        }
    }

    // Groups an asset's PHAssetResources into the roles importPHAsset and scanLibrary both
    // care about, so a resource combination is only ever interpreted in one place.
    private static func analyzeAsset(_ asset: PHAsset) -> AssetAnalysis {
        var photoResource: PHAssetResource?
        var fullSizePhotoResource: PHAssetResource?
        var adjustmentDataResource: PHAssetResource?
        var livePhotoVideoResource: PHAssetResource?
        var videoResource: PHAssetResource?
        var fullSizeVideoResource: PHAssetResource?

        for r in PHAssetResource.assetResources(for: asset) as [PHAssetResource] {
            switch r.type as PHAssetResourceType {
            case .photo: photoResource = r
            case .fullSizePhoto: fullSizePhotoResource = r
            case .adjustmentData: adjustmentDataResource = r
            case .pairedVideo: livePhotoVideoResource = r
            case .fullSizePairedVideo:
                if livePhotoVideoResource == nil { livePhotoVideoResource = r }
            case .video: videoResource = r
            case .fullSizeVideo: fullSizeVideoResource = r
            default: break
            }
        }

        return AssetAnalysis(
            photoResource: photoResource,
            fullSizePhotoResource: fullSizePhotoResource,
            adjustmentDataResource: adjustmentDataResource,
            livePhotoVideoResource: livePhotoVideoResource,
            videoResource: videoResource,
            fullSizeVideoResource: fullSizeVideoResource
        )
    }

    /// Builds the exact resource list without touching resource bytes. A missing cloud mapping is
    /// deliberately not an error: the caller imports normally but cannot use iCloud deduplication.
    private static func applePhotosImportPlan(_ asset: PHAsset, analysis: AssetAnalysis) -> ApplePhotosImportPlan? {
        guard case .success(let cloudIdentifier)? = PHPhotoLibrary.shared()
            .cloudIdentifierMappings(forLocalIdentifiers: [asset.localIdentifier])[asset.localIdentifier]
        else { return nil }

        var resources: [PlannedResource] = []
        if analysis.isEdited, let resource = analysis.adjustmentDataResource {
            resources.append(PlannedResource(resource: resource, type: .adjustmentData, isLinkable: false))
        }
        if analysis.hasStill, let resource = analysis.livePhotoVideoResource {
            let type: FfiApplePhotosResourceType = resource.type == .pairedVideo ? .pairedVideo : .fullSizePairedVideo
            resources.append(PlannedResource(resource: resource, type: type, isLinkable: false))
        }
        if let resource = analysis.photoResource {
            resources.append(PlannedResource(resource: resource, type: .photo, isLinkable: true))
        } else if let resource = analysis.fullSizePhotoResource {
            resources.append(PlannedResource(resource: resource, type: .fullSizePhoto, isLinkable: true))
        } else if let resource = analysis.livePhotoVideoResource {
            let type: FfiApplePhotosResourceType = resource.type == .pairedVideo ? .pairedVideo : .fullSizePairedVideo
            resources.append(PlannedResource(resource: resource, type: type, isLinkable: true))
        } else if let resource = analysis.videoResource ?? analysis.fullSizeVideoResource {
            let type: FfiApplePhotosResourceType = resource.type == .video ? .video : .fullSizeVideo
            resources.append(PlannedResource(resource: resource, type: type, isLinkable: true))
        }

        guard !resources.isEmpty else { return nil }
        let modificationDate = asset.modificationDate.map { ISO8601DateFormatter().string(from: $0) }
        return ApplePhotosImportPlan(
            revision: FfiApplePhotosAssetRevision(
                cloudAssetId: cloudIdentifier.stringValue,
                modificationDate: modificationDate,
                resources: resources.map { FfiApplePhotosResourceDescriptor(resourceType: $0.type, filename: $0.resource.originalFilename) }
            ),
            resources: resources
        )
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
    func scanLibrary(repository: any LibraryRepositoryProtocol) async -> LibraryScan? {
        let status = await PHPhotoLibrary.requestAuthorization(for: .readWrite)
        guard status == .authorized || status == .limited else { return nil }

        let allAssets = PHAsset.fetchAssets(with: nil)
        var photoCount = 0
        var videoCount = 0
        var livePhotoVideoCount = 0
        var editMetadataCount = 0
        var ignoredAssets: [IgnoredAsset] = []
        var totalBytes: Int64 = 0
        var assets: [PHAsset] = []
        assets.reserveCapacity(allAssets.count)

        for i in 0..<allAssets.count {
            let asset = allAssets.object(at: i)
            let analysis = Self.analyzeAsset(asset)

            // This is the same metadata-only revision lookup performed again immediately before
            // staging. It keeps already-imported iCloud assets out of the initial queue.
            if let plan = Self.applePhotosImportPlan(asset, analysis: analysis) {
                do {
                    if try await repository.applePhotosAssetRevisionMediaIDs(plan.revision) != nil {
                        continue
                    }
                } catch {
                    AppLogger.log(.error, "Apple Photos revision lookup failed for \(asset.localIdentifier): \(error)")
                }
            }

            switch analysis.kind {
            case .photo:
                photoCount += 1
                assets.append(asset)
            case .video:
                videoCount += 1
                assets.append(asset)
            case nil:
                ignoredAssets.append(IgnoredAsset(localIdentifier: asset.localIdentifier, mediaType: asset.mediaType, creationDate: asset.creationDate))
                continue
            }

            if analysis.importsLivePhotoVideo { livePhotoVideoCount += 1 }
            if analysis.importsEditMetadata { editMetadataCount += 1 }

            let resources = PHAssetResource.assetResources(for: asset)
            for resource in resources {
                if let size = resource.value(forKey: "fileSize") as? Int64 {
                    totalBytes += size
                    break
                }
            }
        }

        return LibraryScan(
            photoCount: photoCount,
            videoCount: videoCount,
            livePhotoVideoCount: livePhotoVideoCount,
            editMetadataCount: editMetadataCount,
            ignoredAssets: ignoredAssets,
            estimatedBytes: totalBytes,
            assets: assets
        )
    }

    /// Imports PHAssets created after the last recorded watermark date.
    /// On first run, stores the current date and imports nothing.
    /// Returns the number of newly imported assets.
    func importNewAssets(libraryId: FfiLibraryId, albumId: FfiAlbumUuid?, repository: any LibraryRepositoryProtocol) async -> Int {
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
            guard !Task.isCancelled else { return imported }
            let asset = result.object(at: i)
            do {
                let ids = try await importPHAsset(asset, into: albumId, repository: repository)
                if !ids.isEmpty { imported += 1 }
            } catch {
                AppLogger.log(.error, "auto-import asset \(asset.localIdentifier) failed: \(error)")
            }
        }

        guard !Task.isCancelled else { return imported }
        setLastImportDate(now, libraryId: libraryId)
        return imported
    }

    // MARK: - Single asset import

    @discardableResult
    func importPHAsset(_ asset: PHAsset, into albumId: FfiAlbumUuid?, repository: any LibraryRepositoryProtocol) async throws -> [FfiMediaUuid] {
        try await importPHAssetResources(asset, into: albumId, repository: repository).linkableMediaIDs
    }

    @discardableResult
    func importPHAssetResources(_ asset: PHAsset, into albumId: FfiAlbumUuid?, repository: any LibraryRepositoryProtocol) async throws -> ImportedAsset {
        let analysis = Self.analyzeAsset(asset)
        guard analysis.isImportable else {
            return ImportedAsset(linkableMediaIDs: [], allMediaIDs: [])
        }

        let photoResource = analysis.photoResource
        let fullSizePhotoResource = analysis.fullSizePhotoResource
        let adjustmentDataResource = analysis.adjustmentDataResource
        // A Live Photo (Apple's name for a still with a short paired motion video captured at
        // the same moment) surfaces its video half as .pairedVideo/.fullSizePairedVideo.
        let livePhotoVideoResource = analysis.livePhotoVideoResource
        let videoResource = analysis.videoResource ?? analysis.fullSizeVideoResource
        let importPlan = Self.applePhotosImportPlan(asset, analysis: analysis)

        if let importPlan,
           let existingMediaIDs = try await repository.applePhotosAssetRevisionMediaIDs(importPlan.revision) {
            let linkableMediaIDs = zip(importPlan.resources, existingMediaIDs)
                .compactMap { $0.isLinkable ? $1 : nil }
            if let albumId {
                for mediaId in linkableMediaIDs {
                    try await repository.addMediaToAlbumWithoutNotification(albumID: albumId, mediaID: mediaId)
                }
            }
            return ImportedAsset(linkableMediaIDs: linkableMediaIDs, allMediaIDs: existingMediaIDs)
        }

        // When both are present, fullSizePhoto is a rendered duplicate of photo with the
        // edits baked in. We only want the original on disk plus a link to its AAE sidecar.
        let isEdited = analysis.isEdited
        if isEdited && adjustmentDataResource == nil {
            AppLogger.log(.error, "asset \(asset.localIdentifier) is edited but has no adjustment data resource, importing original photo without an AAE link")
        }

        let hasStill = analysis.hasStill
        var linkableMediaIDs: [FfiMediaUuid] = []
        var allMediaIDs: [FfiMediaUuid] = []
        var didSetThumbnail = false

        func importResource(_ resource: PHAssetResource, resourceType: FfiApplePhotosResourceType, albumId: FfiAlbumUuid?, appleAaeMediaId: FfiMediaUuid?, appleLivePhotoMediaId: FfiMediaUuid?, allowThumbnail: Bool) async throws -> FfiMediaUuid {
            let filePath = try await downloadResource(resource)
            defer { try? FileManager.default.removeItem(at: filePath) }

            let mediaId = try await repository.importMediaWithoutNotification(
                source: MediaImportSource(
                    path: filePath.path,
                    originalFilename: resource.originalFilename,
                    appleAaeMediaID: appleAaeMediaId,
                    appleLivePhotoMediaID: appleLivePhotoMediaId
                ),
                albumID: albumId
            )

            if allowThumbnail, !didSetThumbnail, let thumbData = ThumbnailGenerator.generate(for: filePath) {
                try? await repository.setMediaThumbnail(mediaID: mediaId, data: thumbData)
                didSetThumbnail = true
            }

            if let importPlan {
                try await repository.recordApplePhotosResourceOrigin(
                    FfiApplePhotosResourceOrigin(
                        mediaId: mediaId,
                        cloudAssetId: importPlan.revision.cloudAssetId,
                        modificationDate: importPlan.revision.modificationDate,
                        resourceType: resourceType,
                        filename: resource.originalFilename
                    )
                )
            }

            return mediaId
        }

        var aaeMediaId: FfiMediaUuid?
        if isEdited, let adjustmentDataResource {
            aaeMediaId = try await importResource(adjustmentDataResource, resourceType: .adjustmentData, albumId: nil, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: false)
            if let aaeMediaId { allMediaIDs.append(aaeMediaId) }
        }

        // The Live Photo's motion video is imported first without album membership, and
        // linked from the still below. It never becomes a standalone album item.
        var livePhotoMediaId: FfiMediaUuid?
        if hasStill, let livePhotoVideoResource {
            let type: FfiApplePhotosResourceType = livePhotoVideoResource.type == .pairedVideo ? .pairedVideo : .fullSizePairedVideo
            livePhotoMediaId = try await importResource(livePhotoVideoResource, resourceType: type, albumId: nil, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: false)
            if let livePhotoMediaId { allMediaIDs.append(livePhotoMediaId) }
        }

        if let photoResource {
            let id = try await importResource(photoResource, resourceType: .photo, albumId: albumId, appleAaeMediaId: aaeMediaId, appleLivePhotoMediaId: livePhotoMediaId, allowThumbnail: true)
            linkableMediaIDs.append(id)
            allMediaIDs.append(id)
        } else if let fullSizePhotoResource {
            let id = try await importResource(fullSizePhotoResource, resourceType: .fullSizePhoto, albumId: albumId, appleAaeMediaId: nil, appleLivePhotoMediaId: livePhotoMediaId, allowThumbnail: true)
            linkableMediaIDs.append(id)
            allMediaIDs.append(id)
        } else if let livePhotoVideoResource, !hasStill {
            // No still was present, so the paired video is not part of a Live Photo pairing.
            // Import it as a normal standalone video.
            let type: FfiApplePhotosResourceType = livePhotoVideoResource.type == .pairedVideo ? .pairedVideo : .fullSizePairedVideo
            let id = try await importResource(livePhotoVideoResource, resourceType: type, albumId: albumId, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: true)
            linkableMediaIDs.append(id)
            allMediaIDs.append(id)
        }

        if livePhotoVideoResource == nil, let videoResource {
            let resourceType: FfiApplePhotosResourceType = videoResource.type == .video ? .video : .fullSizeVideo
            let id = try await importResource(videoResource, resourceType: resourceType, albumId: albumId, appleAaeMediaId: nil, appleLivePhotoMediaId: nil, allowThumbnail: !hasStill)
            linkableMediaIDs.append(id)
            allMediaIDs.append(id)
        }

        return ImportedAsset(linkableMediaIDs: linkableMediaIDs, allMediaIDs: allMediaIDs)
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

}
#endif
