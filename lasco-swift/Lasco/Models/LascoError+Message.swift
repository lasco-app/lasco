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
        case .CloudQuotaExceeded(let msg):
            return "Lasco Cloud storage quota would be exceeded: \(msg)"
        case .MissingLocalMedia:
            return "Some media are not available locally."
        case .MissingMediaOnConfiguredSources:
            return "Some media have no known place to be copied from."
        case .MediaTooLarge(let sizeBytes, let limitBytes):
            return "This media is too large (\(sizeBytes) bytes; limit: \(limitBytes) bytes)."
        case .CrdtRecoveryAvailable:
            return "The local library state needs recovery from its operation log."
        case .Storage(let msg), .Other(let msg):
            return msg
        }
    }
}
