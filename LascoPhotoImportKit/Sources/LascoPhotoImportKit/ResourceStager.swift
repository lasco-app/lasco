import Foundation
#if canImport(Photos)
@preconcurrency import Photos

/// The only component that retains `PHAssetResource` values. Replacing the registry invalidates
/// every old ticket, making interrupted imports rescan instead of replaying source locators.
actor PhotoLibraryResourceStager {
    private var resources: [String: PHAssetResource] = [:]

    func replace(with resources: [String: PHAssetResource]) {
        self.resources = resources
    }

    func stage(resourceTicket: String, destinationDirectory: URL) async throws -> StagedResource {
        guard let resource = resources[resourceTicket] else { throw PhotoLibraryImportError.unknownResourceTicket }
        try FileManager.default.createDirectory(at: destinationDirectory, withIntermediateDirectories: true)
        let filename = URL(fileURLWithPath: resource.originalFilename).lastPathComponent
        let destination = destinationDirectory.appendingPathComponent("\(UUID().uuidString)-\(filename)")
        let options = PHAssetResourceRequestOptions()
        options.isNetworkAccessAllowed = true
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            PHAssetResourceManager.default().writeData(for: resource, toFile: destination, options: options) { error in
                if let error { continuation.resume(throwing: error) }
                else { continuation.resume(returning: ()) }
            }
        }
        guard destination.standardizedFileURL.path.hasPrefix(destinationDirectory.standardizedFileURL.path + "/") else {
            throw PhotoLibraryImportError.invalidStagingDirectory
        }
        return StagedResource(resourceTicket: resourceTicket, path: destination)
    }
}
#endif
