import Foundation
import Security

struct LascoCloudRemote: Decodable, Sendable {
    let id: String
    let name: String
    let endpoint: String
    let bucket: String
    let region: String
    let pathPrefix: String
    let accessKeyId: String
    let secretAccessKey: String
    let sessionToken: String?
    let expiresAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, endpoint, bucket, region
        case pathPrefix = "path_prefix"
        case accessKeyId = "access_key_id"
        case secretAccessKey = "secret_access_key"
        case sessionToken = "session_token"
        case expiresAt = "expires_at"
    }
}

struct LascoCloudCredentialsResponse: Decodable, Sendable { let remotes: [LascoCloudRemote] }
private struct LascoCloudLogin: Decodable { let token: String }
private struct LascoCloudLoginRequest: Encodable {
    let email: String; let password: String; let platform: String
    let appVersion: String
    enum CodingKeys: String, CodingKey { case email, password, platform; case appVersion = "app_version" }
}

enum LascoCloudError: LocalizedError { case unauthorized; case invalidRemoteCount
    var errorDescription: String? { self == .unauthorized ? "Authenticate with Lasco Cloud again" : "Lasco Cloud must provide two storage remotes" }
}

actor LascoCloudClient {
    private let baseURL: URL
    init() { baseURL = URL(string: Bundle.main.object(forInfoDictionaryKey: "LASCO_CLOUD_URL") as? String ?? "https://api.lasco.cloud")! }

    func login(libraryID: String, email: String, password: String) async throws {
        let request = LascoCloudLoginRequest(email: email, password: password, platform: "ios", appVersion: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0")
        let data = try JSONEncoder().encode(request)
        let login: LascoCloudLogin = try await send(path: "api/v1/sessions", token: nil, body: data)
        try KeychainTokenStore.save(login.token, libraryID: libraryID)
    }

    func storageCredentials(libraryID: String) async throws -> [LascoCloudRemote] {
        guard let token = KeychainTokenStore.token(libraryID: libraryID) else { throw LascoCloudError.unauthorized }
        do {
            let response: LascoCloudCredentialsResponse = try await send(
                path: "api/v1/storage-credentials", token: token, body: nil
            )
            return response.remotes
        } catch LascoCloudError.unauthorized { KeychainTokenStore.remove(libraryID: libraryID); throw LascoCloudError.unauthorized }
    }

    private func send<T: Decodable>(path: String, token: String?, body: Data?) async throws -> T {
        var request = URLRequest(url: baseURL.appending(path: path))
        request.httpMethod = "POST"; request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body { request.httpBody = body; request.setValue("application/json", forHTTPHeaderField: "Content-Type") }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw URLError(.badServerResponse) }
        if http.statusCode == 401 { throw LascoCloudError.unauthorized }
        guard (200..<300).contains(http.statusCode) else { throw URLError(.badServerResponse) }
        return try JSONDecoder().decode(T.self, from: data)
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
