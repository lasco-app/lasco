import SwiftUI

struct RecentMediaScrollView<Header: View, Content: View>: View {
    @Environment(\.lascoTheme) private var theme
    @Binding private var scrollPosition: FfiMediaUuid?
    private let header: Header
    private let content: Content

    init(
        scrollPosition: Binding<FfiMediaUuid?>,
        @ViewBuilder header: () -> Header,
        @ViewBuilder content: () -> Content
    ) {
        _scrollPosition = scrollPosition
        self.header = header()
        self.content = content()
    }

    var body: some View {
        ScrollView {
            #if canImport(UIKit)
            LazyVStack(alignment: .leading, spacing: 24, pinnedViews: [.sectionHeaders]) {
                Section {
                    VStack(alignment: .leading, spacing: 24) {
                        content
                    }
                    .padding(.horizontal, 20)
                } header: {
                    header
                        .padding(.horizontal, 20)
                        .background(theme.bg)
                }
            }
            #else
            VStack(alignment: .leading, spacing: 24) {
                header
                    .padding(.horizontal, 20)
                content
                    .padding(.horizontal, 20)
            }
            #endif
        }
        .background(theme.bg)
        .scrollContentBackground(.hidden)
        .scrollPosition(id: $scrollPosition, anchor: .top)
    }
}
