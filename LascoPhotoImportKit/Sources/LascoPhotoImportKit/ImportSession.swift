import Foundation

public struct CollectionLink: Codable, Hashable, Sendable {
    public let cloudCollectionID: String
    public let albumID: String?
    public init(cloudCollectionID: String, albumID: String?) { self.cloudCollectionID = cloudCollectionID; self.albumID = albumID }
}

public struct AlbumMembership: Codable, Hashable, Sendable {
    public let mediaID: String
    public let albumIDs: Set<String>
    public init(mediaID: String, albumIDs: Set<String>) { self.mediaID = mediaID; self.albumIDs = albumIDs }
}

public struct PhotosImportAction: Codable, Hashable, Sendable {
    public let resource: PhotosResource
    public let adjustmentDataMediaID: String?
    public let pairedVideoMediaID: String?
    public init(resource: PhotosResource, adjustmentDataMediaID: String?, pairedVideoMediaID: String?) {
        self.resource = resource
        self.adjustmentDataMediaID = adjustmentDataMediaID
        self.pairedVideoMediaID = pairedVideoMediaID
    }
}

public struct PhotosMembershipAction: Codable, Hashable, Sendable {
    public let mediaID: String
    public let albumID: String
    public init(mediaID: String, albumID: String) { self.mediaID = mediaID; self.albumID = albumID }
}

public struct PhotosChunk: Codable, Hashable, Sendable {
    public let number: Int
    public let assetTickets: [String]
    public init(number: Int, assetTickets: [String]) { self.number = number; self.assetTickets = assetTickets }
}

public enum PhotosChunkResult: String, Codable, Sendable { case succeeded, paused, cancelled }

/// Every cross-boundary exchange is a value. In particular, the Swift actor never calls a host
/// callback and never receives a Lasco object.
public enum PhotosFact: Codable, Sendable {
    case revisionMatches([RevisionMatch])
    case collectionLinks([CollectionLink])
    case albumMemberships([AlbumMembership])
    case remoteInventories([RemoteInventory])
    case collectionCreated(cloudCollectionID: String, albumID: String)
    case resourceImported(resourceTicket: String, mediaID: String)
    case membershipAdded(PhotosMembershipAction)
    case chunkFinished(PhotosChunkResult)
    case failed(message: String)
}

public enum PhotosStep: Codable, Sendable {
    case lookupRevisions([AssetManifest])
    case lookupCollectionLinks([String])
    case lookupAlbumMemberships([String])
    case confirmRemoteInventory(remoteIDs: [String], mediaIDs: [String])
    case createCollection(PhotosCollection)
    case importResource(PhotosImportAction)
    case addMembership(PhotosMembershipAction)
    case chunkReady(PhotosChunk)
    case paused
    case finished
}

public enum ApplePhotosImportSessionError: LocalizedError, Sendable {
    case unexpectedFacts
    case failedHostAction(String)
    case invalidActionResult
    case missingDependency(String)
    case missingCollectionParent(String)

    public var errorDescription: String? {
        switch self {
        case .unexpectedFacts: "The host supplied facts that do not answer the requested Photos step"
        case .failedHostAction(let message): message
        case .invalidActionResult: "The host completed a different Photos action than requested"
        case .missingDependency(let ticket): "The primary resource requires unavailable companion \(ticket)"
        case .missingCollectionParent(let id): "The collection parent \(id) was not created or linked"
        }
    }
}

