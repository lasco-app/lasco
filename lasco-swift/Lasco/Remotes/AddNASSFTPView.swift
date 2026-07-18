import SwiftUI

struct AddNASSFTPView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme

    @FocusState private var nameFieldFocused: Bool
    @State private var name = ""
    @State private var host = ""
    @State private var port = "22"
    @State private var username = ""
    @State private var password = ""
    @State private var path = ""

    var body: some View {
        ZStack(alignment: .bottom) {
            theme.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                HStack {
                    Button("Cancel") { dismiss() }
                        .font(LascoFont.body(14))
                        .foregroundStyle(theme.inkMuted)
                        .buttonStyle(.plain)
                    Spacer()
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 16)

                Text("LASCO")
                    .font(LascoFont.categoryLarge(28))
                    .foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 8)

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text("Add NAS / SFTP remote.")
                            .font(LascoFont.title(26))
                            .foregroundStyle(theme.ink)
                            .fixedSize(horizontal: false, vertical: true)

                        Text("Connect to a NAS or any server accessible via SFTP.")
                            .font(LascoFont.body(16))
                            .foregroundStyle(theme.inkSub)
                            .fixedSize(horizontal: false, vertical: true)
                            .lineSpacing(4)

                        VStack(alignment: .leading, spacing: 16) {
                            VStack(alignment: .leading, spacing: 6) {
                                FieldLabel(text: "Remote name")
                                TextField("my-nas", text: $name)
                                    .textFieldStyle(.plain)
                                    .lascoInput()
                                    .autocorrectionDisabled()
                                    .focused($nameFieldFocused)
                                    #if os(iOS)
                                    .textInputAutocapitalization(.never)
                                    #endif
                            }
                            inputField("Host", placeholder: "192.168.1.10 or nas.local", binding: $host)
                            inputField("Port", placeholder: "22", binding: $port)
                                #if os(iOS)
                                .keyboardType(.numberPad)
                                #endif
                            inputField("Username", placeholder: "", binding: $username)
                            secureInputField("Password", binding: $password)
                            inputField("Remote path", placeholder: "/photos", binding: $path)
                        }

                        Spacer().frame(height: 100)
                    }
                    .padding(.horizontal, 32)
                    .padding(.top, 40)
                }
            }

            VStack(spacing: 0) {
                Button("Add Remote") {
                    // TODO: wire up FFI addRemote once available
                    dismiss()
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
        !name.isEmpty && !host.isEmpty && !port.isEmpty && !username.isEmpty && !password.isEmpty
    }

    private func inputField(_ label: String, placeholder: String, binding: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FieldLabel(text: label)
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
            FieldLabel(text: label)
            SecureField("", text: binding)
                .textFieldStyle(.plain)
                .lascoInput()
        }
    }
}

#Preview {
    AddNASSFTPView()
        .environmentObject(LibraryModel())
}
