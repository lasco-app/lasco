import Foundation
import Observation

@MainActor
@Observable
final class OnboardingCoordinator {
    var showOnboarding = false
    var resumeLibraryID: String?
    var resumeStep = 0
    private(set) var error: String?

    func setError(_ error: Error?) {
        self.error = error?.localizedDescription
    }

    func clearError() {
        error = nil
    }
}

