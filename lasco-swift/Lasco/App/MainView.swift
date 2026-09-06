import SwiftUI

struct MainView: View {
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme
    @Environment(\.scenePhase) private var scenePhase
    @State private var selectedTab: AppTab = .home
    @State private var homePath: [LibraryDestination] = []
    @State private var hideManageTabBar = false
    @State private var albumNavigation: AlbumNavigationModel
    @State private var recentMediaModel: RecentMediaModel
    @State private var albumListModel: AlbumListModel
    @State private var uiState: LibraryUIState

    let repository: LibraryRepository
    let session: LibrarySessionState
    let syncCoordinator: SyncCoordinator
    let importCoordinator: MediaImportCoordinator
    let releasePolicy: ClientReleasePolicy

    init(
        repository: LibraryRepository,
        session: LibrarySessionState,
        syncCoordinator: SyncCoordinator,
        importCoordinator: MediaImportCoordinator,
        releasePolicy: ClientReleasePolicy
    ) {
        self.repository = repository
        self.session = session
        self.syncCoordinator = syncCoordinator
        self.importCoordinator = importCoordinator
        self.releasePolicy = releasePolicy

        let restoredState = LibraryUIState.load(libraryID: session.libraryID)
        let recentMediaModel = RecentMediaModel(repository: repository)
        recentMediaModel.showingOrphans = restoredState.showingOrphans
        _recentMediaModel = State(initialValue: recentMediaModel)
        _albumListModel = State(initialValue: AlbumListModel(repository: repository))
        _albumNavigation = State(initialValue: AlbumNavigationModel { ids in
            try await repository.albums(withIDs: ids)
        })
        _uiState = State(initialValue: restoredState)
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            tabContent
                .safeAreaInset(edge: .bottom) {
                    Color.clear.frame(height: hidesTabBar ? 0 : 88)
                }
                .onPreferenceChange(ManageTabBarHiddenKey.self) { isHidden in
                    guard selectedTab == .manage, hideManageTabBar != isHidden else { return }
                    hideManageTabBar = isHidden
                }

            if !hidesTabBar {
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
            await albumNavigation.restoreIfNeeded(savedIDs: uiState.albumPathIDs)
            await syncCoordinator.fetchDefaultRemote()
        }
        .onChange(of: albumNavigation.path) { _, path in
            let albumPathIDs: [String] = path.compactMap { destination in
                guard case .album(let album) = destination else { return nil }
                return album.albumId.value
            }
            guard albumPathIDs != uiState.albumPathIDs else { return }
            uiState.albumPathIDs = albumPathIDs
            saveUIState()
        }
        .onChange(of: recentMediaModel.showingOrphans) { _, showingOrphans in
            uiState.showingOrphans = showingOrphans
            saveUIState()
        }
        .onChange(of: selectedTab) { _, tab in
            if tab != .manage {
                hideManageTabBar = false
            }
            saveUIState()
        }
        .onChange(of: scenePhase) { _, phase in
            guard phase != .active else { return }
            saveUIState()
        }
        .onDisappear {
            saveUIState()
        }
    }

    private var tabContent: some View {
        TabView(selection: $selectedTab) {
            ContentView(
                repository: repository,
                session: session,
                importCoordinator: importCoordinator,
                model: recentMediaModel,
                allMediaPosition: $uiState.allMediaPosition,
                orphanMediaPosition: $uiState.orphanMediaPosition,
                path: $homePath,
                openAlbum: openAlbum
            )
            .tag(AppTab.home)
            .toolbar(.hidden, for: .tabBar)

            AlbumsView(
                repository: repository,
                session: session,
                importCoordinator: importCoordinator,
                model: albumListModel,
                navigation: albumNavigation
            )
            .tag(AppTab.albums)
            .toolbar(.hidden, for: .tabBar)

            StatusView(repository: repository, session: session, syncCoordinator: syncCoordinator)
                .tag(AppTab.status)
                .toolbar(.hidden, for: .tabBar)

            ManageView(repository: repository, session: session, syncCoordinator: syncCoordinator)
                .tag(AppTab.manage)
                .toolbar(.hidden, for: .tabBar)
        }
    }

    private func openAlbum(_ album: FfiAlbum) {
        albumNavigation.open(album)
        selectedTab = .albums
    }

    private var hidesTabBar: Bool {
        switch selectedTab {
        case .home:
            homePath.contains { if case .mediaDetail = $0 { return true } else { return false } }
        case .albums:
            albumNavigation.path.contains { if case .mediaDetail = $0 { return true } else { return false } }
        case .manage:
            hideManageTabBar
        case .status:
            false
        }
    }

    private func saveUIState() {
        uiState.showingOrphans = recentMediaModel.showingOrphans
        uiState.save(libraryID: session.libraryID)
    }
}
