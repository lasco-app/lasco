import Foundation
import Observation

@MainActor
@Observable
final class MediaImportCoordinator {
    private(set) var isImporting = false
    private(set) var progress: (done: Int, total: Int)?

    private let repository: any LibraryRepositoryProtocol
    private var importTask: Task<String?, Never>?

    init(repository: any LibraryRepositoryProtocol) {
        self.repository = repository
    }

    func importMedia(urls: [URL], albumID: String) async -> String? {
        guard importTask == nil else { return nil }
        isImporting = true
        progress = (0, urls.count)
        let task = Task { await self.performImportMedia(urls: urls, albumID: albumID) }
        importTask = task
        let result = await task.value
        importTask = nil
        isImporting = false
        progress = nil
        return result
    }

    func close() {
        importTask?.cancel()
        importTask = nil
        isImporting = false
        progress = nil
    }

    private func performImportMedia(urls: [URL], albumID: String) async -> String? {
        var sources: [MediaImportSource] = []
        for url in urls {
            let accessed = url.startAccessingSecurityScopedResource()
            defer { if accessed { url.stopAccessingSecurityScopedResource() } }
            sources.append(MediaImportSource(path: url.path))
        }
        do {
            let ids = try await repository.importMediaBatch(sources, albumID: albumID)
            for (index, id) in ids.enumerated() {
                guard !Task.isCancelled else { return nil }
                if let data = ThumbnailGenerator.generate(for: urls[index]) {
                    try? await repository.setMediaThumbnail(mediaID: id, data: data)
                }
                progress = (index + 1, urls.count)
            }
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}
