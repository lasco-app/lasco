import SwiftUI

struct LicenseView: View {
    @Environment(\.dismiss) var dismiss
    @Environment(\.lascoTheme) var theme

    var body: some View {
        NavigationStack {
            ZStack {
                theme.bg.ignoresSafeArea()

                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        Text("LICENSES")
                            .font(LascoFont.categoryLarge())
                            .foregroundStyle(theme.ink)
                            .padding(.top, 20)

                        VStack(alignment: .leading, spacing: 0) {
                            infoRow(title: "Lasco", value: "GNU GPLv3")

                            Divider()
                                .background(theme.inkMuted.opacity(0.2))

                            if let uiDependenciesURL {
                                licenseRow(title: "User Interface Dependencies") {
                                    ThirdPartyLicensesView(fileURL: uiDependenciesURL, title: "User Interface Dependencies")
                                        .environment(\.lascoTheme, theme)
                                        .navigationBarBackButtonHidden(true)
                                }
                            }

                            if let coreDependenciesURL {
                                Divider()
                                    .background(theme.inkMuted.opacity(0.2))

                                licenseRow(title: "Core Dependencies") {
                                    ThirdPartyLicensesView(fileURL: coreDependenciesURL, title: "Core Dependencies")
                                        .environment(\.lascoTheme, theme)
                                        .navigationBarBackButtonHidden(true)
                                }
                            }
                        }
                        .lascoPanel()

                        Spacer(minLength: 40)
                    }
                    .padding(.horizontal, 20)
                }
                .background(theme.bg)
                .scrollContentBackground(.hidden)
            }
            .toolbar {
                ToolbarItem(placement: .automatic) {
                    Button {
                        dismiss()
                    } label: {
                        Image("times")
                            .renderingMode(.template)
                            .resizable()
                            .frame(width: 16, height: 16)
                            .foregroundStyle(theme.ink)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    @ViewBuilder
    private func licenseRow<Destination: View>(title: String, @ViewBuilder destination: () -> Destination) -> some View {
        NavigationLink {
            destination()
        } label: {
            HStack {
                Text(title)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkSub)
                Spacer()
                Text("→")
                    .font(LascoFont.mono())
                    .foregroundStyle(theme.inkMuted)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func infoRow(title: String, value: String) -> some View {
        HStack {
            Text(title)
                .font(LascoFont.body())
                .foregroundStyle(theme.inkSub)
            Spacer()
            Text(value)
                .font(LascoFont.mono())
                .foregroundStyle(theme.inkMuted)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
    }

    private var coreDependenciesURL: URL? {
        Bundle.main.url(forResource: "third-party-licenses", withExtension: "html")
    }

    private var uiDependenciesURL: URL? {
        Bundle.main.url(forResource: "ui-dependencies", withExtension: "html")
    }
}

private struct ThirdPartyLicensesView: View {
    let fileURL: URL
    let title: String

    @Environment(\.dismiss) var dismiss
    @Environment(\.lascoTheme) var theme

    var body: some View {
        HTMLView(fileURL: fileURL)
            .navigationTitle(title)
            .toolbar {
                ToolbarItem(placement: .automatic) {
                    Button {
                        dismiss()
                    } label: {
                        Image("times")
                            .renderingMode(.template)
                            .resizable()
                            .frame(width: 16, height: 16)
                            .foregroundStyle(theme.ink)
                    }
                    .buttonStyle(.plain)
                }
            }
    }
}
