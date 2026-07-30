import SwiftUI

struct SearchView: View {
    @Environment(\.lascoTheme) var theme
    @State private var query = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("SEARCH")
                            .font(LascoFont.categoryLarge())
                            .foregroundStyle(theme.ink)
                        Text("Search your library")
                            .font(LascoFont.subtitle())
                            .foregroundStyle(theme.inkMuted)
                    }
                    .padding(.top, 20)

                    TextField("Search…", text: $query)
                        .textFieldStyle(.plain)
                        .lascoInput()

                    if query.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Start typing to search.")
                                .font(LascoFont.body())
                                .foregroundStyle(theme.inkMuted)
                        }
                        .padding(20)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .lascoPanel()
                    }

                    Spacer(minLength: 40)
                }
                .padding(.horizontal, 20)
            }
            .background(theme.bg)
            .scrollContentBackground(.hidden)
            .navigationTitle("")
            .hideSystemNavigationBar()
        }
        .background(theme.bg)
    }
}
