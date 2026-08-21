import SwiftUI

struct LascoCloudView: View {
    @Environment(\.lascoTheme) private var theme
    let libraryID: FfiLibraryId

    @State private var account: LascoCloudAccount?
    @State private var error: String?
    @State private var isLoading = true

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("LASCO CLOUD")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)

                    VStack(alignment: .leading, spacing: 8) {
                        if isLoading {
                            Text("Loading subscription…")
                                .foregroundStyle(theme.inkMuted)
                        } else if let error {
                            Text(error)
                                .foregroundStyle(theme.error)
                        } else if let account {
                            CloudInfoRow(label: "Email", value: account.email)
                            if let subscription = account.subscription {
                                CloudInfoRow(label: "Plan", value: subscription.planName)
                                CloudInfoRow(label: "Status", value: subscription.status.capitalized)
                                CloudInfoRow(label: "Storage", value: ByteCountFormatter.string(fromByteCount: subscription.storageQuotaBytes, countStyle: .decimal))
                                CloudInfoRow(label: "Renews", value: renewalDate(subscription.renewsAt))
                            } else {
                                CloudInfoRow(label: "Plan", value: "No active plan")
                            }
                        } else {
                            Text("No active Lasco Cloud plan.")
                                .foregroundStyle(theme.inkSub)
                        }
                    }
                    .font(LascoFont.body())
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .lascoPanel()
                }
                .padding(.horizontal, 20)
                .padding(.top, 20)
            }
            .scrollContentBackground(.hidden)
        }
        .navigationTitle("")
        .hideSystemNavigationBar()
        .task(id: libraryID) {
            await loadSubscription()
        }
    }

    private func loadSubscription() async {
        isLoading = true
        error = nil
        do {
            account = try await LascoCloudClient().subscription(libraryID: libraryID.value)
        } catch {
            self.error = error.localizedDescription
        }
        isLoading = false
    }

    private func renewalDate(_ value: String) -> String {
        guard let date = ISO8601DateFormatter().date(from: value) else { return value }
        return date.formatted(date: .abbreviated, time: .omitted)
    }
}

private struct CloudInfoRow: View {
    @Environment(\.lascoTheme) private var theme
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .foregroundStyle(theme.inkSub)
            Spacer()
            Text(value)
                .font(LascoFont.mono())
                .foregroundStyle(theme.ink)
        }
    }
}
