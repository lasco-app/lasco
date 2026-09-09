import Testing
@testable import Lasco

struct AlbumBreadcrumbTitleTests {
    @Test
    func displaysTheFullPathForTwoAlbums() {
        #expect(albumBreadcrumbTitle(["a", "b"]) == "A / B")
    }

    @Test
    func collapsesPathsDeeperThanTwoAlbums() {
        #expect(albumBreadcrumbTitle(["a", "b", "c"]) == "... / B / C")
        #expect(albumBreadcrumbTitle(["a", "b", "c", "d"]) == "... / C / D")
    }
}
