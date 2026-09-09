import SwiftUI

struct LibraryCreateForm: View {
    private enum Field: Hashable {
        case name, username, password, confirmPassword
    }

    @Binding var name: String
    @Binding var username: String
    @Binding var password: String
    @Binding var confirmPassword: String
    var error: String?
    @FocusState private var focusedField: Field?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Library name", size: 13)
                TextField("My Photos", text: $name)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .accessibilityIdentifier("new-library.name")
                    .autocorrectionDisabled()
                    .focused($focusedField, equals: .name)
                    .submitLabel(.next)
                    .onSubmit { focusedField = .username }
            }

            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Username", size: 13)
                TextField("", text: $username)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .accessibilityIdentifier("new-library.username")
                    .autocorrectionDisabled()
                    .focused($focusedField, equals: .username)
                    .submitLabel(.next)
                    .onSubmit { focusedField = .password }
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
            }

            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Password", size: 13)
                SecureField("", text: $password)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .accessibilityIdentifier("new-library.password")
                    .focused($focusedField, equals: .password)
                    .submitLabel(.next)
                    .onSubmit { focusedField = .confirmPassword }
                if !password.isEmpty && password.count < 5 {
                    Text("Password must be at least 5 characters.")
                        .font(LascoFont.body(14))
                        .foregroundStyle(Color.Lasco.ink)
                }
            }

            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Confirm password", size: 13)
                SecureField("", text: $confirmPassword)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .accessibilityIdentifier("new-library.confirm-password")
                    .focused($focusedField, equals: .confirmPassword)
                    .submitLabel(.done)
                    .onSubmit { focusedField = nil }
                if !confirmPassword.isEmpty && confirmPassword != password {
                    Text("Passwords do not match.")
                        .font(LascoFont.body(14))
                        .foregroundStyle(Color.Lasco.ink)
                }
            }

            if let err = error {
                Text(err)
                    .font(LascoFont.body(13))
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

}
