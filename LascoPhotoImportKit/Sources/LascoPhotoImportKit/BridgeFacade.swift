import Foundation

/// JSON transport for hosts which cannot call Swift value APIs directly (for example the desktop
/// JVM bridge). This contains no PhotoKit policy or external library dependency: it only decodes
/// values, forwards them to `ApplePhotosImportSession`, then encodes the next value.
public actor PhotosImportJSONFacade {
    private var sessions: [String: ApplePhotosImportSession] = [:]

    public init() {}

    /// Starts a PhotoKit-backed session for a native host. The returned discovery can be shown to
    /// the user and its opaque tickets remain usable only through this facade's session ID.
    public func beginPhotoLibrary(remoteIDsJSON: String, chunkSize: Int = 32) async throws -> String {
        let remoteIDs = try JSONDecoder().decode([String].self, from: Data(remoteIDsJSON.utf8))
        let sessionID = UUID().uuidString
        let session = ApplePhotosImportSession(photoLibrary: PhotoLibraryDiscovery(), remoteIDs: remoteIDs, chunkSize: chunkSize)
        let discovery = try await session.discover()
        sessions[sessionID] = session
        let result = PhotosBridgeSession(sessionID: sessionID, discovery: discovery)
        return String(decoding: try JSONEncoder().encode(result), as: UTF8.self)
    }

    public func begin(discoveryJSON: String, remoteIDsJSON: String, chunkSize: Int = 32) throws -> String {
        let decoder = JSONDecoder()
        let discovery = try decoder.decode(PhotosDiscovery.self, from: Data(discoveryJSON.utf8))
        let remoteIDs = try decoder.decode([String].self, from: Data(remoteIDsJSON.utf8))
        let sessionID = UUID().uuidString
        sessions[sessionID] = ApplePhotosImportSession(discovery: discovery, remoteIDs: remoteIDs, chunkSize: chunkSize)
        return sessionID
    }

    public func advance(sessionID: String, factsJSON: String) async throws -> String {
        guard let session = sessions[sessionID] else { throw PhotoLibraryImportError.unknownResourceTicket }
        let facts = try JSONDecoder().decode([PhotosFact].self, from: Data(factsJSON.utf8))
        let step = try await session.advance(facts)
        let data = try JSONEncoder().encode(step)
        return String(decoding: data, as: UTF8.self)
    }

    public func end(sessionID: String) {
        sessions.removeValue(forKey: sessionID)
    }

    public func stage(sessionID: String, resourceTicket: String, destinationDirectory: URL) async throws -> String {
        guard let session = sessions[sessionID] else { throw PhotoLibraryImportError.unknownResourceTicket }
        let staged = try await session.stage(resourceTicket: resourceTicket, destinationDirectory: destinationDirectory)
        return String(decoding: try JSONEncoder().encode(staged), as: UTF8.self)
    }
}

public struct PhotosBridgeSession: Codable, Sendable {
    public let sessionID: String
    public let discovery: PhotosDiscovery

    public init(sessionID: String, discovery: PhotosDiscovery) {
        self.sessionID = sessionID
        self.discovery = discovery
    }
}

#if canImport(ObjectiveC)
/// Completion-handler façade for Objective-C hosts. Kotlin/JNA can use the same JSON vocabulary
/// through a tiny native adapter without reproducing discovery or planning policy.
@objcMembers
@MainActor
public final class LascoPhotoImportKitObjCFacade: NSObject {
    private let facade = PhotosImportJSONFacade()

    public func beginPhotoLibrary(remoteIDsJSON: String, chunkSize: Int, completion: @escaping (String?, NSError?) -> Void) {
        Task { @MainActor in
            do { completion(try await facade.beginPhotoLibrary(remoteIDsJSON: remoteIDsJSON, chunkSize: chunkSize), nil) }
            catch { completion(nil, error as NSError) }
        }
    }

    public func advance(sessionID: String, factsJSON: String, completion: @escaping (String?, NSError?) -> Void) {
        Task { @MainActor in
            do { completion(try await facade.advance(sessionID: sessionID, factsJSON: factsJSON), nil) }
            catch { completion(nil, error as NSError) }
        }
    }

    public func stage(sessionID: String, resourceTicket: String, destinationDirectory: URL, completion: @escaping (String?, NSError?) -> Void) {
        Task { @MainActor in
            do { completion(try await facade.stage(sessionID: sessionID, resourceTicket: resourceTicket, destinationDirectory: destinationDirectory), nil) }
            catch { completion(nil, error as NSError) }
        }
    }

    public func end(sessionID: String) {
        Task { @MainActor in await facade.end(sessionID: sessionID) }
    }
}
#endif
