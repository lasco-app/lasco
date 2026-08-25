import Foundation

/// Swift configures the endpoint, but Rust owns every Lasco Cloud request and
/// all credentials for the currently open library.
enum LascoCloudEndpoint {
    static var url: String {
        #if DEBUG
        DevelopmentCloudEndpoint.url
        #else
        Bundle.main.object(forInfoDictionaryKey: "LASCO_CLOUD_URL") as? String ?? "https://api.lasco.cloud"
        #endif
    }

    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
    }
}

struct LascoCloudRemote: Sendable {
    let id: String
    let libraryID: String?
    let name: String
    let endpoint: String
    let bucket: String
    let region: String
    let pathPrefix: String

    init(ffi: FfiLascoCloudRemote) {
        id = ffi.id
        libraryID = ffi.libraryId
        name = ffi.name
        endpoint = ffi.endpoint
        bucket = ffi.bucket
        region = ffi.region
        pathPrefix = ffi.pathPrefix
    }
}

struct LascoCloudAccount: Sendable {
    let email: String
    let subscription: LascoCloudSubscription?

    init(ffi: FfiLascoCloudAccount) {
        email = ffi.email
        subscription = ffi.subscription.map { LascoCloudSubscription(ffi: $0) }
    }
}

struct LascoCloudSubscription: Sendable {
    let planID: String
    let planName: String
    let status: String
    let storageQuotaBytes: Int64
    let renewsAt: String

    init(ffi: FfiLascoCloudSubscription) {
        planID = ffi.planId
        planName = ffi.planName
        status = ffi.status
        storageQuotaBytes = Int64(clamping: ffi.storageQuotaBytes)
        renewsAt = ffi.renewsAt
    }
}

enum LascoCloudError: LocalizedError {
    case invalidRemoteCount

    var errorDescription: String? {
        switch self {
        case .invalidRemoteCount:
            "Lasco Cloud must provide two storage remotes"
        }
    }
}
