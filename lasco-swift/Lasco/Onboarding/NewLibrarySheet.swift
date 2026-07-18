import SwiftUI

struct LibraryCreateForm: View {
    @Binding var name: String
    @Binding var username: String
    @Binding var password: String
    @Binding var confirmPassword: String
    var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Library name", size: 13)
                TextField("My Photos", text: $name)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .autocorrectionDisabled()
            }

            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Username", size: 13)
                TextField("", text: $username)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
            }

            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Password", size: 13)
                SecureField("", text: $password)
                    .textFieldStyle(.plain)
                    .lascoInput()
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

