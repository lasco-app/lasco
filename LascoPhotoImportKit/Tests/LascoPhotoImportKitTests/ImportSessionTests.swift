import XCTest
@testable import LascoPhotoImportKit

final class ImportSessionTests: XCTestCase {
    func testPlannerCreatesParentsAndImportsCompanionsBeforePrimaryMembership() async throws {
        let metadata = PhotosSourceMetadata(modifiedAt: "2026-09-16T10:00:00Z")
        let aae = PhotosResource(ticket: "aae", assetTicket: "asset", role: .adjustmentData, type: .adjustmentData, filename: "edit.aae", byteCount: 1, sourceMetadata: metadata, cloudAssetID: "cloud-asset")
        let motion = PhotosResource(ticket: "motion", assetTicket: "asset", role: .pairedVideo, type: .pairedVideo, filename: "motion.mov", byteCount: 2, sourceMetadata: metadata, cloudAssetID: "cloud-asset")
        let still = PhotosResource(ticket: "still", assetTicket: "asset", role: .primary, type: .photo, filename: "still.heic", byteCount: 3, sourceMetadata: metadata, cloudAssetID: "cloud-asset", companionTickets: .init(adjustmentData: "aae", pairedVideo: "motion"))
        let asset = PhotosAsset(ticket: "asset", cloudAssetID: "cloud-asset", modificationDate: metadata.modifiedAt, resources: [aae, motion, still])
        let folder = PhotosCollection(cloudCollectionID: "folder", kind: .folder, name: "2026")
        let album = PhotosCollection(cloudCollectionID: "album", kind: .album, name: "Trip", parentCloudCollectionID: "folder", memberCloudAssetIDs: ["cloud-asset"])
        let session = ApplePhotosImportSession(discovery: .init(assets: [asset], collections: [album, folder]), remoteIDs: ["remote"], chunkSize: 1)

        guard case .lookupRevisions(let manifests) = try await session.advance([]) else { return XCTFail("expected revision lookup") }
        XCTAssertEqual(manifests, [asset.manifest!])
        guard case .lookupCollectionLinks = try await session.advance([.revisionMatches([])]) else { return XCTFail("expected collection lookup") }
        guard case .lookupAlbumMemberships(let mediaIDs) = try await session.advance([.collectionLinks([])]) else { return XCTFail("expected membership lookup") }
        XCTAssertTrue(mediaIDs.isEmpty)
        guard case .confirmRemoteInventory(let remoteIDs, _) = try await session.advance([.albumMemberships([])]) else { return XCTFail("expected inventory lookup") }
        XCTAssertEqual(remoteIDs, ["remote"])

        guard case .createCollection(let first) = try await session.advance([.remoteInventories([])]) else { return XCTFail("expected folder creation") }
        XCTAssertEqual(first.cloudCollectionID, "folder")
        guard case .createCollection(let second) = try await session.advance([.collectionCreated(cloudCollectionID: "folder", albumID: "folder-id")]) else { return XCTFail("expected album creation") }
        XCTAssertEqual(second.cloudCollectionID, "album")
        guard case .importResource(let firstImport) = try await session.advance([.collectionCreated(cloudCollectionID: "album", albumID: "album-id")]) else { return XCTFail("expected AAE import") }
        XCTAssertEqual(firstImport.resource.ticket, "aae")
        XCTAssertNil(firstImport.adjustmentDataMediaID)

        guard case .importResource(let secondImport) = try await session.advance([.resourceImported(resourceTicket: "aae", mediaID: "aae-id")]) else { return XCTFail("expected motion import") }
        XCTAssertEqual(secondImport.resource.ticket, "motion")
        guard case .importResource(let primaryImport) = try await session.advance([.resourceImported(resourceTicket: "motion", mediaID: "motion-id")]) else { return XCTFail("expected primary import") }
        XCTAssertEqual(primaryImport.resource.ticket, "still")
        XCTAssertEqual(primaryImport.adjustmentDataMediaID, "aae-id")
        XCTAssertEqual(primaryImport.pairedVideoMediaID, "motion-id")
        guard case .addMembership(let membership) = try await session.advance([.resourceImported(resourceTicket: "still", mediaID: "still-id")]) else { return XCTFail("expected primary membership") }
        XCTAssertEqual(membership, .init(mediaID: "still-id", albumID: "album-id"))
        guard case .chunkReady(let chunk) = try await session.advance([.membershipAdded(membership)]) else { return XCTFail("expected chunk") }
        XCTAssertEqual(chunk.assetTickets, ["asset"])
        guard case .finished = try await session.advance([.chunkFinished(.succeeded)]) else { return XCTFail("expected finished") }
    }

    func testCompleteRevisionIsReusedAndOnlyPrimaryIsAddedToAlbum() async throws {
        let companion = PhotosResource(ticket: "aae", assetTicket: "asset", role: .adjustmentData, type: .adjustmentData, filename: "edit.aae", byteCount: 1, sourceMetadata: .init(), cloudAssetID: "cloud")
        let primary = PhotosResource(ticket: "still", assetTicket: "asset", role: .primary, type: .photo, filename: "still.heic", byteCount: 2, sourceMetadata: .init(), cloudAssetID: "cloud", companionTickets: .init(adjustmentData: "aae"))
        let asset = PhotosAsset(ticket: "asset", cloudAssetID: "cloud", modificationDate: nil, resources: [companion, primary])
        let collection = PhotosCollection(cloudCollectionID: "album", kind: .album, name: "Trip", memberCloudAssetIDs: ["cloud"])
        let session = ApplePhotosImportSession(discovery: .init(assets: [asset], collections: [collection]), remoteIDs: [])

        _ = try await session.advance([])
        _ = try await session.advance([.revisionMatches([.init(manifest: asset.manifest!, mediaIDs: ["aae-id", "still-id"])])])
        _ = try await session.advance([.collectionLinks([.init(cloudCollectionID: "album", albumID: "album-id")])])
        _ = try await session.advance([.albumMemberships([])])
        guard case .addMembership(let action) = try await session.advance([.remoteInventories([])]) else { return XCTFail("expected membership") }
        XCTAssertEqual(action, .init(mediaID: "still-id", albumID: "album-id"))
    }

    func testMalformedPrimarySelfLinkIsIgnored() async throws {
        let primary = PhotosResource(ticket: "still", assetTicket: "asset", role: .primary, type: .photo, filename: "still.heic", byteCount: 2, sourceMetadata: .init(), cloudAssetID: nil, companionTickets: .init(adjustmentData: "still"))
        let session = ApplePhotosImportSession(discovery: .init(assets: [.init(ticket: "asset", cloudAssetID: nil, modificationDate: nil, resources: [primary])], collections: []), remoteIDs: [])
        _ = try await session.advance([])
        _ = try await session.advance([.revisionMatches([])])
        _ = try await session.advance([.collectionLinks([])])
        _ = try await session.advance([.albumMemberships([])])
        guard case .importResource(let action) = try await session.advance([.remoteInventories([])]) else { return XCTFail("expected primary import") }
        XCTAssertNil(action.adjustmentDataMediaID)
    }
}
