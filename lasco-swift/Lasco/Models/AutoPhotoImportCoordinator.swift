import Foundation
import Observation
#if canImport(UIKit)
import Photos
#endif

@MainActor
@Observable
final class AutoPhotoImportCoordinator {
    private(set) var isImporting = false

    private let repository: any LibraryRepositoryProtocol
    private let session: LibrarySessionState
    private var importTask: Task<Int, Never>?
    #if canImport(UIKit)
    private let photoImporter = PhotoLibraryImporter()
    #endif

    init(repository: any LibraryRepositoryProtocol, session: LibrarySessionState) {
        self.repository = repository
        self.session = session
    }

    func close() {
        importTask?.cancel()
        importTask = nil
        isImporting = false
    }

    #if canImport(UIKit)
    func importFromPhotoLibrary() async {
        guard session.autoImportDeviceMedia, importTask == nil else { return }
        isImporting = true
        let task = Task { [photoImporter, repository, session] in
            await photoImporter.importNewAssets(
                libraryId: session.libraryID,
                albumId: nil,
                repository: repository
            )
        }
        importTask = task
        let imported = await task.value
        importTask = nil
        isImporting = false
        if imported > 0 {
            await repository.notifyPhotoImportChanged(initialImport: false)
        }
    }
    #endif
}
