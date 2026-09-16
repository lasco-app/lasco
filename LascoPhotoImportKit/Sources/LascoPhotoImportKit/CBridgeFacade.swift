import Foundation
#if canImport(Photos)
@preconcurrency import Photos
import Darwin

/// C symbols retained for the desktop JNA boundary. They marshal JSON and opaque tickets only;
/// PhotoKit selection, mapping, staging, and session ownership remain in this package.
private actor CBridgeRegistry {
    static let shared = CBridgeRegistry()
    private let photoLibrary = PhotoLibraryDiscovery()
    private var scannedCount = 0
    private var totalCount = 0

    func discoverJSON() async throws -> String {
        let discovery = try await photoLibrary.discover()
        scannedCount = discovery.scannedAssetCount
        totalCount = discovery.totalAssetCount
        return String(decoding: try JSONEncoder().encode(discovery), as: UTF8.self)
    }

    func stage(resourceTicket: String, directory: String) async throws -> String {
        let staged = try await photoLibrary.stage(
            resourceTicket: resourceTicket,
            destinationDirectory: URL(fileURLWithPath: directory)
        )
        return staged.path.path
    }

    func scanned() -> Int32 { Int32(clamping: scannedCount) }
    func total() -> Int32 { Int32(clamping: totalCount) }
}

private final class CBridgeWaiter<Value: Sendable>: @unchecked Sendable {
    private let condition = NSCondition()
    private var result: Result<Value, Error>?

    func resolve(_ result: Result<Value, Error>) {
        condition.lock()
        self.result = result
        condition.signal()
        condition.unlock()
    }

    func wait() throws -> Value {
        condition.lock()
        while result == nil { condition.wait() }
        let result = self.result!
        condition.unlock()
        return try result.get()
    }
}

private func waitForBridge<Value: Sendable>(_ operation: @escaping @Sendable () async throws -> Value) throws -> Value {
    let waiter = CBridgeWaiter<Value>()
    Task.detached {
        do { waiter.resolve(.success(try await operation())) }
        catch { waiter.resolve(.failure(error)) }
    }
    return try waiter.wait()
}

private func retainedCString(_ value: String) -> UnsafeMutablePointer<CChar>? { strdup(value) }

@_cdecl("lasco_photos_authorization_status")
public func lascoPhotosAuthorizationStatus() -> Int32 {
    Int32(PHPhotoLibrary.authorizationStatus(for: .readWrite).rawValue)
}

@_cdecl("lasco_photos_request_authorization")
public func lascoPhotosRequestAuthorization() -> Int32 {
    (try? waitForBridge { Int32((await PHPhotoLibrary.requestAuthorization(for: .readWrite)).rawValue) })
        ?? lascoPhotosAuthorizationStatus()
}

@_cdecl("lasco_photos_discover_json")
public func lascoPhotosDiscoverJSON() -> UnsafeMutablePointer<CChar>? {
    do { return retainedCString(try waitForBridge { try await CBridgeRegistry.shared.discoverJSON() }) }
    catch { return nil }
}

@_cdecl("lasco_photos_discovery_scanned_count")
public func lascoPhotosDiscoveryScannedCount() -> Int32 {
    (try? waitForBridge { await CBridgeRegistry.shared.scanned() }) ?? 0
}

@_cdecl("lasco_photos_discovery_total_count")
public func lascoPhotosDiscoveryTotalCount() -> Int32 {
    (try? waitForBridge { await CBridgeRegistry.shared.total() }) ?? 0
}

@_cdecl("lasco_photos_stage")
public func lascoPhotosStage(_ resourceTicket: UnsafePointer<CChar>?, _ directory: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let resourceTicket, let directory else { return nil }
    let ticket = String(cString: resourceTicket)
    let path = String(cString: directory)
    do { return retainedCString(try waitForBridge { try await CBridgeRegistry.shared.stage(resourceTicket: ticket, directory: path) }) }
    catch { return nil }
}

@_cdecl("lasco_photos_free_string")
public func lascoPhotosFreeString(_ value: UnsafeMutablePointer<CChar>?) {
    guard let value else { return }
    free(value)
}
#endif
