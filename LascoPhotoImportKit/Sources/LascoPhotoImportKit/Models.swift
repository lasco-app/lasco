import Foundation

/// The only durable identity this package exposes for a Photos asset is PhotoKit's serialized
/// cloud identifier. Process-local tickets are deliberately separate values.
public struct AssetManifest: Codable, Hashable, Sendable {
    public let cloudAssetID: String
    public let modificationDate: String?
    public let resources: [PhotosResourceDescriptor]

    public init(cloudAssetID: String, modificationDate: String?, resources: [PhotosResourceDescriptor]) {
        self.cloudAssetID = cloudAssetID
        self.modificationDate = modificationDate
        self.resources = resources
    }
}

public enum PhotosResourceRole: String, Codable, CaseIterable, Sendable {
    case adjustmentData
    case pairedVideo
    case primary
}

public enum PhotosResourceType: String, Codable, CaseIterable, Sendable {
    case photo = "PHOTO"
    case fullSizePhoto = "FULL_SIZE_PHOTO"
    case video = "VIDEO"
    case fullSizeVideo = "FULL_SIZE_VIDEO"
    case adjustmentData = "ADJUSTMENT_DATA"
    case pairedVideo = "PAIRED_VIDEO"
    case fullSizePairedVideo = "FULL_SIZE_PAIRED_VIDEO"
}

public struct PhotosResourceDescriptor: Codable, Hashable, Sendable {
    public let type: PhotosResourceType
    public let filename: String

    public init(type: PhotosResourceType, filename: String) {
        self.type = type
        self.filename = filename
    }
}

public struct PhotosSourceMetadata: Codable, Hashable, Sendable {
    public let capturedAt: String?
    public let modifiedAt: String?
    public let latitude: Double?
    public let longitude: Double?

    public init(capturedAt: String? = nil, modifiedAt: String? = nil, latitude: Double? = nil, longitude: Double? = nil) {
        self.capturedAt = capturedAt
        self.modifiedAt = modifiedAt
        self.latitude = latitude
        self.longitude = longitude
    }
}

/// `ticket` and `assetTicket` are opaque, process-local capabilities. Hosts must never persist
/// either value. They exist solely to stage a resource and to link unmapped album members during
/// this session.
public struct PhotosResource: Codable, Hashable, Sendable {
    public let ticket: String
    public let assetTicket: String
    public let role: PhotosResourceRole
    public let type: PhotosResourceType
    public let filename: String
    public let byteCount: Int64
    public let sourceMetadata: PhotosSourceMetadata
    public let cloudAssetID: String?
    public let companionTickets: PhotosCompanionTickets

    public init(ticket: String, assetTicket: String, role: PhotosResourceRole, type: PhotosResourceType, filename: String, byteCount: Int64, sourceMetadata: PhotosSourceMetadata, cloudAssetID: String?, companionTickets: PhotosCompanionTickets = .init()) {
        self.ticket = ticket
        self.assetTicket = assetTicket
        self.role = role
        self.type = type
        self.filename = filename
        self.byteCount = byteCount
        self.sourceMetadata = sourceMetadata
        self.cloudAssetID = cloudAssetID
        self.companionTickets = companionTickets
    }

    public var descriptor: PhotosResourceDescriptor { .init(type: type, filename: filename) }
}

public struct PhotosCompanionTickets: Codable, Hashable, Sendable {
    public let adjustmentData: String?
    public let pairedVideo: String?

    public init(adjustmentData: String? = nil, pairedVideo: String? = nil) {
        self.adjustmentData = adjustmentData
        self.pairedVideo = pairedVideo
    }
}

public struct PhotosAsset: Codable, Hashable, Sendable {
    public let ticket: String
    public let cloudAssetID: String?
    public let modificationDate: String?
    /// Already in manifest/dependency order: adjustment data, paired video, primary media.
    public let resources: [PhotosResource]

    public init(ticket: String, cloudAssetID: String?, modificationDate: String?, resources: [PhotosResource]) {
        self.ticket = ticket
        self.cloudAssetID = cloudAssetID
        self.modificationDate = modificationDate
        self.resources = resources
    }

