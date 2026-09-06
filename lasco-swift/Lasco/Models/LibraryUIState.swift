import Foundation

struct LibraryUIState: Codable, Equatable {
    static let currentVersion = 1

    var version = currentVersion
    var showingOrphans = false
    var allMediaPosition = 0
    var orphanMediaPosition = 0
    var albumPathIDs: [String] = []

    static func load(
        libraryID: FfiLibraryId,
        defaults: UserDefaults = .standard
    ) -> LibraryUIState {
        guard let data = defaults.data(forKey: storageKey(libraryID: libraryID)),
              let decoded = try? JSONDecoder().decode(LibraryUIState.self, from: data),
              decoded.version == currentVersion else {
            return LibraryUIState()
        }

        var state = decoded
        state.allMediaPosition = max(0, state.allMediaPosition)
        state.orphanMediaPosition = max(0, state.orphanMediaPosition)
        return state
    }

    func save(
        libraryID: FfiLibraryId,
        defaults: UserDefaults = .standard
    ) {
        guard let data = try? JSONEncoder().encode(self) else { return }
        defaults.set(data, forKey: Self.storageKey(libraryID: libraryID))
    }

    static func remove(
        libraryID: FfiLibraryId,
        defaults: UserDefaults = .standard
    ) {
        defaults.removeObject(forKey: storageKey(libraryID: libraryID))
    }

    private static func storageKey(libraryID: FfiLibraryId) -> String {
        "lasco.libraryUIState.\(libraryID.value)"
    }
}

enum MediaPositionRestoration {
    static func clampedPosition(_ position: Int, count: Int) -> Int? {
        guard count > 0 else { return nil }
        return min(max(0, position), count - 1)
    }
}

enum AlbumNavigationRestoration {
    static func restoredAlbums(savedIDs: [String], albums: [FfiAlbum]) -> [FfiAlbum] {
        let albumsByID = Dictionary(albums.map { ($0.albumId.value, $0) }) { current, _ in current }
        var previousAlbumID: FfiAlbumUuid?
        var restored: [FfiAlbum] = []

        for savedID in savedIDs {
            guard let album = albumsByID[savedID],
                  !album.deleted,
                  !album.isDisconnected else {
                break
            }
            if let previousAlbumID, album.parentAlbumId != previousAlbumID { break }
            restored.append(album)
            previousAlbumID = album.albumId
        }

        return restored
    }
}
