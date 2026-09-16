#if canImport(UIKit)
import Photos
import UIKit
import LascoPhotoImportKit

actor PhotoLibraryImporter {
    private static let cloudMappingBatchSize = 250
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
        let assets: [PreparedAsset]
        let albums: [AlbumNode]
    }

    /// A prepared, current-process-only PhotoKit reference. The durable identity, when present,
    /// is the cloud ID; no local identifier crosses into album/provenance state.
    struct PreparedAsset {
        let asset: PHAsset
        let cloudAssetId: String?
        /// This only exists for the current scan. It lets albums retain membership for assets
        /// which Photos cannot map to an iCloud identifier.
        let membershipIdentity: AlbumMembershipIdentity
    }

    /// A durable cloud identifier when PhotoKit supplies one, otherwise an opaque identifier
    /// scoped to this scan. Neither variant is written to the Lasco library as media metadata.
    enum AlbumMembershipIdentity: Hashable {
        case cloudAsset(String)
        case session(UUID)
    }

    struct IgnoredAsset {
        /// A display-only, current-process key. Never expose PhotoKit's local identifier.
        let sessionID = UUID()
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
    private static func applePhotosImportPlan(_ asset: PHAsset, analysis: AssetAnalysis, cloudAssetId: String?) -> ApplePhotosImportPlan? {
        guard let cloudAssetId else { return nil }

        // The package is the single authority for AAE → paired-video → primary selection.
        // Keep the PHAssetResource objects at this app boundary only for eventual staging.
        let resourcesWithCandidates = (PHAssetResource.assetResources(for: asset) as [PHAssetResource]).compactMap { resource -> (ResourceSelection.Candidate, PHAssetResource)? in
            guard let type = sharedResourceType(resource) else { return nil }
            return (.init(ticket: UUID().uuidString, type: type, filename: resource.originalFilename, byteCount: 0), resource)
        }
        let resourceByTicket = Dictionary(uniqueKeysWithValues: resourcesWithCandidates.map { ($0.0.ticket, $0.1) })
        let resources = ResourceSelection.select(from: resourcesWithCandidates.map(\.0)).compactMap { selected -> PlannedResource? in
            guard let resource = resourceByTicket[selected.candidate.ticket] else { return nil }
            return PlannedResource(resource: resource, type: ffiResourceType(selected.candidate.type), isLinkable: selected.role == .primary)
        }

        guard !resources.isEmpty else { return nil }
        let modificationDate = asset.modificationDate.map { ISO8601DateFormatter().string(from: $0) }
        return ApplePhotosImportPlan(
            revision: FfiApplePhotosAssetRevision(
                cloudAssetId: cloudAssetId,
                modificationDate: modificationDate,
                resources: resources.map { FfiApplePhotosResourceDescriptor(resourceType: $0.type, filename: $0.resource.originalFilename) }
            ),
            resources: resources
        )
    }

    private static func sharedResourceType(_ resource: PHAssetResource) -> PhotosResourceType? {
        switch resource.type as PHAssetResourceType {
        case .photo: .photo
        case .fullSizePhoto: .fullSizePhoto
        case .video: .video
        case .fullSizeVideo: .fullSizeVideo
        case .adjustmentData: .adjustmentData
        case .pairedVideo: .pairedVideo
        case .fullSizePairedVideo: .fullSizePairedVideo
        default: nil
        }
    }

    private static func ffiResourceType(_ type: PhotosResourceType) -> FfiApplePhotosResourceType {
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

    /// Resolves a complete PhotoKit batch before import planning. The archival cloud value keeps
    /// local identifiers from escaping this native adapter; older supported OS versions retain
    /// the compatible legacy value as a runtime fallback.
    private static func cloudIDs(for localIdentifiers: [String]) -> [String: String] {
        var seen = Set<String>()
        let uniqueIdentifiers = localIdentifiers.filter { seen.insert($0).inserted }
        var cloudIDs: [String: String] = [:]
        for start in stride(from: 0, to: uniqueIdentifiers.count, by: cloudMappingBatchSize) {
            let end = min(start + cloudMappingBatchSize, uniqueIdentifiers.count)
            let batch = Array(uniqueIdentifiers[start..<end])
            let mappings = PHPhotoLibrary.shared().cloudIdentifierMappings(forLocalIdentifiers: batch)
            for localIdentifier in batch {
                guard case .success(let cloudIdentifier)? = mappings[localIdentifier] else { continue }
                if #available(iOS 18.2, macOS 15.2, *) {
                    cloudIDs[localIdentifier] = cloudIdentifier.archivalStringValue
                } else {
                    cloudIDs[localIdentifier] = cloudIdentifier.stringValue
                }
            }
        }
        return cloudIDs
    }

    struct AlbumNode {
        let cloudCollectionId: String
        let kind: FfiApplePhotosCollectionKind
        let name: String
        let parentCloudCollectionId: String?
        let memberAssetIdentities: [AlbumMembershipIdentity]
    }

    private struct RawAlbumNode {
        let localID: String
        let name: String
        let parentLocalID: String?
        let memberLocalAssetIDs: [String]
        let kind: FfiApplePhotosCollectionKind
    }

    /// Walks the iOS album/folder tree and returns a flat, parent-before-child list.
    /// Folders become nodes with no direct members, real user albums carry their asset identifiers.
    func scanAlbumTree() async -> [AlbumNode] {
        let nodes = rawAlbumTree()
        let cloudIDs = Self.cloudIDs(for: nodes.map(\.localID) + nodes.flatMap(\.memberLocalAssetIDs))
        let sessionIdentities = Dictionary(uniqueKeysWithValues: Set(nodes.flatMap(\.memberLocalAssetIDs)).map {
            ($0, AlbumMembershipIdentity.session(UUID()))
        })
        return albumNodes(from: nodes, cloudIDs: cloudIDs, sessionIdentities: sessionIdentities)
    }

    private func rawAlbumTree() -> [RawAlbumNode] {
        var nodes: [RawAlbumNode] = []
        let topLevel = PHCollectionList.fetchTopLevelUserCollections(with: nil)
        walkCollections(topLevel, parentLocalID: nil, into: &nodes)
        return nodes
    }

    private func albumNodes(
        from nodes: [RawAlbumNode],
        cloudIDs: [String: String],
        sessionIdentities: [String: AlbumMembershipIdentity]
    ) -> [AlbumNode] {
        func cloudID(_ localID: String) -> String? {
            cloudIDs[localID]
        }
        return nodes.compactMap { node in
            guard let cloudCollectionId = cloudID(node.localID) else { return nil }
            return AlbumNode(
                cloudCollectionId: cloudCollectionId,
                kind: node.kind,
                name: node.name,
                parentCloudCollectionId: node.parentLocalID.flatMap(cloudID),
                memberAssetIdentities: node.memberLocalAssetIDs.compactMap { localID in
                    if let cloudAssetID = cloudID(localID) {
                        return .cloudAsset(cloudAssetID)
                    }
                    return sessionIdentities[localID]
                }
            )
        }
    }

    private func walkCollections(_ result: PHFetchResult<PHCollection>, parentLocalID: String?, into nodes: inout [RawAlbumNode]) {
        for i in 0..<result.count {
            let collection = result.object(at: i)
            if let folder = collection as? PHCollectionList {
                nodes.append(RawAlbumNode(localID: folder.localIdentifier, name: folder.localizedTitle ?? "", parentLocalID: parentLocalID, memberLocalAssetIDs: [], kind: .folder))
                let children = PHCollection.fetchCollections(in: folder, options: nil)
                walkCollections(children, parentLocalID: folder.localIdentifier, into: &nodes)
            } else if let album = collection as? PHAssetCollection,
                      album.assetCollectionType == .album, album.assetCollectionSubtype == .albumRegular {
                let assets = PHAsset.fetchAssets(in: album, options: nil)
                var memberAssetIds: [String] = []
                memberAssetIds.reserveCapacity(assets.count)
                for j in 0..<assets.count {
                    memberAssetIds.append(assets.object(at: j).localIdentifier)
                }
                nodes.append(RawAlbumNode(localID: album.localIdentifier, name: album.localizedTitle ?? "", parentLocalID: parentLocalID, memberLocalAssetIDs: memberAssetIds, kind: .album))
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
        let rawAlbums = rawAlbumTree()
        let assetLocalIDs = (0..<allAssets.count).map { allAssets.object(at: $0).localIdentifier }
        let cloudIDs = Self.cloudIDs(for:
            assetLocalIDs
                + rawAlbums.map(\.localID)
                + rawAlbums.flatMap(\.memberLocalAssetIDs)
        )
        let sessionIdentities = Dictionary(uniqueKeysWithValues: assetLocalIDs.map {
            ($0, AlbumMembershipIdentity.session(UUID()))
        })
        let albums = albumNodes(
            from: rawAlbums,
            cloudIDs: cloudIDs,
            sessionIdentities: sessionIdentities
        )
        var assets: [PreparedAsset] = []
        assets.reserveCapacity(allAssets.count)

        for i in 0..<allAssets.count {
            let asset = allAssets.object(at: i)
            let analysis = Self.analyzeAsset(asset)

            let cloudAssetId = cloudIDs[asset.localIdentifier]
            if let plan = Self.applePhotosImportPlan(asset, analysis: analysis, cloudAssetId: cloudAssetId) {
                do {
                    if try await repository.applePhotosAssetRevisionMediaIDs(plan.revision) != nil {
                        continue
                    }
                } catch {
                    AppLogger.log(.error, "Apple Photos revision lookup failed: \(error)")
                }
            }

            switch analysis.kind {
            case .photo:
                photoCount += 1
                assets.append(PreparedAsset(
                    asset: asset,
                    cloudAssetId: cloudAssetId,
                    membershipIdentity: cloudAssetId.map(AlbumMembershipIdentity.cloudAsset)
                        ?? sessionIdentities[asset.localIdentifier]!
                ))
            case .video:
                videoCount += 1
                assets.append(PreparedAsset(
                    asset: asset,
                    cloudAssetId: cloudAssetId,
                    membershipIdentity: cloudAssetId.map(AlbumMembershipIdentity.cloudAsset)
                        ?? sessionIdentities[asset.localIdentifier]!
                ))
            case nil:
                ignoredAssets.append(IgnoredAsset(mediaType: asset.mediaType, creationDate: asset.creationDate))
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
            assets: assets,
            albums: albums
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
        let cloudIDs = Self.cloudIDs(for: (0..<result.count).map { result.object(at: $0).localIdentifier })
        for i in 0..<result.count {
            guard !Task.isCancelled else { return imported }
            let asset = result.object(at: i)
            do {
                let ids = try await importPHAssetResources(
                    asset,
                    cloudAssetId: cloudIDs[asset.localIdentifier],
                    into: albumId,
                    repository: repository
                ).linkableMediaIDs
                if !ids.isEmpty { imported += 1 }
            } catch {
                AppLogger.log(.error, "auto-import asset failed: \(error)")
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
    func importPHAssetResources(_ asset: PHAsset, cloudAssetId: String? = nil, into albumId: FfiAlbumUuid?, repository: any LibraryRepositoryProtocol) async throws -> ImportedAsset {
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
        let importPlan = Self.applePhotosImportPlan(asset, analysis: analysis, cloudAssetId: cloudAssetId)

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
            AppLogger.log(.error, "An edited Apple Photos asset has no adjustment-data resource; importing the original photo without an AAE link")
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
