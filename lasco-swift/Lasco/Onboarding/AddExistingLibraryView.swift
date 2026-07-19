import SwiftUI

struct AddExistingLibraryView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @State private var nickname = ""
    @State private var username = ""
    @State private var password = ""

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
        !nickname.isEmpty && !username.isEmpty && !password.isEmpty
            && !remoteName.isEmpty
            && !endpoint.isEmpty && !bucket.isEmpty && !accessKey.isEmpty && !secretKey.isEmpty
            && (!createNewUser || (!newUsername.isEmpty && !newPassword.isEmpty))
            && uploadAcknowledged
            && !isAdding
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

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text("Point Lasco at an S3 remote that already holds a library. It downloads the library and syncs it to this device.")
                            .font(LascoFont.body(16))
                            .foregroundStyle(theme.inkSub)
                            .fixedSize(horizontal: false, vertical: true)
                            .lineSpacing(4)

                        VStack(alignment: .leading, spacing: 16) {
                            inputField("Library name", placeholder: "my-library", binding: $nickname)

                            inputField("Username", placeholder: "an existing user", binding: $username)
                            secureInputField("Password", binding: $password)

                            Toggle(isOn: $createNewUser) {
                                Text("Create a new user on this device")
                                    .font(LascoFont.body(15))
                                    .foregroundStyle(theme.ink)
                            }
                            .tint(theme.ink)

                            if createNewUser {
                                inputField("New username", placeholder: "this device's user", binding: $newUsername)
                                secureInputField("New password", binding: $newPassword)
                                Text("The new user shares the library but signs in with its own password.")
                                    .font(LascoFont.body(13))
                                    .foregroundStyle(theme.inkMuted)
                                    .fixedSize(horizontal: false, vertical: true)
                                    .lineSpacing(3)
                            }

                            Divider().overlay(theme.inkMuted.opacity(0.3))

                            inputField("Remote name", placeholder: "my s3 remote", binding: $remoteName)
                            inputField("Endpoint URL", placeholder: "https://region1.example-s3-server.com", binding: $endpoint)
                            inputField("Bucket", placeholder: "my-photos-bucket", binding: $bucket)
                            inputField("Region", placeholder: "region1", binding: $region)
                            inputField("Path prefix (optional)", placeholder: "photos/", binding: $pathPrefix)
                            inputField("Access key", placeholder: "", binding: $accessKey)
                            secureInputField("Secret key", binding: $secretKey)

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

                        Spacer().frame(height: 100)
                    }
                    .padding(.horizontal, 32)
                    .padding(.top, 8)
                }
            }

            VStack(spacing: 0) {
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
            }
            .padding(.horizontal, 32)
            .padding(.top, 20)
            .padding(.bottom, 48)
            .background(
                LinearGradient(
                    colors: [theme.bg.opacity(0), theme.bg],
                    startPoint: .top, endPoint: .bottom
                )
            )
        }
    }

    private func addLibrary() {
        isAdding = true
        let ok = libraryModel.addExisting(
            nickname: nickname,
            username: username,
            password: password,
            newUsername: createNewUser ? newUsername : nil,
            newPassword: createNewUser ? newPassword : nil,
            remoteId: remoteName,
            endpoint: endpoint,
            bucket: bucket,
            region: region,
            pathPrefix: pathPrefix,
            accessKey: accessKey,
            secretKey: secretKey
        )
        isAdding = false
        if ok {
            dismiss()
        } else if let err = libraryModel.error {
            libraryModel.error = nil
            toastManager.show(error: err)
        }
    }

    private func testConnection() {
        testState = .testing
        let endpoint = endpoint, bucket = bucket, region = region, pathPrefix = pathPrefix
        let accessKey = accessKey, secretKey = secretKey
        Task.detached {
            let result: TestState
            do {
                try ffiTestS3Remote(
                    endpoint: endpoint,
                    bucket: bucket,
                    region: region,
                    pathPrefix: pathPrefix,
                    accessKey: accessKey,
                    secretKey: secretKey
                )
                result = .success
            } catch let e as LascoError {
                result = .failure(e.friendlyMessage)
            } catch {
                result = .failure(error.localizedDescription)
            }
            await MainActor.run { testState = result }
        }
    }

    private func inputField(_ label: String, placeholder: String, binding: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(text: label, size: 14)
            TextField(placeholder, text: binding)
                .textFieldStyle(.plain)
                .lascoInput()
                .autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.never)
                #endif
        }
    }

    private func secureInputField(_ label: String, binding: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(text: label, size: 14)
            SecureField("", text: binding)
                .textFieldStyle(.plain)
                .lascoInput()
        }
    }
}

#Preview {
    AddExistingLibraryView()
        .environmentObject(LibraryModel())
}
