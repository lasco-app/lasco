import SwiftUI

struct LascoCloudLoginView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(ToastManager.self) private var toastManager
    @Environment(\.lascoTheme) private var theme
    let repository: LibraryRepository
    let libraryID: FfiLibraryId
    let onRemoteReady: @MainActor () async throws -> Void
    @State private var email = ""
    @State private var password = ""
    @State private var submitting = false
    @State private var error: String?
    @State private var completedSteps: [String] = []
    @State private var currentStep: String?

    init(
        repository: LibraryRepository,
        libraryID: FfiLibraryId,
        onRemoteReady: @escaping @MainActor () async throws -> Void = {}
    ) {
        self.repository = repository
        self.libraryID = libraryID
        self.onRemoteReady = onRemoteReady
    }

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()
            VStack(alignment: .leading, spacing: 16) {
                Text("Lasco Cloud").font(LascoFont.title(26)).foregroundStyle(theme.ink)
                Text("Authenticate this library with your Lasco Cloud account.").font(LascoFont.body()).foregroundStyle(theme.inkSub)
                FieldLabel(text: "Email")
                TextField("you@example.com", text: $email).textFieldStyle(.plain).lascoInput().textInputAutocapitalization(.never).autocorrectionDisabled()
                FieldLabel(text: "Password")
                SecureField("", text: $password).textFieldStyle(.plain).lascoInput()
                if !completedSteps.isEmpty || currentStep != nil {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(completedSteps, id: \.self) { step in
                            Label(step, systemImage: "checkmark.circle.fill")
                                .font(LascoFont.body(13))
                                .foregroundStyle(theme.ok)
                        }
                        if let currentStep {
                            Text(currentStep)
                                .font(LascoFont.body(13))
                                .foregroundStyle(theme.inkSub)
                        }
                    }
                }
                if let error { Text(error).font(LascoFont.body(13)).foregroundStyle(theme.error) }
                Spacer()
                Button(submitting ? (currentStep ?? "Authenticating…") : "Authenticate") {
                    submitting = true
                    error = nil
                    completedSteps = []
                    currentStep = "Authenticating…"
                    Task {
                        do {
                            try await repository.authenticateLascoCloud(
                                email: email,
                                password: password,
                                libraryID: libraryID,
                                onProgress: updateConnectionProgress
                            )
                            currentStep = "Refreshing this library…"
                            try await onRemoteReady()
                            toastManager.show(ok: "Lasco Cloud: connected"); dismiss()
                        } catch {
                            self.error = error.localizedDescription
                            currentStep = nil
                        }
                        submitting = false
                    }
                }
                .buttonStyle(LascoPrimaryButtonStyle()).frame(maxWidth: .infinity)
                .disabled(email.isEmpty || password.isEmpty || submitting)
            }
            .padding(32)
        }
    }

    private func updateConnectionProgress(_ step: LibraryRepository.LascoCloudConnectionStep) {
        switch step {
        case .authenticated:
            completedSteps.append("Authentication successful")
            currentStep = "Checking Cloud storage…"
        case .remotesValidated:
            completedSteps.append("Cloud storage configuration verified")
            currentStep = "Configuring storage remotes…"
        case .remotesConfigured:
            completedSteps.append("Storage remotes configured")
        }
    }
}
