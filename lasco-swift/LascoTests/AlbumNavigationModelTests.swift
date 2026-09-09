import Foundation
import Testing
@testable import Lasco

@MainActor
struct AlbumNavigationModelTests {
    @Test
    func userOpenWinsWhenRestorationCompletesLater() async {
        let saved = album(id: "saved", parentID: nil)
        let opened = album(id: "opened", parentID: nil)
        let query = DelayedAlbumQuery()
        let model = AlbumNavigationModel(queryAlbums: query.albums)

        let restoration = Task { await model.restoreIfNeeded(savedIDs: ["saved"]) }
        await query.waitForRequest(number: 1)
        model.open(opened)
        query.resolve([saved])
        await restoration.value

        #expect(albumIDs(in: model.path) == ["opened"])
    }

    @Test
    func validationCannotOverwriteANewerPush() async {
        let root = album(id: "root", parentID: nil)
        let child = album(id: "child", parentID: root.albumId)
        let query = DelayedAlbumQuery()
        let model = AlbumNavigationModel(queryAlbums: query.albums)
        model.replacePath([.album(root)], source: .directOpen)

        let validation = Task { await model.validateCurrentPath() }
        await query.waitForRequest(number: 1)
        model.replacePath([.album(root), .album(child)], source: .navigationStack)
        query.resolve([root])
        await validation.value

        #expect(albumIDs(in: model.path) == ["root", "child"])
    }

    @Test
    func restorationAppliesValidAlbumsInSavedOrder() async {
        let root = album(id: "root", parentID: nil)
        let child = album(id: "child", parentID: root.albumId)
        let model = AlbumNavigationModel { _ in [child, root] }

        await model.restoreIfNeeded(savedIDs: ["root", "child"])

        #expect(albumIDs(in: model.path) == ["root", "child"])
    }

    @Test
    func restorationKeepsOnlyTheValidPrefix() async {
        let root = album(id: "root", parentID: nil)
        let movedChild = album(id: "child", parentID: nil)
        let model = AlbumNavigationModel { _ in [root, movedChild] }

        await model.restoreIfNeeded(savedIDs: ["root", "child"])

        #expect(albumIDs(in: model.path) == ["root"])
    }

    @Test
    func validationTrimsADeletedAlbumAfterReturningToAlbums() async {
        let root = album(id: "root", parentID: nil)
        let deletedChild = album(id: "child", parentID: root.albumId, deleted: true)
        let model = AlbumNavigationModel { _ in [root, deletedChild] }
        model.replacePath([.album(root), .album(deletedChild)], source: .directOpen)

        await model.validateCurrentPath()

        #expect(albumIDs(in: model.path) == ["root"])
    }

    @Test
    func equivalentPathDoesNotCreateANavigationMutation() {
        let root = album(id: "root", parentID: nil)
        let model = AlbumNavigationModel { _ in [] }
        model.replacePath([.album(root)], source: .directOpen)
        let revision = model.revision

        model.replacePath([.album(root)], source: .navigationStack)

        #expect(model.revision == revision)
    }

    @Test
    func cancelledRestorationCanRetry() async {
        let root = album(id: "root", parentID: nil)
        let query = DelayedAlbumQuery()
        let model = AlbumNavigationModel(queryAlbums: query.albums)

        let cancelledRestoration = Task { await model.restoreIfNeeded(savedIDs: ["root"]) }
        await query.waitForRequest(number: 1)
        cancelledRestoration.cancel()
        query.resolve([root])
        await cancelledRestoration.value
        #expect(model.path.isEmpty)

        let retry = Task { await model.restoreIfNeeded(savedIDs: ["root"]) }
        await query.waitForRequest(number: 2)
        query.resolve([root])
        await retry.value

        #expect(albumIDs(in: model.path) == ["root"])
    }

    private func album(
        id: String,
        parentID: FfiAlbumUuid?,
        deleted: Bool = false
    ) -> FfiAlbum {
        FfiAlbum(
            albumId: FfiAlbumUuid(value: id),
            name: id,
            parentAlbumId: parentID,
            mediaCount: 0,
            deleted: deleted,
            isDisconnected: false,
            thumbnailMediaId: nil
        )
    }

    private func albumIDs(in path: [AlbumsDestination]) -> [String] {
        path.compactMap { destination in
            guard case .album(let album) = destination else { return nil }
            return album.albumId.value
        }
    }
}

@MainActor
private final class DelayedAlbumQuery {
    private var requestCount = 0
    private var waiters: [Int: [CheckedContinuation<Void, Never>]] = [:]
    private var responseContinuation: CheckedContinuation<[FfiAlbum], Never>?

    func albums(_ ids: Set<FfiAlbumUuid>) async throws -> [FfiAlbum] {
        requestCount += 1
        let currentRequest = requestCount
        let currentWaiters = waiters.removeValue(forKey: currentRequest) ?? []
        currentWaiters.forEach { $0.resume() }
        return await withCheckedContinuation { continuation in
            responseContinuation = continuation
        }
    }

    func waitForRequest(number: Int) async {
        guard requestCount < number else { return }
        await withCheckedContinuation { continuation in
            waiters[number, default: []].append(continuation)
        }
    }

    func resolve(_ albums: [FfiAlbum]) {
        let continuation = responseContinuation
        responseContinuation = nil
        continuation?.resume(returning: albums)
    }
}
