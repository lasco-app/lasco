import Foundation
import Testing
@testable import Lasco

@MainActor
struct LibraryUIStateTests {
    @Test
    func stateRoundTripsPerLibrary() throws {
        let suiteName = "LibraryUIStateTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let libraryID = FfiLibraryId(value: "library-a")
        let state = LibraryUIState(
            showingOrphans: true,
            allMediaPosition: 42,
            orphanMediaPosition: 7,
            albumPathIDs: ["root", "child"]
        )

        state.save(libraryID: libraryID, defaults: defaults)

        #expect(LibraryUIState.load(libraryID: libraryID, defaults: defaults) == state)
        #expect(LibraryUIState.load(
            libraryID: FfiLibraryId(value: "library-b"),
            defaults: defaults
        ) == LibraryUIState())
    }

    @Test
    func unsupportedVersionFallsBackToDefaults() throws {
        let suiteName = "LibraryUIStateTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let libraryID = FfiLibraryId(value: "library")
        var state = LibraryUIState(allMediaPosition: 10)
        state.version = LibraryUIState.currentVersion + 1

        state.save(libraryID: libraryID, defaults: defaults)

        #expect(LibraryUIState.load(libraryID: libraryID, defaults: defaults) == LibraryUIState())
    }

    @Test(arguments: [
        (position: -2, count: 0, expected: nil),
        (position: -2, count: 10, expected: 0),
        (position: 4, count: 10, expected: 4),
        (position: 20, count: 10, expected: 9),
    ])
    func mediaPositionIsClamped(position: Int, count: Int, expected: Int?) {
        #expect(MediaPositionRestoration.clampedPosition(position, count: count) == expected)
    }

    @Test
    func albumRestorationKeepsOnlyTheValidPrefix() {
        let root = album(id: "root", parentID: nil)
        let child = album(id: "child", parentID: root.albumId)
        let movedLeaf = album(id: "leaf", parentID: nil)

        let restored = AlbumNavigationRestoration.restoredAlbums(
            savedIDs: ["root", "child", "leaf"],
            albums: [root, child, movedLeaf]
        )

        #expect(restored.map(\.albumId.value) == ["root", "child"])
    }

    @Test
    func albumRestorationRejectsDeletedAndDisconnectedAlbums() {
        let deleted = album(id: "deleted", parentID: nil, deleted: true)
        let disconnected = album(id: "disconnected", parentID: nil, isDisconnected: true)

        #expect(AlbumNavigationRestoration.restoredAlbums(
            savedIDs: ["deleted"],
            albums: [deleted]
        ).isEmpty)
        #expect(AlbumNavigationRestoration.restoredAlbums(
            savedIDs: ["disconnected"],
            albums: [disconnected]
        ).isEmpty)
    }

    @Test
    func directlyOpenedNestedAlbumCanBeRestored() {
        let parentID = FfiAlbumUuid(value: "parent")
        let nested = album(id: "nested", parentID: parentID)

        let restored = AlbumNavigationRestoration.restoredAlbums(
            savedIDs: ["nested"],
            albums: [nested]
        )

        #expect(restored.map(\.albumId.value) == ["nested"])
    }

    private func album(
        id: String,
        parentID: FfiAlbumUuid?,
        deleted: Bool = false,
        isDisconnected: Bool = false
    ) -> FfiAlbum {
        FfiAlbum(
            albumId: FfiAlbumUuid(value: id),
            name: id,
            parentAlbumId: parentID,
            mediaCount: 0,
            deleted: deleted,
            isDisconnected: isDisconnected,
            thumbnailMediaId: nil
        )
    }
}