/// Process-local planner for an Apple Photos scan. The host owns every Lasco read/write and feeds
/// the resulting facts back into this actor. Restarting creates a new scan and a new session.
public actor ApplePhotosImportSession {
    private enum Phase { case fresh, revisions, collectionLinks, memberships, inventory, actions, chunk, paused, finished }
    private enum Work {
        case create(PhotosCollection)
        case importResource(PhotosResource)
        case membership(assetTicket: String, collectionID: String)
    }

    private let discoveryProvider: PhotoLibraryDiscovery?
    private var discoveryValue: PhotosDiscovery?
    private let remoteIDs: [String]
    private let chunkSize: Int
    private var phase: Phase = .fresh
    private var mediaIDByResourceTicket: [String: String] = [:]
    private var albumIDByCollectionID: [String: String] = [:]
    private var membershipsByMediaID: [String: Set<String>] = [:]
    private var work: [Work] = []
    private var nextAssetIndex = 0
    private var activeChunk: PhotosChunk?

    public init(discovery: PhotosDiscovery, remoteIDs: [String], chunkSize: Int = 32) {
        self.discoveryProvider = nil
        self.discoveryValue = discovery
        self.remoteIDs = remoteIDs
        self.chunkSize = max(1, chunkSize)
    }

    public init(photoLibrary: PhotoLibraryDiscovery, remoteIDs: [String], chunkSize: Int = 32) {
        self.discoveryProvider = photoLibrary
        self.discoveryValue = nil
        self.remoteIDs = remoteIDs
        self.chunkSize = max(1, chunkSize)
    }

    public func discover() async throws -> PhotosDiscovery {
        if let discoveryValue { return discoveryValue }
        guard let discoveryProvider else { throw ApplePhotosImportSessionError.unexpectedFacts }
        let discovery = try await discoveryProvider.discover()
        discoveryValue = discovery
        return discovery
    }

    public func stage(resourceTicket: String, destinationDirectory: URL) async throws -> StagedResource {
        guard let discoveryProvider else { throw PhotoLibraryImportError.unknownResourceTicket }
        return try await discoveryProvider.stage(resourceTicket: resourceTicket, destinationDirectory: destinationDirectory)
    }

    public func advance(_ facts: [PhotosFact]) throws -> PhotosStep {
        if case .failed(let message)? = facts.first { throw ApplePhotosImportSessionError.failedHostAction(message) }
        let discovery = try requireDiscovery()
        switch phase {
        case .fresh:
            guard facts.isEmpty else { throw ApplePhotosImportSessionError.unexpectedFacts }
            phase = .revisions
            return .lookupRevisions(discovery.assets.compactMap(\.manifest))
        case .revisions:
            guard case .revisionMatches(let matches)? = only(facts) else { throw ApplePhotosImportSessionError.unexpectedFacts }
            for match in matches where match.isComplete {
                guard let asset = discovery.assets.first(where: { $0.manifest == match.manifest }), let mediaIDs = match.mediaIDs else { continue }
                for (resource, mediaID) in zip(asset.resources, mediaIDs) { mediaIDByResourceTicket[resource.ticket] = mediaID }
            }
            phase = .collectionLinks
            return .lookupCollectionLinks(discovery.collections.map(\.cloudCollectionID))
        case .collectionLinks:
            guard case .collectionLinks(let links)? = only(facts) else { throw ApplePhotosImportSessionError.unexpectedFacts }
            links.forEach { if let id = $0.albumID { albumIDByCollectionID[$0.cloudCollectionID] = id } }
            phase = .memberships
            return .lookupAlbumMemberships(primaryMediaIDs(in: discovery))
        case .memberships:
            guard case .albumMemberships(let memberships)? = only(facts) else { throw ApplePhotosImportSessionError.unexpectedFacts }
            memberships.forEach { membershipsByMediaID[$0.mediaID] = $0.albumIDs }
            phase = .inventory
            return .confirmRemoteInventory(remoteIDs: remoteIDs, mediaIDs: mediaIDByResourceTicket.values.sorted())
        case .inventory:
            guard case .remoteInventories? = only(facts) else { throw ApplePhotosImportSessionError.unexpectedFacts }
            phase = .actions
            enqueueNextChunk(in: discovery, includeCollections: true)
            return try nextAction(in: discovery)
        case .actions:
            try applyActionFacts(facts, in: discovery)
            return try nextAction(in: discovery)
        case .chunk:
            guard case .chunkFinished(let result)? = only(facts) else { throw ApplePhotosImportSessionError.unexpectedFacts }
            switch result {
            case .succeeded:
                activeChunk = nil
                if nextAssetIndex >= discovery.assets.count { phase = .finished; return .finished }
                phase = .actions
                enqueueNextChunk(in: discovery, includeCollections: false)
                return try nextAction(in: discovery)
            case .paused: phase = .paused; return .paused
            case .cancelled: phase = .finished; return .finished
            }
        case .paused, .finished:
            guard facts.isEmpty else { throw ApplePhotosImportSessionError.unexpectedFacts }
            return phase == .paused ? .paused : .finished
        }
    }

    private func requireDiscovery() throws -> PhotosDiscovery {
        guard let discoveryValue else { throw ApplePhotosImportSessionError.unexpectedFacts }
        return discoveryValue
    }

    private func only(_ facts: [PhotosFact]) -> PhotosFact? { facts.count == 1 ? facts[0] : nil }

    private func primaryMediaIDs(in discovery: PhotosDiscovery) -> [String] {
        discovery.resources.filter { $0.role == .primary }.compactMap { mediaIDByResourceTicket[$0.ticket] }.sorted()
    }

    private func enqueueNextChunk(in discovery: PhotosDiscovery, includeCollections: Bool) {
        if includeCollections {
            let ordered = parentFirstCollections(discovery.collections)
            work += ordered.filter { albumIDByCollectionID[$0.cloudCollectionID] == nil }.map(Work.create)
        }
        let assets = discovery.assets.dropFirst(nextAssetIndex).prefix(chunkSize)
        activeChunk = PhotosChunk(number: (nextAssetIndex / chunkSize) + 1, assetTickets: assets.map(\.ticket))
        nextAssetIndex += assets.count
        for asset in assets {
            for resource in asset.resources where mediaIDByResourceTicket[resource.ticket] == nil { work.append(.importResource(resource)) }
            guard let primary = asset.resources.first(where: { $0.role == .primary }) else { continue }
            for collection in collectionsContaining(asset: asset, all: discovery.collections) {
                work.append(.membership(assetTicket: asset.ticket, collectionID: collection.cloudCollectionID))
            }
            // `primary` intentionally only validates the shape here. Companions are resolved when
            // the action is emitted, after prior ordered work has completed.
            _ = primary
        }
    }

    private func nextAction(in discovery: PhotosDiscovery) throws -> PhotosStep {
        guard !work.isEmpty else {
            phase = .chunk
            return .chunkReady(activeChunk ?? PhotosChunk(number: 0, assetTickets: []))
        }
        switch work[0] {
        case .create(let collection): return .createCollection(collection)
        case .importResource(let resource):
            let dependencies = try dependencies(for: resource)
            return .importResource(.init(resource: resource, adjustmentDataMediaID: dependencies.adjustmentData, pairedVideoMediaID: dependencies.pairedVideo))
        case .membership(let assetTicket, let collectionID):
            guard let albumID = albumIDByCollectionID[collectionID] else { throw ApplePhotosImportSessionError.missingCollectionParent(collectionID) }
            guard let asset = discovery.assets.first(where: { $0.ticket == assetTicket }),
                  let primary = asset.resources.first(where: { $0.role == .primary }),
                  let mediaID = mediaIDByResourceTicket[primary.ticket] else {
                throw ApplePhotosImportSessionError.invalidActionResult
            }
            let action = PhotosMembershipAction(mediaID: mediaID, albumID: albumID)
            if membershipsByMediaID[mediaID, default: []].contains(albumID) {
                work.removeFirst()
                return try nextAction(in: discovery)
            }
            return .addMembership(action)
        }
    }

    private func dependencies(for resource: PhotosResource) throws -> (adjustmentData: String?, pairedVideo: String?) {
        guard resource.role == .primary else { return (nil, nil) }
        func mediaID(_ ticket: String?) throws -> String? {
            guard let ticket else { return nil }
            // A stale bridge must not turn one primary resource into its own prerequisite.
            // Other cycles cannot occur because a manifest has only companion → primary edges.
            guard ticket != resource.ticket else { return nil }
            guard let id = mediaIDByResourceTicket[ticket] else { throw ApplePhotosImportSessionError.missingDependency(ticket) }
            return id
        }
        return (try mediaID(resource.companionTickets.adjustmentData), try mediaID(resource.companionTickets.pairedVideo))
    }

    private func applyActionFacts(_ facts: [PhotosFact], in discovery: PhotosDiscovery) throws {
        guard let current = work.first, let fact = only(facts) else { throw ApplePhotosImportSessionError.unexpectedFacts }
        switch (current, fact) {
        case (.create(let collection), .collectionCreated(let id, let albumID)) where id == collection.cloudCollectionID:
            albumIDByCollectionID[id] = albumID
        case (.importResource(let resource), .resourceImported(let ticket, let mediaID)) where ticket == resource.ticket:
            mediaIDByResourceTicket[ticket] = mediaID
        case (.membership(_, _), .membershipAdded(let action)):
            membershipsByMediaID[action.mediaID, default: []].insert(action.albumID)
        default: throw ApplePhotosImportSessionError.invalidActionResult
        }
        work.removeFirst()
    }

    private func collectionsContaining(asset: PhotosAsset, all collections: [PhotosCollection]) -> [PhotosCollection] {
        collections.filter {
            asset.cloudAssetID.map($0.memberCloudAssetIDs.contains) == true || $0.memberAssetTickets.contains(asset.ticket)
        }
    }

    private func parentFirstCollections(_ collections: [PhotosCollection]) -> [PhotosCollection] {
        var byID = Dictionary(uniqueKeysWithValues: collections.map { ($0.cloudCollectionID, $0) })
        var emitted = Set<String>()
        var output: [PhotosCollection] = []
        func append(_ collection: PhotosCollection) {
            guard emitted.insert(collection.cloudCollectionID).inserted else { return }
            if let parentID = collection.parentCloudCollectionID, let parent = byID[parentID] { append(parent) }
            output.append(collection)
        }
        collections.forEach(append)
        byID.removeAll()
        return output
    }
}
