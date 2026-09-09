import SwiftUI

enum ExistingLibrarySource: String, Identifiable {
    case lascoCloud
    case s3

    var id: Self { self }
}

struct AddExistingLibraryView: View {
    private enum FocusedField: Hashable {
        case nickname, username, password
        case newUsername, newPassword
        case cloudEmail, cloudPassword
        case remoteName, endpoint, bucket, region, pathPrefix, accessKey, secretKey
    }

    let source: ExistingLibrarySource

    @Environment(LibraryDirectoryModel.self) private var directory
    @Environment(\.dismiss) private var dismiss
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @State private var nickname = ""
    @State private var username = ""
    @State private var password = ""

    @State private var cloudEmail = ""
    @State private var cloudPassword = ""

    @State private var createNewUser = false
    @State private var newUsername = ""
    @State private var newPassword = ""

    @State private var remoteName = "my s3 remote"
    @State private var endpoint = ""
    @State private var bucket = ""
    @State private var region = ""
    @State private var pathPrefix = ""
    @State private var accessKey = ""
    @State private var secretKey = ""
    @State private var uploadAcknowledged = false

    @State private var isAdding = false
    @FocusState private var focusedField: FocusedField?

    enum TestState: Equatable {
        case idle
        case testing
        case success
        case failure(String)
    }
    @State private var testState: TestState = .idle

    private var canTest: Bool {
        !endpoint.isEmpty && !bucket.isEmpty && !accessKey.isEmpty && !secretKey.isEmpty
            && testState != .testing
    }

