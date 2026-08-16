import Foundation

extension LascoError {
    var friendlyMessage: String {
        switch self {
        case .InvalidCredentials:
            return "Incorrect username or password."
        case .NotFound:
            return "That library could not be found."
        case .SyncBusy:
            return "A sync is already in progress."
        case .MissingLocalMedia:
            return "Some media are not available locally."
        case .Storage(let msg), .Other(let msg):
            return msg
        }
    }
}
