import SwiftUI
#if os(macOS)
import AppKit
#endif

struct AddLocalFSRemoteView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    @FocusState private var nameFieldFocused: Bool
    @State private var name = ""

    private var isValid: Bool { !name.isEmpty }

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

                Text("Add local FS remote")
                    .font(LascoFont.title(26))
                    .foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 8)

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        HStack(spacing: 8) {
                            Canvas { ctx, size in
                                for x in stride(from: CGFloat(0), to: size.width, by: 2) {
                                    for y in stride(from: CGFloat(0), to: size.height, by: 2) {
                                        ctx.fill(
                                            Path(CGRect(x: x, y: y, width: 1, height: 1)),
                                            with: .color(theme.warn)
                                        )
                                    }
                                }
                            }
                            .frame(width: 10, height: 10)

                            Text("Saves the data locally, use it only for test purposes!")
                                .font(LascoFont.body(13))
                                .foregroundStyle(theme.inkMuted)
                        }

                        VStack(alignment: .leading, spacing: 16) {
                            VStack(alignment: .leading, spacing: 6) {
                                FieldLabel(text: "Remote name")
                                TextField("local-test", text: $name)
                                    .textFieldStyle(.plain)
                                    .lascoInput()
                                    .autocorrectionDisabled()
                                    .focused($nameFieldFocused)
                                    #if os(iOS)
                                    .textInputAutocapitalization(.never)
                                    #endif
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
                    guard !name.isEmpty else { return }
                    let remoteName = name
                    if let remoteId = libraryModel.addRemoteDebugLocalApple(name: remoteName) {
                        dismiss()
                        Task {
                            if let err = await libraryModel.initializeRemote(remoteId: remoteId) {
                                toastManager.show(error: err)
                            } else {
                                toastManager.show(ok: "\(remoteName): initialized")
                            }
                        }
                    }
                }
                .buttonStyle(LascoDevButtonStyle())
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
}

#Preview {
    AddLocalFSRemoteView()
        .environmentObject(LibraryModel())
}