    public var manifest: AssetManifest? {
        guard let cloudAssetID else { return nil }
        return .init(cloudAssetID: cloudAssetID, modificationDate: modificationDate, resources: resources.map(\.descriptor))
    }
}

public enum PhotosCollectionKind: String, Codable, Sendable { case folder, album }

public struct PhotosCollection: Codable, Hashable, Sendable {
    public let cloudCollectionID: String
    public let kind: PhotosCollectionKind
    public let name: String
    public let parentCloudCollectionID: String?
    public let memberCloudAssetIDs: [String]
    /// Opaque scan-only asset tickets for members lacking cloud identifiers.
    public let memberAssetTickets: [String]

    public init(cloudCollectionID: String, kind: PhotosCollectionKind, name: String, parentCloudCollectionID: String? = nil, memberCloudAssetIDs: [String] = [], memberAssetTickets: [String] = []) {
        self.cloudCollectionID = cloudCollectionID
        self.kind = kind
        self.name = name
        self.parentCloudCollectionID = parentCloudCollectionID
        self.memberCloudAssetIDs = memberCloudAssetIDs
        self.memberAssetTickets = memberAssetTickets
    }
}

/// A non-importable Photos asset retained only for the host's scan summary. It has no durable
/// identity and must never be written to a Lasco library.
public struct PhotosIgnoredAsset: Codable, Hashable, Sendable {
    public enum Kind: String, Codable, Sendable { case audio, image, video, unknown }

    public let kind: Kind
    public let capturedAt: String?

    public init(kind: Kind, capturedAt: String?) {
        self.kind = kind
        self.capturedAt = capturedAt
    }
}

public struct PhotosDiscovery: Codable, Sendable {
    public let assets: [PhotosAsset]
    public let collections: [PhotosCollection]
    public let ignoredAssets: [PhotosIgnoredAsset]
    public let scannedAssetCount: Int
    public let totalAssetCount: Int

    public init(
        assets: [PhotosAsset],
        collections: [PhotosCollection],
        ignoredAssets: [PhotosIgnoredAsset] = [],
        scannedAssetCount: Int? = nil,
        totalAssetCount: Int? = nil
    ) {
        self.assets = assets
        self.collections = collections
        self.ignoredAssets = ignoredAssets
        self.scannedAssetCount = scannedAssetCount ?? assets.count
        self.totalAssetCount = totalAssetCount ?? assets.count
    }

    public var resources: [PhotosResource] { assets.flatMap(\.resources) }

    private enum CodingKeys: String, CodingKey {
        case assets, collections, ignoredAssets, scannedAssetCount, totalAssetCount
    }

    /// Desktop bridges from a previous package version do not contain ignored-item metadata.
    /// Treat that absence as an empty list so an in-flight host upgrade can still resume safely.
    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        assets = try values.decode([PhotosAsset].self, forKey: .assets)
        collections = try values.decode([PhotosCollection].self, forKey: .collections)
        ignoredAssets = try values.decodeIfPresent([PhotosIgnoredAsset].self, forKey: .ignoredAssets) ?? []
        scannedAssetCount = try values.decodeIfPresent(Int.self, forKey: .scannedAssetCount) ?? assets.count
        totalAssetCount = try values.decodeIfPresent(Int.self, forKey: .totalAssetCount) ?? assets.count
    }
}

public struct StagedResource: Codable, Hashable, Sendable {
    public let resourceTicket: String
    public let path: URL

    public init(resourceTicket: String, path: URL) {
        self.resourceTicket = resourceTicket
        self.path = path
    }
}

public struct RevisionMatch: Codable, Hashable, Sendable {
    public let manifest: AssetManifest
    /// `nil` or a count mismatch means the manifest is incomplete and must be safely retried.
    public let mediaIDs: [String]?

    public init(manifest: AssetManifest, mediaIDs: [String]?) {
        self.manifest = manifest
        self.mediaIDs = mediaIDs
    }

    public var isComplete: Bool { mediaIDs?.count == manifest.resources.count }
}

public struct RemoteInventory: Codable, Hashable, Sendable {
    public let remoteID: String
    public let confirmedMediaIDs: Set<String>

    public init(remoteID: String, confirmedMediaIDs: Set<String>) {
        self.remoteID = remoteID
        self.confirmedMediaIDs = confirmedMediaIDs
    }
}
