import SwiftUI

struct RecentMediaScrollView<Header: View, Content: View>: View {
    @Environment(\.lascoTheme) private var theme
    @Binding private var scrollPosition: FfiMediaUuid?

    private let mode: RecentMediaMode
    private let header: Header
    private let content: Content

    init(
        mode: RecentMediaMode,
        scrollPosition: Binding<FfiMediaUuid?>,
        @ViewBuilder header: () -> Header,
        @ViewBuilder content: () -> Content
    ) {
        self.mode = mode
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
        .id(mode)
        .scrollPosition(id: $scrollPosition, anchor: .top)
        .background(theme.bg)
        .scrollContentBackground(.hidden)
    }
}
