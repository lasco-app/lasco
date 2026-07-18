import Foundation

extension LibraryModel {

    // MARK: - Default upload album

    var defaultUploadAlbum: FfiAlbum? {
        guard let id = defaultUploadAlbumId else { return nil }
        return albums.first { $0.albumId == id }
    }

    func setDefaultUploadAlbum(albumId: String?) {
        do { try lib?.setDefaultUploadAlbum(albumId: albumId) }
        catch { AppLogger.log(.error, "setDefaultUploadAlbum failed: \(error)") }
        defaultUploadAlbumId = albumId
    }

    // MARK: - Thumbnail

    func setAlbumThumbnail(albumId: String, mediaId: String?) {
        do { try lib?.setAlbumThumbnail(albumId: albumId, mediaId: mediaId) }
        catch { AppLogger.log(.error, "setAlbumThumbnail \(albumId) failed: \(error)") }
        reload()
    }

    /// Returns the media ID for the album thumbnail — explicit if set, otherwise most recent.
    func albumThumbnailMediaId(albumId: String) -> String? {
        guard let album = albums.first(where: { $0.albumId == albumId }) else { return nil }
        if let explicit = album.thumbnailMediaId { return explicit }
        let media = mediaInAlbum(albumId: albumId)
        return media.max(by: { $0.date < $1.date })?.mediaId
    }

    // MARK: - Album items

    func albumListItemsSorted(albumId: String, ascending: Bool) -> [FfiAlbumItem] {
        (try? lib?.albumListItemsSorted(albumId: albumId, ascending: ascending)) ?? []
    }

    // MARK: - Operations

    func listOperationGroups() -> [FfiOperationGroup] {
        (try? lib?.listOperationGroups()) ?? []
    }

    // MARK: - Mutations

    func createAlbum(name: String, parentAlbumId: String? = nil) {
        AppLogger.log(.info, "creating album '\(name)'")
        do {
            _ = try lib?.createAlbum(name: name, parentAlbumId: parentAlbumId)
            albums = (try? lib?.listAlbums()) ?? []
            schedulePush()
        } catch let e as LascoError {
            AppLogger.log(.error, "createAlbum '\(name)' failed: \(e)")
            error = e.friendlyMessage
        } catch {
            AppLogger.log(.error, "createAlbum '\(name)' failed: \(error)")
            self.error = error.localizedDescription
        }
    }

    func renameAlbum(albumId: String, name: String) {
        do { try lib?.renameAlbum(albumId: albumId, name: name) }
        catch { AppLogger.log(.error, "renameAlbum \(albumId) failed: \(error)") }
        reload()
        schedulePush()
    }

    func reparentAlbum(albumId: String, newParentAlbumId: String?) {
        do { try lib?.reparentAlbum(albumId: albumId, newParentAlbumId: newParentAlbumId) }
        catch { AppLogger.log(.error, "reparentAlbum \(albumId) failed: \(error)") }
        reload()
        schedulePush()
    }

    func deleteAlbum(albumId: String) {
        do { try lib?.deleteAlbum(albumId: albumId) }
        catch { AppLogger.log(.error, "deleteAlbum \(albumId) failed: \(error)") }
        reload()
        schedulePush()
    }
}
