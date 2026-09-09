import Testing
@testable import Lasco

@MainActor
struct MediaGallerySessionTests {
    @Test
    func seedIsAvailableBeforeAnyAsyncWork() {
        let items = Self.items(count: 20)
        let session = MediaGallerySession(
            seed: MediaGallerySeed(
                source: .homeByDate,
                position: 7,
                item: items[7],
                totalCount: items.count
            ),
            dataSource: FakeGalleryDataSource(items: items)
        )

        #expect(session.totalCount == 20)
        #expect(session.selectedPosition == 7)
        #expect(session.item(at: 7)?.id == items[7].id)
    }

    @Test
    func selectingUnloadedPositionLoadsItsBatch() async {
        let items = Self.items(count: 30)
        let session = MediaGallerySession(
            seed: MediaGallerySeed(
                source: .homeByDate,
                position: 0,
                item: items[0],
                totalCount: items.count
            ),
            dataSource: FakeGalleryDataSource(items: items)
        )

        session.select(17)
        await session.ensureLoaded(at: 17)

        #expect(session.selectedPosition == 17)
        #expect(session.item(at: 17)?.id == items[17].id)
        #expect(session.item(at: 0) == nil)
    }

    @Test
    func countFailureDoesNotBlankSeededPage() async {
        let items = Self.items(count: 4)
        let session = MediaGallerySession(
            seed: MediaGallerySeed(
                source: .homeByDate,
                position: 2,
                item: items[2],
                totalCount: items.count
            ),
            dataSource: FakeGalleryDataSource(items: items, countFails: true)
        )

        await session.start()

        #expect(session.totalCount == 4)
        #expect(session.item(at: 2)?.id == items[2].id)
    }

    private static func items(count: Int) -> [AlbumItem] {
        (0..<count).map { index in
            .media(FfiMediaItem(
                mediaId: FfiMediaUuid(value: "media-\(index)"),
                filenameOriginal: "image-\(index).jpg",
                name: "Image \(index)",
                date: "2026-01-01T00:00:00Z",
                year: 2026,
                month: 1,
                sizeBytes: 1,
                contentHash: "hash-\(index)",
                author: "test",
                appleAaeMediaId: nil,
                appleLivePhotoMediaId: nil
            ))
        }
    }
}

private enum FakeGalleryError: Error {
    case countFailed
    case notFound
}

private struct FakeGalleryDataSource: MediaGalleryDataLoading {
    let storedItems: [AlbumItem]
    let countFails: Bool

    init(items: [AlbumItem], countFails: Bool = false) {
        storedItems = items
        self.countFails = countFails
    }

    func changes() async -> AsyncStream<LibraryChange> {
        AsyncStream { continuation in continuation.finish() }
    }

    func count() async throws -> Int {
        if countFails { throw FakeGalleryError.countFailed }
        return storedItems.count
    }

    func items(in range: Range<Int>) async throws -> [AlbumItem] {
        Array(storedItems[range])
    }

    func position(of itemID: AlbumItem.ID, count: Int) async throws -> Int? {
        storedItems.firstIndex(where: { $0.id == itemID })
    }

    func groupMedia(groupID: FfiGroupUuid) async throws -> [FfiMediaItem] {
        []
    }

    func media(id: FfiMediaUuid) async throws -> FfiMediaItem {
        for case .media(let media) in storedItems where media.mediaId == id {
            return media
        }
        throw FakeGalleryError.notFound
    }

    func containingAlbumIDs(mediaID: FfiMediaUuid) async throws -> [FfiAlbumUuid] {
        []
    }

    func albums(withIDs ids: Set<FfiAlbumUuid>) async throws -> [FfiAlbum] {
        []
    }
}