    private var isValid: Bool {
        let credentialsValid = !nickname.isEmpty && !username.isEmpty && !password.isEmpty
            && (!createNewUser || (!newUsername.isEmpty && !newPassword.isEmpty))
            && !isAdding
        switch source {
        case .lascoCloud:
            return credentialsValid && !cloudEmail.isEmpty && !cloudPassword.isEmpty
        case .s3:
            return credentialsValid
                && !remoteName.isEmpty
                && !endpoint.isEmpty && !bucket.isEmpty && !accessKey.isEmpty && !secretKey.isEmpty
                && uploadAcknowledged
        }
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            theme.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                HStack {
                    Spacer()
                    Button(action: { dismiss() }) {
                        Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(theme.ink)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 16)

                Text("Add an existing library")
                    .font(LascoFont.title(26))
                    .foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 8)

                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(alignment: .leading, spacing: 20) {
                        Text(source == .lascoCloud
                            ? "Sign in to Lasco Cloud to add the library associated with your account."
                            : "Point Lasco at an S3 remote that already holds a library. It downloads the library and syncs it to this device."
                        )
                            .font(LascoFont.body(16))
                            .foregroundStyle(theme.inkSub)
                            .fixedSize(horizontal: false, vertical: true)
                            .lineSpacing(4)

                        VStack(alignment: .leading, spacing: 16) {
                            inputField("Library name", placeholder: "my-library", binding: $nickname, identifier: "existing-library.name", focus: .nickname)

                            inputField("Username", placeholder: "an existing user", binding: $username, identifier: "existing-library.username", focus: .username)
                            secureInputField("Password", binding: $password, identifier: "existing-library.password", focus: .password)

                            Toggle(isOn: $createNewUser) {
                                Text("Create a new user on this device")
                                    .font(LascoFont.body(15))
                                    .foregroundStyle(theme.ink)
                            }
                            .tint(theme.ink)

                            if createNewUser {
                                inputField("New username", placeholder: "this device's user", binding: $newUsername, focus: .newUsername)
                                secureInputField("New password", binding: $newPassword, focus: .newPassword)
                                Text("The new user shares the library but signs in with its own password.")
                                    .font(LascoFont.body(13))
                                    .foregroundStyle(theme.inkMuted)
                                    .fixedSize(horizontal: false, vertical: true)
                                    .lineSpacing(3)
                            }

                            Divider().overlay(theme.inkMuted.opacity(0.3))

                            if source == .lascoCloud {
                                Text("Sign in to Lasco Cloud to find the library associated with your account.")
                                    .font(LascoFont.body(14))
                                    .foregroundStyle(theme.inkSub)
                                inputField(
                                    "Lasco Cloud email",
                                    placeholder: "you@example.com",
                                    binding: $cloudEmail,
                                    identifier: "existing-library.cloud-email",
                                    focus: .cloudEmail,
                                    onSubmit: { focusedField = .cloudPassword }
                                )
                                secureInputField("Lasco Cloud password", binding: $cloudPassword, identifier: "existing-library.cloud-password", focus: .cloudPassword)
                            } else {
                                inputField("Remote name", placeholder: "my s3 remote", binding: $remoteName, focus: .remoteName)
                                inputField("Endpoint URL", placeholder: "https://region1.example-s3-server.com", binding: $endpoint, focus: .endpoint)
                                inputField("Bucket", placeholder: "my-photos-bucket", binding: $bucket, focus: .bucket)
                                inputField("Region", placeholder: "region1", binding: $region, focus: .region)
                                inputField("Path prefix (optional)", placeholder: "photos/", binding: $pathPrefix, focus: .pathPrefix)
                                inputField("Access key", placeholder: "", binding: $accessKey, focus: .accessKey)
                                secureInputField("Secret key", binding: $secretKey, focus: .secretKey)

                                LascoCheckbox(
                                    isOn: $uploadAcknowledged,
                                    label: "I understand this app will upload my photos to the S3 bucket configured above."
                                )

                                Button(action: testConnection) {
                                    HStack(spacing: 8) {
                                        if testState == .testing {
                                            ProgressView().controlSize(.small)
                                        }
                                        Text(testState == .testing ? "Testing…" : "Test connection")
                                    }
                                }
                                .buttonStyle(.plain)
                                .foregroundStyle(canTest ? theme.ink : theme.inkMuted)
                                .disabled(!canTest)

                                switch testState {
                                case .success:
                                    Text("Connection succeeded.")
                                        .font(LascoFont.body(13))
                                        .foregroundStyle(theme.ok)
                                case .failure(let msg):
                                    Text(msg)
                                        .font(LascoFont.body(13))
                                        .foregroundStyle(theme.error)
                                        .fixedSize(horizontal: false, vertical: true)
                                case .idle, .testing:
                                    EmptyView()
                                }
                            }
                        }

                        Button {
                            addLibrary()
                        } label: {
                            HStack(spacing: 8) {
                                if isAdding {
                                    ProgressView().controlSize(.small)
                                }
                                Text(isAdding ? "Adding…" : "Add library")
                            }
                        }
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                        .disabled(!isValid)
                        .opacity(isValid ? 1 : 0.45)
                        .padding(.top, 8)

                        Spacer().frame(height: 48)
                        }
                        .padding(.horizontal, 32)
                        .padding(.top, 8)
                    }
                    .scrollDismissesKeyboard(.interactively)
                    .onChange(of: focusedField) { _, field in
                        guard let field else { return }
                        withAnimation {
                            proxy.scrollTo(field, anchor: .center)
                        }
                    }
                }
            }

        }
    }

    private func addLibrary() {
        isAdding = true
        Task {
            do {
                switch source {
                case .lascoCloud:
                    try await directory.addExistingLascoCloud(
                        nickname: nickname,
                        username: username,
                        password: password,
                        newUsername: createNewUser ? newUsername : nil,
                        newPassword: createNewUser ? newPassword : nil,
                        cloudEmail: cloudEmail,
                        cloudPassword: cloudPassword
                    )
                case .s3:
                    try await directory.addExisting(
                        nickname: nickname,
                        username: username,
                        password: password,
                        newUsername: createNewUser ? newUsername : nil,
                        newPassword: createNewUser ? newPassword : nil,
                        remoteID: remoteName,
                        endpoint: endpoint,
                        bucket: bucket,
                        region: region,
                        pathPrefix: pathPrefix,
                        accessKey: accessKey,
                        secretKey: secretKey
                    )
                }
                isAdding = false
                dismiss()
            } catch {
                isAdding = false
                toastManager.show(error: error.localizedDescription)
            }
        }
    }

    private func testConnection() {
        testState = .testing
        let endpoint = endpoint, bucket = bucket, region = region, pathPrefix = pathPrefix
        let accessKey = accessKey, secretKey = secretKey
        Task {
            let result: TestState
            do {
                try await directory.testS3Remote(endpoint: endpoint, bucket: bucket, region: region, pathPrefix: pathPrefix, accessKey: accessKey, secretKey: secretKey)
                result = .success
            } catch let e as LascoError {
                result = .failure(e.friendlyMessage)
            } catch {
                result = .failure(error.localizedDescription)
            }
            await MainActor.run { testState = result }
        }
    }

    private func inputField(
        _ label: String,
        placeholder: String,
        binding: Binding<String>,
        identifier: String? = nil,
        focus: FocusedField,
        onSubmit: (() -> Void)? = nil
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(text: label, size: 14)
            TextField(placeholder, text: binding)
                .textFieldStyle(.plain)
                .lascoInput()
                .accessibilityIdentifier(identifier ?? label)
                .focused($focusedField, equals: focus)
                .autocorrectionDisabled()
                .submitLabel(onSubmit == nil ? .done : .next)
                .onSubmit { onSubmit?() }
                #if os(iOS)
                .textInputAutocapitalization(.never)
                #endif
        }
        .id(focus)
    }

    private func secureInputField(
        _ label: String,
        binding: Binding<String>,
        identifier: String? = nil,
        focus: FocusedField
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(text: label, size: 14)
            SecureField("", text: binding)
                .textFieldStyle(.plain)
                .lascoInput()
                .accessibilityIdentifier(identifier ?? label)
                .focused($focusedField, equals: focus)
                .submitLabel(.done)
                .onSubmit {
                    focusedField = nil
                }
        }
        .id(focus)
    }
}

#Preview {
    AddExistingLibraryView(source: .s3)
}
