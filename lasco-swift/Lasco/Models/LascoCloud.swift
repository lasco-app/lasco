import Foundation

/// Swift configures the endpoint, but Rust owns every Lasco Cloud request and
/// all credentials for the currently open library.
enum LascoCloudEndpoint {
    static var url: String {
        #if DEBUG
        DevelopmentCloudEndpoint.url
        #else
        Bundle.main.object(forInfoDictionaryKey: "LASCO_CLOUD_URL") as? String ?? "https://cloud.getlasco.app"
        #endif
    }

    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
    }

    static var buildNumber: Int {
        Int(Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "0") ?? 0
    }
}

struct ClientReleaseDecision: Decodable, Sendable {
    let updateAvailable: Bool
    let syncAllowed: Bool
    let storeURL: String
    let message: String

    enum CodingKeys: String, CodingKey {
        case updateAvailable = "update_available"
        case syncAllowed = "sync_allowed"
        case storeURL = "store_url"
        case message
    }
}

@MainActor
@Observable
final class ClientReleasePolicy {
    static let shared = ClientReleasePolicy()

    private(set) var decision: ClientReleaseDecision?

    func refresh() async {
        decision = try? await requestDecision()
    }

    /// A sync requires a fresh successful decision. Update checks are
    /// informational, but Cloud sync intentionally fails closed if policy
    /// cannot be verified.
    func syncBlockMessage() async -> String? {
        do {
            let decision = try await requestDecision()
            self.decision = decision
            return decision.syncAllowed ? nil : decision.message
        } catch {
            return "Lasco Cloud can’t verify this app version right now. Try again shortly."
        }
    }

    private func requestDecision() async throws -> ClientReleaseDecision {
        var components = URLComponents(string: LascoCloudEndpoint.url + "/api/v1/client-release-policy")!
        components.queryItems = [
            URLQueryItem(name: "platform", value: "ios"),
            URLQueryItem(name: "build", value: String(LascoCloudEndpoint.buildNumber)),
            URLQueryItem(name: "display_version", value: LascoCloudEndpoint.appVersion),
        ]
        let (data, response) = try await URLSession.shared.data(from: components.url!)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw URLError(.badServerResponse) }
        return try JSONDecoder().decode(ClientReleaseDecision.self, from: data)
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
