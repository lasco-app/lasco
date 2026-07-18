import SwiftUI

struct UsersView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(ToastManager.self) var toastManager
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme

    @State private var showAddUser = false

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                #if canImport(UIKit)
                LascoBackButton(action: { dismiss() })
                    .padding(.horizontal, 32)
                    .padding(.top, 20)
                #endif

                VStack(alignment: .leading, spacing: 4) {
                    Text("USERS")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                }
                .padding(.horizontal, 32)
                .padding(.top, 16)
                .padding(.bottom, 32)

                VStack(alignment: .leading, spacing: 12) {
                    if libraryModel.users.isEmpty {
                        Text("No users found.")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkMuted)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 20)
                            .lascoPanel()
                    } else {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(libraryModel.users.enumerated()), id: \.element) { index, username in
                                if index > 0 {
                                    Divider()
                                        .background(theme.inkMuted.opacity(0.2))
                                }
                                UserRow(
                                    username: username,
                                    isMe: username == libraryModel.openUsername
                                )
                            }
                        }
                        .lascoPanel()
                    }

                    Button("Add user") { showAddUser = true }
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                }
                .padding(.horizontal, 32)

                Spacer()
            }
        }
        .navigationBarBackButtonHidden(true)
        .navigationTitle("")
        .hideSystemNavigationBar()
        .toolbarBackButton(action: { dismiss() })
        .sheet(isPresented: $showAddUser) {
            AddUserView()
                .environmentObject(libraryModel)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
    }
}

private struct UserRow: View {
    let username: String
    let isMe: Bool
    @Environment(\.lascoTheme) var theme

    var body: some View {
        HStack(spacing: 8) {
            Text(username)
                .font(LascoFont.body())
                .foregroundStyle(theme.ink)
            Spacer()
            if isMe {
                Text("you")
                    .font(LascoFont.pixel())
                    .foregroundStyle(.white)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(theme.accent.opacity(0.12))
                    .clipShape(RoundedRectangle(cornerRadius: 4))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
    }
}

private struct AddUserView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    @Environment(ToastManager.self) var toastManager

    @State private var username = ""
    @State private var password = ""
    @State private var passwordConfirm = ""
    @State private var errorMessage: String?

    private var canSubmit: Bool {
        !username.trimmingCharacters(in: .whitespaces).isEmpty &&
        !password.isEmpty &&
        password == passwordConfirm
    }

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text("Add user")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                    Spacer()
                    Button(action: { dismiss() }) {
                        Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .foregroundStyle(theme.inkMuted)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 8)

                Text("New user will be able to open this library.")
                    .font(LascoFont.subtitle())
                    .foregroundStyle(theme.inkMuted)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 32)

                VStack(spacing: 16) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Username")
                            .font(LascoFont.pixel())
                            .foregroundStyle(theme.inkMuted)
                        TextField("username", text: $username)
                            .font(LascoFont.body())
                            .foregroundStyle(theme.ink)
                            .autocorrectionDisabled()
                            #if os(iOS)
                            .textInputAutocapitalization(.never)
                            #endif
                            .padding(.horizontal, 12)
                            .padding(.vertical, 10)
                            .background(theme.inkMuted.opacity(0.08))
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("Password")
                            .font(LascoFont.pixel())
                            .foregroundStyle(theme.inkMuted)
                        SecureField("password", text: $password)
                            .font(LascoFont.body())
                            .foregroundStyle(theme.ink)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 10)
                            .background(theme.inkMuted.opacity(0.08))
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("Confirm password")
                            .font(LascoFont.pixel())
                            .foregroundStyle(theme.inkMuted)
                        SecureField("confirm password", text: $passwordConfirm)
                            .font(LascoFont.body())
                            .foregroundStyle(theme.ink)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 10)
                            .background(theme.inkMuted.opacity(0.08))
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                    }

                    if let err = errorMessage {
                        Text(err)
                            .font(LascoFont.pixel())
                            .foregroundStyle(Color.red)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }

                    Button("Add user") {
                        let trimmed = username.trimmingCharacters(in: .whitespaces)
                        if let err = libraryModel.addUser(username: trimmed, password: password) {
                            errorMessage = err
                        } else {
                            toastManager.show(ok: "User \"\(trimmed)\" added")
                            dismiss()
                        }
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                    .disabled(!canSubmit)
                    .opacity(canSubmit ? 1 : 0.4)
                }
                .padding(.horizontal, 32)

                Spacer()
            }
        }
    }
}
