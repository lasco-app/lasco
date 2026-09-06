import SwiftUI

struct MainView: View {
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme
    @Environment(\.scenePhase) private var scenePhase
    @State private var selectedTab: AppTab = .home
    @State private var hideTabBar = false
    @State private var albumToOpen: FfiAlbum? = nil
    @State private var albumsPath: [AlbumsDestination] = []
    @State private var didRestoreAlbumPath = false
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
        _uiState = State(initialValue: restoredState)
    }

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
            await restoreAlbumPath()
            await syncCoordinator.fetchDefaultRemote()
        }
        .onChange(of: albumsPath) { _, path in
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
        .onChange(of: selectedTab) {
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

    @ViewBuilder
    private var tabContent: some View {
        switch selectedTab {
        case .home:
            ContentView(
                repository: repository,
                session: session,
                importCoordinator: importCoordinator,
                model: recentMediaModel,
                allMediaPosition: $uiState.allMediaPosition,
                orphanMediaPosition: $uiState.orphanMediaPosition,
                openAlbum: openAlbum
            )
        case .albums:
            AlbumsView(
                repository: repository,
                session: session,
                importCoordinator: importCoordinator,
                model: albumListModel,
                path: $albumsPath,
                pendingAlbum: $albumToOpen
            )
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

    private func restoreAlbumPath() async {
        guard !didRestoreAlbumPath else { return }
        didRestoreAlbumPath = true
        guard !uiState.albumPathIDs.isEmpty else { return }

        let ids = Set(uiState.albumPathIDs.map(FfiAlbumUuid.init(value:)))
        guard let albums = try? await repository.albums(withIDs: ids),
              !Task.isCancelled else {
            return
        }
        let restoredAlbums = AlbumNavigationRestoration.restoredAlbums(
            savedIDs: uiState.albumPathIDs,
            albums: albums
        )
        albumsPath = restoredAlbums.map { .album($0) }
    }

    private func saveUIState() {
        uiState.showingOrphans = recentMediaModel.showingOrphans
        uiState.save(libraryID: session.libraryID)
    }
}
