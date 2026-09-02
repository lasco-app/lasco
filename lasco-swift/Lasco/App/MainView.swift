import SwiftUI

struct MainView: View {
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme
    @State private var selectedTab: AppTab = .home
    @State private var hideTabBar = false
    @State private var albumToOpen: FfiAlbum? = nil

    let repository: LibraryRepository
    let session: LibrarySessionState
    let syncCoordinator: SyncCoordinator
    let importCoordinator: MediaImportCoordinator
    let releasePolicy: ClientReleasePolicy

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
        .safeAreaInset(edge: .top) {
            if (selectedTab == .home || selectedTab == .albums),
               let decision = releasePolicy.decision,
               decision.updateAvailable,
               let url = URL(string: decision.storeURL) {
                Link(destination: url) {
                    HStack { Text(decision.message); Spacer(); Text("Update") }
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(theme.ink)
                        .padding(.horizontal, 16).padding(.vertical, 10)
                        .background(theme.pink)
                }
            }
        }
        .task {
            await syncCoordinator.fetchDefaultRemote()
        }
    }

    @ViewBuilder
    private var tabContent: some View {
        switch selectedTab {
        case .home:
            ContentView(repository: repository, session: session, importCoordinator: importCoordinator, openAlbum: openAlbum)
        case .albums:
            AlbumsView(repository: repository, session: session, importCoordinator: importCoordinator, pendingAlbum: $albumToOpen)
        // case .search:
        //     SearchView()
        case .status:
            StatusView(repository: repository, session: session, syncCoordinator: syncCoordinator)
        case .manage:
            ManageView(repository: repository, session: session, syncCoordinator: syncCoordinator)
        }
    }

    private func openAlbum(_ album: FfiAlbum) {
        albumToOpen = album
        selectedTab = .albums
    }
}
