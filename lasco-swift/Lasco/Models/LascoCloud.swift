import Foundation
import Security

private struct LascoCloudRemoteInfo: Decodable, Sendable {
    let id: String
    let libraryID: String?
    let name: String
    let endpoint: String
    let bucket: String
    let region: String
    let pathPrefix: String

    enum CodingKeys: String, CodingKey {
        case id, name, endpoint, bucket, region
        case libraryID = "library_id"
        case pathPrefix = "path_prefix"
    }
}

private struct LascoCloudRemoteCredentials: Decodable, Sendable {
    let id: String
    let accessKeyId: String
    let secretAccessKey: String
    let expiresAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case accessKeyId = "access_key_id"
        case secretAccessKey = "secret_access_key"
        case expiresAt = "expires_at"
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
    let accessKeyId: String
    let secretAccessKey: String
    let expiresAt: String

    fileprivate init(info: LascoCloudRemoteInfo, credentials: LascoCloudRemoteCredentials) {
        id = info.id; libraryID = info.libraryID; name = info.name; endpoint = info.endpoint; bucket = info.bucket
        region = info.region; pathPrefix = info.pathPrefix
        accessKeyId = credentials.accessKeyId; secretAccessKey = credentials.secretAccessKey
        expiresAt = credentials.expiresAt
    }
}

struct LascoCloudSession: Sendable {
    let baseURL: String
    let token: String
}

private struct LascoCloudRemoteInfoResponse: Decodable { let remotes: [LascoCloudRemoteInfo] }
private struct LascoCloudCredentialsResponse: Decodable { let credentials: [LascoCloudRemoteCredentials] }
private struct LascoCloudLogin: Decodable { let token: String }
struct LascoCloudAccount: Decodable, Sendable {
    let email: String
    let subscription: LascoCloudSubscription?
}
struct LascoCloudSubscription: Decodable, Sendable {
    let planID: String
    let planName: String
    let status: String
    let storageQuotaBytes: Int64
    let renewsAt: String

    enum CodingKeys: String, CodingKey {
        case status
        case planID = "plan_id"
        case planName = "plan_name"
        case storageQuotaBytes = "storage_quota_bytes"
        case renewsAt = "renews_at"
    }
}
private struct LascoCloudLoginRequest: Encodable {
    let email: String; let password: String; let platform: String
    let appVersion: String
    enum CodingKeys: String, CodingKey { case email, password, platform; case appVersion = "app_version" }
}
private struct LascoCloudRemoteLibraryIDRequest: Encodable {
    let libraryID: String

    enum CodingKeys: String, CodingKey { case libraryID = "library_id" }
}

enum LascoCloudError: LocalizedError {
    case unauthorized
    case invalidRemoteCount
    case invalidResponse(endpoint: String, detail: String)
    case connectionFailed(endpoint: String)
    case requestFailed(endpoint: String, statusCode: Int)

    var errorDescription: String? {
        switch self {
        case .unauthorized:
            "Authenticate with Lasco Cloud again"
        case .invalidRemoteCount:
            "Lasco Cloud must provide two storage remotes"
        case .invalidResponse(let endpoint, let detail):
            "Lasco Cloud returned an invalid response from \(endpoint): \(detail)"
        case .connectionFailed(let endpoint):
            "Couldn't reach Lasco Cloud at \(endpoint). Make sure the server is running. On a physical iPhone, use your Mac's LAN address instead of localhost or 127.0.0.1."
        case .requestFailed(let endpoint, let statusCode):
            "Lasco Cloud request to \(endpoint) failed (HTTP \(statusCode))."
        }
    }
}

