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
                if let error { Text(error).font(LascoFont.body(13)).foregroundStyle(theme.error) }
                if submitting {
                    ProgressView()
                        .tint(theme.ink)
                        .frame(maxWidth: .infinity)
                        .accessibilityLabel("Authenticating")
                }
                Spacer()
                Button("Authenticate", action: authenticate)
                .buttonStyle(LascoPrimaryButtonStyle()).frame(maxWidth: .infinity)
                .disabled(email.isEmpty || password.isEmpty || submitting)
            }
            .padding(32)
        }
    }

    private func authenticate() {
        submitting = true
        error = nil
        Task {
            do {
                try await repository.authenticateLascoCloud(
                    email: email,
                    password: password,
                    libraryID: libraryID
                )
                try await onRemoteReady()
                toastManager.show(ok: "Lasco Cloud: connected"); dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            submitting = false
        }
    }
}
