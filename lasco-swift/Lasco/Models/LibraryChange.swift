import Foundation

enum LibraryChange: Sendable, Hashable {
    case all
    case session
    case mediaList
    case media(String)
    case albumList
    case album(String)
    case localMutation
}

