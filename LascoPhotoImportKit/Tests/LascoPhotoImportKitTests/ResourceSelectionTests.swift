import XCTest
@testable import LascoPhotoImportKit

final class ResourceSelectionTests: XCTestCase {
    func testEditedLivePhotoSelectsAAEThenPairedVideoThenPrimary() {
        let selected = ResourceSelection.select(from: [
            .init(ticket: "full", type: .fullSizePhoto, filename: "edited.heic", byteCount: 1),
            .init(ticket: "aae", type: .adjustmentData, filename: "edit.aae", byteCount: 2),
            .init(ticket: "motion", type: .pairedVideo, filename: "motion.mov", byteCount: 3),
            .init(ticket: "primary", type: .photo, filename: "original.heic", byteCount: 4),
        ])

        XCTAssertEqual(selected.map(\.candidate.ticket), ["aae", "motion", "primary"])
        XCTAssertEqual(selected.map(\.role), [.adjustmentData, .pairedVideo, .primary])
    }

    func testStandalonePairedVideoIsPrimary() {
        let selected = ResourceSelection.select(from: [
            .init(ticket: "motion", type: .fullSizePairedVideo, filename: "motion.mov", byteCount: 3),
        ])
        XCTAssertEqual(selected.map(\.role), [.primary])
    }

    func testStandalonePairedVideoDoesNotLinkToItself() {
        let resource = PhotosResource(
            ticket: "motion", assetTicket: "asset", role: .primary, type: .pairedVideo,
            filename: "motion.mov", byteCount: 3, sourceMetadata: .init(), cloudAssetID: "cloud"
        )
        XCTAssertNil(resource.companionTickets.pairedVideo)
    }
}
