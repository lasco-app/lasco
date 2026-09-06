import Foundation
import Observation

@MainActor
@Observable
final class AlbumNavigationModel {
    typealias AlbumQuery = @MainActor (Set<FfiAlbumUuid>) async throws -> [FfiAlbum]

    enum MutationSource {
        case restoration
        case validation
        case directOpen
        case navigationStack
    }

    private enum RestorationState {
        case notStarted
        case inProgress
        case complete
    }

    private let queryAlbums: AlbumQuery
    private var restorationState: RestorationState = .notStarted
    private var hasEstablishedPath = false

    private(set) var path: [AlbumsDestination] = []
    private(set) var revision = 0

    init(queryAlbums: @escaping AlbumQuery) {
        self.queryAlbums = queryAlbums
    }

    func open(_ album: FfiAlbum) {
        replacePath([.album(album)], source: .directOpen)
    }

    func replacePath(_ newPath: [AlbumsDestination], source: MutationSource) {
        guard newPath != path else { return }

        path = newPath
        revision += 1
        if source != .restoration {
            hasEstablishedPath = true
        }
    }

    func restoreIfNeeded(savedIDs: [String]) async {
        guard restorationState == .notStarted, !hasEstablishedPath else { return }
        guard !savedIDs.isEmpty else {
            restorationState = .complete
            return
        }

        restorationState = .inProgress
        let capturedRevision = revision
        let ids = Set(savedIDs.map(FfiAlbumUuid.init(value:)))

        do {
            let albums = try await queryAlbums(ids)
            guard !Task.isCancelled else {
                restorationState = .notStarted
                return
            }
            guard revision == capturedRevision, !hasEstablishedPath else {
                restorationState = .complete
                return
            }

            let restoredPath = AlbumNavigationRestoration
                .restoredAlbums(savedIDs: savedIDs, albums: albums)
                .map(AlbumsDestination.album)
            replacePath(restoredPath, source: .restoration)
            restorationState = .complete
        } catch is CancellationError {
            restorationState = .notStarted
        } catch {
            restorationState = .notStarted
            AppLogger.log(.error, "album path restoration query failed: \(error)")
        }
    }

    func validateCurrentPath() async {
        let capturedPath = path
        let capturedRevision = revision
        let albumIDs = Self.albumIDs(in: capturedPath)
        guard !albumIDs.isEmpty else { return }

        do {
            let albums = try await queryAlbums(Set(albumIDs.map(FfiAlbumUuid.init(value:))))
            guard !Task.isCancelled else { return }
            guard revision == capturedRevision, path == capturedPath else {
                return
            }

            let restoredAlbums = AlbumNavigationRestoration.restoredAlbums(
                savedIDs: albumIDs,
                albums: albums
            )
            guard restoredAlbums.map(\.albumId.value) != albumIDs else { return }
            replacePath(restoredAlbums.map(AlbumsDestination.album), source: .validation)
        } catch is CancellationError {
            return
        } catch {
            AppLogger.log(.error, "album path validation query failed: \(error)")
        }
    }

    private static func albumIDs(in path: [AlbumsDestination]) -> [String] {
        path.compactMap { destination in
            guard case .album(let album) = destination else { return nil }
            return album.albumId.value
        }
    }

}
