import SwiftUI

/// Adds an SMB 2/3 share after explicitly testing the same credentials and path.
struct AddSmbRemoteView: View {
    @Environment(LibraryRepository.self) private var repository
    @Environment(LibraryDirectoryModel.self) private var directory
    @Environment(ToastManager.self) private var toastManager
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) private var theme

    let onRemoteReady: @MainActor () async throws -> Void
    @State private var name = ""
    @State private var server = ""
    @State private var port = "445"
    @State private var share = ""
    @State private var pathPrefix = ""
    @State private var username = ""
    @State private var password = ""
    @State private var domain = ""
    @State private var uploadAcknowledged = false
    @State private var isTesting = false
    @State private var isAdding = false
    @State private var message: String?
    @State private var messageIsError = false

    init(onRemoteReady: @escaping @MainActor () async throws -> Void = {}) {
        self.onRemoteReady = onRemoteReady
    }

    private var parsedPort: UInt16? { UInt16(port) }
    private var canTest: Bool {
        !server.trimmingCharacters(in: .whitespaces).isEmpty && parsedPort != nil && !share.isEmpty
            && !username.isEmpty && !password.isEmpty && !isTesting
    }
    private var canAdd: Bool { !name.isEmpty && canTest && uploadAcknowledged && !isAdding }

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                HStack {
                    Spacer()
                    Button(action: { dismiss() }) {
                        Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .foregroundStyle(theme.ink)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Close SMB remote setup")
                }
                .padding(.horizontal, 32).padding(.top, 32).padding(.bottom, 16)
                Text("Add an SMB remote")
                    .font(LascoFont.title(26)).foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 32).padding(.bottom, 8)
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("Connect to an SMB 2 or SMB 3 shared folder on your NAS, server, or local network. For \\nas.local\\photos, enter photos as the shared folder name, then optionally choose a folder within it.")
                            .font(LascoFont.body(16)).foregroundStyle(theme.inkSub)
                        field("Remote name", placeholder: "home-nas", value: $name, identifier: "smb-remote.name")
                        field("Server address", placeholder: "nas.local or 192.168.1.20", value: $server, identifier: "smb-remote.server")
                        field("Port", placeholder: "445", value: $port, identifier: "smb-remote.port")
                        field("Shared folder name", placeholder: "photos", value: $share, identifier: "smb-remote.share")
                        field("Folders within shared folder (optional)", placeholder: "lasco", value: $pathPrefix, identifier: "smb-remote.path")
                        field("Username", placeholder: "lasco", value: $username, identifier: "smb-remote.username")
                        field("Domain or workgroup (optional)", placeholder: "WORKGROUP", value: $domain, identifier: "smb-remote.domain")
                        secureField("Password", value: $password)
                        Text("The password is stored locally, encrypted with the library password. It is never shown again.")
                            .font(LascoFont.body(13)).foregroundStyle(theme.inkMuted)
                        LascoCheckbox(isOn: $uploadAcknowledged, label: "I understand this app will upload my photos to the SMB share configured above.")
                        Button(action: testConnection) {
                            HStack(spacing: 8) {
                                if isTesting { ProgressView().controlSize(.small) }
                                Text(isTesting ? "Testing…" : "Test connection")
                            }
                        }
                        .buttonStyle(.plain).foregroundStyle(canTest ? theme.ink : theme.inkMuted).disabled(!canTest)
                        if let message {
                            Text(message).font(LascoFont.body(13))
                                .foregroundStyle(messageIsError ? theme.error : theme.ok)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        Button(isAdding ? "Adding…" : "Add Remote", action: addRemote)
                            .buttonStyle(LascoPrimaryButtonStyle()).frame(maxWidth: .infinity)
                            .disabled(!canAdd).opacity(canAdd ? 1 : 0.45)
                    }
                    .padding(.horizontal, 32).padding(.top, 8).padding(.bottom, 32)
                }
            }
        }
    }

    private func field(_ label: String, placeholder: String, value: Binding<String>, identifier: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(text: label, size: 14)
            TextField(placeholder, text: value).textFieldStyle(.plain).lascoInput()
                .accessibilityIdentifier(identifier).autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.never)
                #endif
        }
    }

    private func secureField(_ label: String, value: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(text: label, size: 14)
            SecureField("", text: value).textFieldStyle(.plain).lascoInput()
                .accessibilityIdentifier("smb-remote.password")
        }
    }

    private func testConnection() {
        guard let port = parsedPort else { return }
        isTesting = true; message = nil
        let values = (server, port, share, pathPrefix, username, password, domain)
        Task {
            do {
                try await directory.testSmbRemote(server: values.0, port: values.1, share: values.2, pathPrefix: values.3, username: values.4, password: values.5, domain: values.6.isEmpty ? nil : values.6)
                message = "Connection succeeded."; messageIsError = false
            } catch {
                message = error.localizedDescription; messageIsError = true
            }
            isTesting = false
        }
    }

    private func addRemote() {
        guard let port = parsedPort, !isAdding else { return }
        isAdding = true; message = nil
        Task {
            var added: FfiRemoteUuid?
            do {
                let id = try await repository.addRemoteSmb(id: name, server: server, port: port, share: share, pathPrefix: pathPrefix, username: username, password: password, domain: domain.isEmpty ? nil : domain)
                added = id
                try await repository.initializeRemote(id: id)
                try await onRemoteReady()
                dismiss(); toastManager.show(ok: "\(name): initialized")
            } catch {
                if let added { try? await repository.removeRemote(id: added) }
                message = error.localizedDescription; messageIsError = true
            }
            isAdding = false
        }
    }
}
