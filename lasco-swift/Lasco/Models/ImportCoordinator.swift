import Foundation
import Observation
#if canImport(UIKit)
import Photos
#endif

@MainActor
@Observable
final class ImportCoordinator {
    private(set) var isImporting = false
    private(set) var isAutoImporting = false
    private(set) var progress: (done: Int, total: Int)?

    private let repository: any LibraryRepositoryProtocol
    private let session: LibrarySessionState
    private var cancelImportTask: (@Sendable () -> Void)?
    #if canImport(UIKit)
    private let photoImporter = PhotoLibraryImporter()
    #endif

    init(repository: any LibraryRepositoryProtocol, session: LibrarySessionState) {
        self.repository = repository
        self.session = session
    }

    func importMedia(urls: [URL], albumID: String) async -> String? {
        guard !isImporting else { return nil }
        isImporting = true
        progress = (0, urls.count)
        let task = Task { [weak self] in
            await self?.performImportMedia(urls: urls, albumID: albumID)
        }
        cancelImportTask = { task.cancel() }
        defer { cancelImportTask = nil }
        return await task.value
    }

    private func performImportMedia(urls: [URL], albumID: String) async -> String? {
        defer {
            isImporting = false
            progress = nil
        }

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

    func setAutoImportDeviceMedia(_ enabled: Bool) async throws {
        try await repository.setAutoImportDeviceMedia(enabled: enabled)
    }

    func close() {
        cancelImportTask?()
        cancelImportTask = nil
        isImporting = false
        isAutoImporting = false
        progress = nil
    }

    #if canImport(UIKit)
    func scanPhotoLibrary() async -> PhotoLibraryImporter.LibraryScan? {
        await photoImporter.scanLibrary()
    }

    func importFromPhotoLibraryWithAlbums(assets: [PHAsset]) async {
        guard let albumID = session.defaultUploadAlbumID else { return }
        isImporting = true
        progress = (0, assets.count)
        let task = Task { [weak self] in
            await self?.performPhotoLibraryImport(assets: assets, albumID: albumID)
        }
        cancelImportTask = { task.cancel() }
        defer {
            cancelImportTask = nil
            isImporting = false
            progress = nil
        }
        await task.value
    }

    private func performPhotoLibraryImport(assets: [PHAsset], albumID: String) async {
        for (index, asset) in assets.enumerated() {
            guard !Task.isCancelled else { return }
            do {
                _ = try await photoImporter.importPHAsset(asset, into: albumID, repository: repository)
            } catch {
                AppLogger.log(.error, "photo import failed: \(error)")
            }
            progress = (index + 1, assets.count)
            if (index + 1).isMultiple(of: 32) || index == assets.count - 1 {
                await repository.notifyChanged(.all)
            }
        }
    }

    func autoImportFromPhotoLibrary() async {
        guard session.autoImportDeviceMedia,
              let albumID = session.defaultUploadAlbumID,
              !isAutoImporting,
              !isImporting else { return }
        isAutoImporting = true
        let task = Task { [weak self] in
            guard let self else { return 0 }
            return await self.photoImporter.importNewAssets(libraryId: self.session.libraryID, albumId: albumID, repository: self.repository)
        }
        cancelImportTask = { task.cancel() }
        defer {
            cancelImportTask = nil
            isAutoImporting = false
        }
        let imported = await task.value
        if imported > 0 {
            await repository.notifyChanged(.all)
        }
    }
    #endif
}
