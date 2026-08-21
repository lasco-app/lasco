#if DEBUG
import SwiftUI

struct DevelopmentCloudEndpointView: View {
    @Environment(\.lascoTheme) private var theme
    @Binding var isPresented: Bool
    @State private var endpoint = DevelopmentCloudEndpoint.defaultURL

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()
            VStack(alignment: .leading, spacing: 16) {
                Text("Development server")
                    .font(LascoFont.title(26))
                    .foregroundStyle(theme.ink)
                Text("Enter the Lasco Cloud address for this development build.")
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkSub)
                FieldLabel(text: "Address and port")
                TextField(DevelopmentCloudEndpoint.defaultURL, text: $endpoint)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                Spacer()
                Button("Use address", action: save)
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                    .disabled(endpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding(32)
        }
        .interactiveDismissDisabled()
    }

    private func save() {
        DevelopmentCloudEndpoint.setURL(endpoint)
        isPresented = false
    }
}
#endif
