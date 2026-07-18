import SwiftUI

struct MainView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme
    @State private var selectedTab: AppTab = .home
    @State private var hideTabBar = false
    @State private var albumToOpen: FfiAlbum? = nil

    var body: some View {
        ZStack(alignment: .bottom) {
            tabContent
                .safeAreaInset(edge: .bottom) {
                    Color.clear.frame(height: hideTabBar ? 0 : 88)
                }
                .onPreferenceChange(HideTabBarKey.self) { hideTabBar = $0 }

            if !hideTabBar {
                FloatingTabBar(selectedTab: $selectedTab)
                    .padding(.horizontal, 44)
                    .padding(.bottom, 24)
            }
        }
        .background(theme.bg)
        .task(id: libraryModel.isOpen) {
            guard libraryModel.isOpen else { return }
            await libraryModel.fetchDefaultRemote()
        }
    }

    @ViewBuilder
    private var tabContent: some View {
        switch selectedTab {
        case .home:
            ContentView(openAlbum: openAlbum)
        case .albums:
            AlbumsView(pendingAlbum: $albumToOpen)
        // case .search:
        //     SearchView()
        case .status:
            StatusView()
        case .manage:
            ManageView()
        }
    }

    private func openAlbum(_ album: FfiAlbum) {
        albumToOpen = album
        selectedTab = .albums
    }
}
