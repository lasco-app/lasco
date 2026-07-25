import SwiftUI

struct AddS3RemoteView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @FocusState private var nameFieldFocused: Bool
    @State private var name = ""
    @State private var endpoint = ""
    @State private var bucket = ""
    @State private var region = ""
    @State private var pathPrefix = ""
    @State private var accessKey = ""
    @State private var secretKey = ""
    @State private var uploadAcknowledged = false

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

                Text("Add a S3 remote")
                    .font(LascoFont.title(26))
                    .foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 8)

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text("Works with any S3-compatible service.")
                            .font(LascoFont.body(16))
                            .foregroundStyle(theme.inkSub)
                            .fixedSize(horizontal: false, vertical: true)
                            .lineSpacing(4)

                        VStack(alignment: .leading, spacing: 16) {
                            VStack(alignment: .leading, spacing: 6) {
                                FieldLabel(text: "Remote name", size: 14)
                                TextField("my-backups", text: $name)
                                    .textFieldStyle(.plain)
                                    .lascoInput()
                                    .autocorrectionDisabled()
                                    .focused($nameFieldFocused)
                                    #if os(iOS)
                                    .textInputAutocapitalization(.never)
                                    #endif
                            }
                            inputField("Endpoint URL", placeholder: "https://region1.example-s3-server.com", binding: $endpoint)
                            inputField("Bucket", placeholder: "my-photos-bucket", binding: $bucket)
                            inputField("Region", placeholder: "region1", binding: $region)
                            inputField("Path prefix (optional)", placeholder: "photos/", binding: $pathPrefix)
                            inputField("Access key", placeholder: "", binding: $accessKey)
                            secureInputField("Secret key", binding: $secretKey)

                            Text("The secret key is stored locally and encrypted with the library password.")
                                .font(LascoFont.body(13))
                                .foregroundStyle(theme.inkMuted)
                                .fixedSize(horizontal: false, vertical: true)
                                .lineSpacing(3)

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
                Button("Add Remote") {
                    if let remoteId = libraryModel.addRemoteS3(
                        id: name,
                        endpoint: endpoint,
                        bucket: bucket,
                        region: region,
                        pathPrefix: pathPrefix,
                        accessKey: accessKey,
                        secretKey: secretKey
                    ) {
                        dismiss()
                        Task {
                            if let err = await libraryModel.initializeRemote(remoteId: remoteId) {
                                toastManager.show(error: err)
                            } else if let err = await libraryModel.pushRemote(remoteId: remoteId) {
                                toastManager.show(error: err)
                            } else {
                                toastManager.show(ok: "\(name): initialized")
                            }
                        }
                    } else if let err = libraryModel.error {
                        libraryModel.error = nil
                        toastManager.show(error: err)
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
        .onAppear { nameFieldFocused = true }
    }

    private var isValid: Bool {
        !name.isEmpty && !endpoint.isEmpty && !bucket.isEmpty && !accessKey.isEmpty && !secretKey.isEmpty
            && uploadAcknowledged
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
    AddS3RemoteView()
        .environmentObject(LibraryModel())
}