actor LascoCloudClient {
    private let baseURL: URL
    init() {
        #if DEBUG
        baseURL = URL(string: DevelopmentCloudEndpoint.url)!
        #else
        baseURL = URL(string: Bundle.main.object(forInfoDictionaryKey: "LASCO_CLOUD_URL") as? String ?? "https://api.lasco.cloud")!
        #endif
    }

    func login(libraryID: String, email: String, password: String) async throws {
        let request = LascoCloudLoginRequest(email: email, password: password, platform: "ios", appVersion: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0")
        let data = try JSONEncoder().encode(request)
        let login: LascoCloudLogin = try await send(path: "api/v1/sessions", token: nil, body: data)
        try KeychainTokenStore.save(login.token, libraryID: libraryID)
    }

    func logout(libraryID: String) {
        KeychainTokenStore.remove(libraryID: libraryID)
    }

    func isLoggedIn(libraryID: String) -> Bool {
        KeychainTokenStore.token(libraryID: libraryID) != nil
    }

    func session(libraryID: String) throws -> LascoCloudSession {
        guard let token = KeychainTokenStore.token(libraryID: libraryID) else {
            throw LascoCloudError.unauthorized
        }
        return LascoCloudSession(baseURL: baseURL.absoluteString, token: token)
    }

    func storageCredentials(libraryID: String) async throws -> [LascoCloudRemote] {
        guard let token = KeychainTokenStore.token(libraryID: libraryID) else { throw LascoCloudError.unauthorized }
        do {
            let info: LascoCloudRemoteInfoResponse = try await send(path: "api/v1/remotes", method: "GET", token: token, body: nil)
            let response: LascoCloudCredentialsResponse = try await send(path: "api/v1/storage-credentials", method: "POST", token: token, body: nil)
            let credentials = Dictionary(uniqueKeysWithValues: response.credentials.map { ($0.id, $0) })
            guard info.remotes.count == 2, credentials.count == info.remotes.count,
                  info.remotes.allSatisfy({ credentials[$0.id] != nil }) else {
                throw LascoCloudError.invalidRemoteCount
            }
            return info.remotes.compactMap { info in
                credentials[info.id].map { LascoCloudRemote(info: info, credentials: $0) }
            }
        } catch LascoCloudError.unauthorized { KeychainTokenStore.remove(libraryID: libraryID); throw LascoCloudError.unauthorized }
    }

    func setRemoteLibraryIDs(libraryID: String, remoteIDs: [String]) async throws {
        guard let token = KeychainTokenStore.token(libraryID: libraryID) else { throw LascoCloudError.unauthorized }
        do {
            let body = try JSONEncoder().encode(LascoCloudRemoteLibraryIDRequest(libraryID: libraryID))
            for remoteID in remoteIDs {
                try await sendNoContent(
                    path: "api/v1/remotes/\(remoteID)/library-id",
                    method: "PUT",
                    token: token,
                    body: body
                )
            }
        } catch LascoCloudError.unauthorized {
            KeychainTokenStore.remove(libraryID: libraryID)
            throw LascoCloudError.unauthorized
        }
    }

    func subscription(libraryID: String) async throws -> LascoCloudAccount {
        guard let token = KeychainTokenStore.token(libraryID: libraryID) else { throw LascoCloudError.unauthorized }
        do {
            let response: LascoCloudAccount = try await send(
                path: "api/v1/subscription",
                method: "GET",
                token: token,
                body: nil
            )
            return response
        } catch LascoCloudError.unauthorized {
            KeychainTokenStore.remove(libraryID: libraryID)
            throw LascoCloudError.unauthorized
        }
    }

    private func send<T: Decodable>(path: String, method: String = "POST", token: String?, body: Data?) async throws -> T {
        let data = try await sendData(path: path, method: method, token: token, body: body)
        do {
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            AppLogger.log(.error, "Lasco Cloud response decode failed for \(path): \(error)")
            throw LascoCloudError.invalidResponse(endpoint: path, detail: decodingDiagnostic(error))
        }
    }

    private func sendNoContent(path: String, method: String, token: String?, body: Data?) async throws {
        _ = try await sendData(path: path, method: method, token: token, body: body)
    }

    private func sendData(path: String, method: String = "POST", token: String?, body: Data?) async throws -> Data {
        var request = URLRequest(url: baseURL.appending(path: path))
        request.httpMethod = method; request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body { request.httpBody = body; request.setValue("application/json", forHTTPHeaderField: "Content-Type") }
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch let error as URLError where isConnectionError(error) {
            throw LascoCloudError.connectionFailed(endpoint: baseURL.absoluteString)
        }
        guard let http = response as? HTTPURLResponse else { throw URLError(.badServerResponse) }
        if http.statusCode == 401 { throw LascoCloudError.unauthorized }
        guard (200..<300).contains(http.statusCode) else {
            throw LascoCloudError.requestFailed(endpoint: path, statusCode: http.statusCode)
        }
        return data
    }

    private func decodingDiagnostic(_ error: Error) -> String {
        guard let error = error as? DecodingError else { return error.localizedDescription }

        func path(_ context: DecodingError.Context) -> String {
            let values = context.codingPath.map(\.stringValue)
            return values.isEmpty ? "the response root" : values.joined(separator: ".")
        }

        switch error {
        case .keyNotFound(let key, let context):
            return "Missing required field '\(key.stringValue)' at \(path(context)). \(context.debugDescription)"
        case .typeMismatch(let type, let context):
            return "Expected \(type) at \(path(context)), but received a different type. \(context.debugDescription)"
        case .valueNotFound(let type, let context):
            return "Expected a non-null \(type) at \(path(context)). \(context.debugDescription)"
        case .dataCorrupted(let context):
            return "Malformed data at \(path(context)). \(context.debugDescription)"
        @unknown default:
            return error.localizedDescription
        }
    }

    private func isConnectionError(_ error: URLError) -> Bool {
        switch error.code {
        case .cannotConnectToHost, .cannotFindHost, .dnsLookupFailed, .networkConnectionLost,
                .notConnectedToInternet, .timedOut:
            true
        default:
            false
        }
    }
}

private enum KeychainTokenStore {
    nonisolated static func save(_ token: String, libraryID: String) throws {
        remove(libraryID: libraryID)
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "com.lasco.lasco.cloud", kSecAttrAccount as String: libraryID, kSecValueData as String: Data(token.utf8)]
        guard SecItemAdd(query as CFDictionary, nil) == errSecSuccess else { throw URLError(.cannotWriteToFile) }
    }
    nonisolated static func token(libraryID: String) -> String? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "com.lasco.lasco.cloud", kSecAttrAccount as String: libraryID, kSecReturnData as String: true]
        var result: CFTypeRef?; guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess, let data = result as? Data else { return nil }; return String(data: data, encoding: .utf8)
    }
    nonisolated static func remove(libraryID: String) { SecItemDelete([kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "com.lasco.lasco.cloud", kSecAttrAccount as String: libraryID] as CFDictionary) }
}
