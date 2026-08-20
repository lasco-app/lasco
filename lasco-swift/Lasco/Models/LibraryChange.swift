import Foundation

enum LibraryChange: Sendable, Hashable {
    case all
    case session
    case mediaList
    case media(FfiMediaUuid)
    case albumList
    case album(FfiAlbumUuid)
    case localMutation
}
