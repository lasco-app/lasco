#if DEBUG
import SwiftUI

struct DevelopmentCloudEndpointView: View {
    private static let endpointInputPrefix = "https://"
    private static let lascoCloudURL = "https://cloud.getlasco.app"

    @Environment(\.lascoTheme) private var theme
    @Binding var isPresented: Bool
    @State private var endpoint = "https://"

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()
            VStack(alignment: .leading, spacing: 16) {
                Text("Development server")
                    .font(LascoFont.title(26))
                    .foregroundStyle(theme.ink)
                FieldLabel(text: "Address and port")
                TextField(Self.endpointInputPrefix, text: $endpoint)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                Spacer()
                Button("Use Lasco Cloud", action: useLascoCloud)
                    .buttonStyle(LascoSecondaryButtonStyle())
                    .frame(maxWidth: .infinity)
                Button("Use address", action: save)
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                    .disabled(isEndpointIncomplete)
            }
            .padding(32)
        }
        .interactiveDismissDisabled()
    }

    private func save() {
        DevelopmentCloudEndpoint.setURL(endpoint)
        isPresented = false
    }

    private func useLascoCloud() {
        endpoint = Self.lascoCloudURL
    }

    private var isEndpointIncomplete: Bool {
        let trimmed = endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty || trimmed == Self.endpointInputPrefix
    }
}
#endif
