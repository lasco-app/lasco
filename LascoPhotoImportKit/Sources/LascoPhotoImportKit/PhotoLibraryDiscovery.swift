import Foundation
#if canImport(Photos)
@preconcurrency import Photos
#endif

public enum PhotoLibraryImportError: LocalizedError, Sendable {
    case unavailable
    case authorizationDenied
    case unknownResourceTicket
    case invalidStagingDirectory

    public var errorDescription: String? {
        switch self {
        case .unavailable: "PhotoKit is unavailable on this platform"
        case .authorizationDenied: "Photos permission has not been granted"
        case .unknownResourceTicket: "The Photos scan has expired; rescan before staging"
        case .invalidStagingDirectory: "The staged file escaped the caller-provided directory"
        }
    }
}

/// Owns PhotoKit objects for exactly one process-local scan. Its public values never reveal
/// PhotoKit local identifiers or PhotoKit object references.
public actor PhotoLibraryDiscovery {
    #if canImport(Photos)
    private let stager = PhotoLibraryResourceStager()
    #endif

    public init() {}

    public func discover() async throws -> PhotosDiscovery {
        #if canImport(Photos)
        let status = await PHPhotoLibrary.requestAuthorization(for: .readWrite)
        guard status == .authorized || status == .limited else {
            throw PhotoLibraryImportError.authorizationDenied
        }
        await stager.replace(with: [:])

        let fetched = PHAsset.fetchAssets(with: nil)
        let assets = (0..<fetched.count).map { fetched.object(at: $0) }
        let cloudIDs = Self.cloudIDs(for: assets.map(\.localIdentifier))
        var assetTickets: [String: String] = [:]
        var assetsByLocalID: [String: PhotosAsset] = [:]
        var discoveredAssets: [PhotosAsset] = []
        var ignoredAssets: [PhotosIgnoredAsset] = []
        var resourcesByTicket: [String: PHAssetResource] = [:]

        for asset in assets {
            let assetTicket = UUID().uuidString
            assetTickets[asset.localIdentifier] = assetTicket
            let candidatesAndResources = (PHAssetResource.assetResources(for: asset) as [PHAssetResource]).compactMap { resource -> (ResourceSelection.Candidate, PHAssetResource)? in
                guard let type = Self.resourceType(resource) else { return nil }
                return (.init(ticket: UUID().uuidString, type: type, filename: resource.originalFilename, byteCount: Self.byteCount(resource)), resource)
            }
            let candidates = candidatesAndResources.map(\.0)
            let assetResourcesByTicket = Dictionary(uniqueKeysWithValues: candidatesAndResources.map { ($0.0.ticket, $0.1) })
            let selected = ResourceSelection.select(from: candidates)
            let companions = PhotosCompanionTickets(
                adjustmentData: selected.first(where: { $0.role == .adjustmentData })?.candidate.ticket,
                pairedVideo: selected.first(where: { $0.role == .pairedVideo })?.candidate.ticket
            )
            let metadata = PhotosSourceMetadata(
                capturedAt: asset.creationDate.map { ISO8601DateFormatter().string(from: $0) },
                modifiedAt: asset.modificationDate.map { ISO8601DateFormatter().string(from: $0) },
                latitude: asset.location?.coordinate.latitude,
                longitude: asset.location?.coordinate.longitude
            )
            let selectedResources = selected.map { selected -> PhotosResource in
                resourcesByTicket[selected.candidate.ticket] = assetResourcesByTicket[selected.candidate.ticket]
                return PhotosResource(
                    ticket: selected.candidate.ticket,
                    assetTicket: assetTicket,
                    role: selected.role,
                    type: selected.candidate.type,
                    filename: selected.candidate.filename,
                    byteCount: selected.candidate.byteCount,
                    sourceMetadata: metadata,
                    cloudAssetID: cloudIDs[asset.localIdentifier],
                    // Links belong to the primary resource only. This prevents self-links.
                    companionTickets: selected.role == .primary ? .init(
                        adjustmentData: companions.adjustmentData == selected.candidate.ticket ? nil : companions.adjustmentData,
                        pairedVideo: companions.pairedVideo == selected.candidate.ticket ? nil : companions.pairedVideo
                    ) : .init()
                )
            }
            guard !selectedResources.isEmpty else {
                ignoredAssets.append(.init(
                    kind: Self.ignoredKind(for: asset.mediaType),
                    capturedAt: asset.creationDate.map { ISO8601DateFormatter().string(from: $0) }
                ))
                continue
            }
            assetsByLocalID[asset.localIdentifier] = PhotosAsset(
                ticket: assetTicket,
                cloudAssetID: cloudIDs[asset.localIdentifier],
                modificationDate: metadata.modifiedAt,
                resources: selectedResources
            )
            discoveredAssets.append(assetsByLocalID[asset.localIdentifier]!)
        }

        let rawCollections = Self.rawCollections()
        let collectionCloudIDs = Self.cloudIDs(for: rawCollections.map(\.localID))
        let collections = rawCollections.compactMap { raw -> PhotosCollection? in
            guard let cloudCollectionID = collectionCloudIDs[raw.localID] else { return nil }
            let members = raw.memberLocalAssetIDs.compactMap { assetsByLocalID[$0] }
            return PhotosCollection(
                cloudCollectionID: cloudCollectionID,
                kind: raw.kind,
                name: raw.name,
                parentCloudCollectionID: raw.parentLocalID.flatMap { collectionCloudIDs[$0] },
                memberCloudAssetIDs: members.compactMap(\.cloudAssetID),
                memberAssetTickets: members.filter { $0.cloudAssetID == nil }.map(\.ticket)
            )
        }
        await stager.replace(with: resourcesByTicket)
        return PhotosDiscovery(
            assets: discoveredAssets,
            collections: collections,
            ignoredAssets: ignoredAssets,
            scannedAssetCount: assets.count,
            totalAssetCount: assets.count
        )
        #else
        throw PhotoLibraryImportError.unavailable
        #endif
    }

    public func stage(resourceTicket: String, destinationDirectory: URL) async throws -> StagedResource {
        #if canImport(Photos)
        return try await stager.stage(resourceTicket: resourceTicket, destinationDirectory: destinationDirectory)
        #else
        throw PhotoLibraryImportError.unavailable
        #endif
    }

    #if canImport(Photos)
    private static let cloudMappingBatchSize = 250

    private static func cloudIDs(for localIdentifiers: [String]) -> [String: String] {
        var result: [String: String] = [:]
        for batch in Array(Set(localIdentifiers)).chunked(into: cloudMappingBatchSize) {
            let mappings = PHPhotoLibrary.shared().cloudIdentifierMappings(forLocalIdentifiers: batch)
            for localIdentifier in batch {
                guard case .success(let identifier)? = mappings[localIdentifier] else { continue }
                // Persist the established deprecated serialization until a deliberate migration
                // can prove an archival-ID transition preserves existing provenance matching.
                result[localIdentifier] = identifier.stringValue
            }
        }
        return result
    }

    private static func byteCount(_ resource: PHAssetResource) -> Int64 {
        (resource.value(forKey: "fileSize") as? NSNumber)?.int64Value ?? 0
    }

    private static func resourceType(_ resource: PHAssetResource) -> PhotosResourceType? {
        switch resource.type {
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

    private static func ignoredKind(for mediaType: PHAssetMediaType) -> PhotosIgnoredAsset.Kind {
        switch mediaType {
        case .audio: .audio
        case .image: .image
        case .video: .video
        default: .unknown
        }
    }

    private struct RawCollection {
        let localID: String
        let name: String
        let parentLocalID: String?
        let memberLocalAssetIDs: [String]
        let kind: PhotosCollectionKind
    }

    private static func rawCollections() -> [RawCollection] {
        var result: [RawCollection] = []
        func walk(_ collections: PHFetchResult<PHCollection>, parentLocalID: String?) {
            for index in 0..<collections.count {
                let collection = collections.object(at: index)
                if let folder = collection as? PHCollectionList {
                    result.append(.init(localID: folder.localIdentifier, name: folder.localizedTitle ?? "", parentLocalID: parentLocalID, memberLocalAssetIDs: [], kind: .folder))
                    walk(PHCollection.fetchCollections(in: folder, options: nil), parentLocalID: folder.localIdentifier)
                } else if let album = collection as? PHAssetCollection,
                          album.assetCollectionType == .album,
                          album.assetCollectionSubtype == .albumRegular {
                    let assets = PHAsset.fetchAssets(in: album, options: nil)
                    result.append(.init(localID: album.localIdentifier, name: album.localizedTitle ?? "", parentLocalID: parentLocalID, memberLocalAssetIDs: (0..<assets.count).map { assets.object(at: $0).localIdentifier }, kind: .album))
                }
            }
        }
        walk(PHCollectionList.fetchTopLevelUserCollections(with: nil), parentLocalID: nil)
        return result
    }
    #endif
}

private extension Array {
    func chunked(into size: Int) -> [[Element]] {
        stride(from: 0, to: count, by: size).map { Array(self[$0..<Swift.min($0 + size, count)]) }
    }
}
