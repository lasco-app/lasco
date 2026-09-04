import Foundation

enum DevelopmentCloudEndpoint {
    static let defaultURL = "http://localhost:3000"
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
