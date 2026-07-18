import Foundation

extension FfiGroup: @retroactive Identifiable {
    public var id: String { groupId }
}

extension LibraryModel {

    func albumGroups(albumId: String) -> [FfiGroup] {
        (try? lib?.albumListGroups(albumId: albumId)) ?? []
    }

    func groupMedia(groupId: String) -> [FfiMediaItem] {
        (try? lib?.groupListMedia(groupId: groupId)) ?? []
    }

    func createGroup(albumId: String) {
        do { _ = try lib?.createGroup(albumId: albumId) }
        catch { AppLogger.log(.error, "createGroup failed: \(error)") }
        reload()
        schedulePush()
    }

    func deleteGroup(groupId: String) {
        do { try lib?.deleteGroup(groupId: groupId) }
        catch { AppLogger.log(.error, "deleteGroup \(groupId) failed: \(error)") }
        reload()
        schedulePush()
    }

    func addMediaToGroup(groupId: String, mediaId: String) {
        do { try lib?.addMediaToGroup(groupId: groupId, mediaId: mediaId) }
        catch { AppLogger.log(.error, "addMediaToGroup failed: \(error)") }
        reload()
        schedulePush()
    }

    func removeMediaFromGroup(groupId: String, mediaId: String) {
        do { try lib?.removeMediaFromGroup(groupId: groupId, mediaId: mediaId) }
        catch { AppLogger.log(.error, "removeMediaFromGroup failed: \(error)") }
        reload()
        schedulePush()
    }

    func createGroupFromSelectedMedia(mediaIds: [String], albumId: String) {
        do {
            guard let groupId = try lib?.createGroup(albumId: albumId) else { return }
            for mediaId in mediaIds {
                try lib?.addMediaToGroup(groupId: groupId, mediaId: mediaId)
            }
            for mediaId in mediaIds {
                try lib?.removeMediaFromAlbum(albumId: albumId, mediaId: mediaId)
            }
        } catch {
            AppLogger.log(.error, "createGroupFromSelectedMedia failed: \(error)")
        }
        reload()
        schedulePush()
    }
}
