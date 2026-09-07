import Foundation

enum DevelopmentCloudEndpoint {
    /// Lasco Cloud is the safe default. Debug builds present an explicit
    /// endpoint picker before they can use a development server.
    static let defaultURL = "https://cloud.getlasco.app"
    private static let key = "lasco.developmentCloudEndpoint"

    static var url: String {
        UserDefaults.standard.string(forKey: key) ?? defaultURL
    }

    static func setURL(_ value: String) {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalized = trimmed.contains("://") ? trimmed : "http://\(trimmed)"
        UserDefaults.standard.set(normalized.trimmingCharacters(in: CharacterSet(charactersIn: "/")), forKey: key)
    }
}
