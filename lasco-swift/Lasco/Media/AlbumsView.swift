import SwiftUI
import UniformTypeIdentifiers
import PhotosUI

extension FfiAlbum: Identifiable {
    public var id: FfiAlbumUuid { albumId }
}

// MARK: - Supporting types

enum ContentSelection {
    case none
    case items(mediaIds: Set<FfiMediaUuid>, groupIds: Set<FfiGroupUuid>)
    case albums(Set<FfiAlbumUuid>)

    var isSelecting: Bool {
        switch self {
        case .none: return false
        default: return true
        }
    }

    var count: Int {
        switch self {
        case .none: return 0
        case .items(let m, let g): return m.count + g.count
        case .albums(let s): return s.count
        }
    }

    func containsMedia(_ id: FfiMediaUuid) -> Bool {
        if case .items(let m, _) = self { return m.contains(id) }
        return false
    }

    func containsGroup(_ id: FfiGroupUuid) -> Bool {
        if case .items(_, let g) = self { return g.contains(id) }
        return false
    }

    func containsAlbum(_ id: FfiAlbumUuid) -> Bool {
        if case .albums(let s) = self { return s.contains(id) }
        return false
    }

    mutating func toggleMedia(_ id: FfiMediaUuid) {
        switch self {
        case .none:
            self = .items(mediaIds: [id], groupIds: [])
        case .items(var m, let g):
            if m.contains(id) { m.remove(id) } else { m.insert(id) }
            self = (m.isEmpty && g.isEmpty) ? .none : .items(mediaIds: m, groupIds: g)
        case .albums:
            break
        }
    }

    mutating func toggleGroup(_ id: FfiGroupUuid) {
        switch self {
        case .none:
            self = .items(mediaIds: [], groupIds: [id])
        case .items(let m, var g):
            if g.contains(id) { g.remove(id) } else { g.insert(id) }
            self = (m.isEmpty && g.isEmpty) ? .none : .items(mediaIds: m, groupIds: g)
        case .albums:
            break
        }
    }

    mutating func toggleAlbum(_ id: FfiAlbumUuid) {
        switch self {
        case .none:
            self = .albums([id])
        case .albums(var s):
            if s.contains(id) { s.remove(id) } else { s.insert(id) }
            self = s.isEmpty ? .none : .albums(s)
        case .items:
            break
        }
    }

    var selectedMediaIds: Set<FfiMediaUuid> {
        if case .items(let m, _) = self { return m }
        return []
    }

    var selectedGroupIds: Set<FfiGroupUuid> {
        if case .items(_, let g) = self { return g }
        return []
    }

    var selectedAlbumIds: Set<FfiAlbumUuid> {
        if case .albums(let s) = self { return s }
        return []
    }
}

// MARK: - Album item (media or group, shown in the same grid)

enum AlbumItem: Hashable, Identifiable {
    case media(FfiMediaItem)
    case group(FfiGroup)

    enum ID: Hashable { case media(FfiMediaUuid), group(FfiGroupUuid) }

    var id: ID {
        switch self {
        case .media(let m): return .media(m.mediaId)
        case .group(let g): return .group(g.groupId)
        }
    }
}

// MARK: - Navigation destination

enum AlbumsDestination: Hashable {
    case album(FfiAlbum)
    case mediaDetail(MediaDetailState)
}

// MARK: - Root nav wrapper

struct AlbumsView: View {
    @State private var path: [AlbumsDestination] = []
    @Environment(\.lascoTheme) var theme
    @Binding var pendingAlbum: FfiAlbum?
    let repository: LibraryRepository
    let session: LibrarySessionState
    let importCoordinator: MediaImportCoordinator
    @State private var model: AlbumListModel

    init(repository: LibraryRepository, session: LibrarySessionState, importCoordinator: MediaImportCoordinator, pendingAlbum: Binding<FfiAlbum?>) {
        self.repository = repository
        self.session = session
        self.importCoordinator = importCoordinator
        _pendingAlbum = pendingAlbum
        _model = State(initialValue: AlbumListModel(repository: repository))
    }

    var body: some View {
        NavigationStack(path: $path) {
            AlbumContentView(album: nil, path: $path)
                .navigationTitle("")
                .hideSystemNavigationBar()
                .navigationDestination(for: AlbumsDestination.self) { dest in
                    switch dest {
                    case .album(let album):
                        AlbumContentView(album: album, path: $path)
                            .navigationBarBackButtonHidden(true)
                            .navigationTitle("")
                            .hideSystemNavigationBar()
                    case .mediaDetail(let detail):
                        MediaDetailView(source: detail.source, startPosition: detail.startPosition, repository: repository, onAlbumTap: { album in path.append(.album(album)) })
                    }
                }
        }
        .background(theme.bg)
        .environment(model)
        .environment(repository)
        .environment(importCoordinator)
        .task { await model.start() }
        .onAppear {
            if let album = pendingAlbum {
                path = [.album(album)]
                pendingAlbum = nil
            }
        }
        .onChange(of: pendingAlbum) { album in
            guard let album else { return }
            path = [.album(album)]
            pendingAlbum = nil
        }
    }
}

// MARK: - Content view

struct AlbumContentView: View {
    @Environment(AlbumListModel.self) private var albumModel
    @Environment(LibraryRepository.self) private var repository
    @Environment(MediaImportCoordinator.self) private var importCoordinator
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    @Environment(ToastManager.self) var toastManager
    let album: FfiAlbum?
    @Binding var path: [AlbumsDestination]
    @State private var detailModel: AlbumDetailModel?

    @State private var showingNewAlbum = false
    @State private var newAlbumName = ""
    @State private var albumToRename: FfiAlbum?
    @State private var renameText = ""
    @State private var showingAddMedia = false
    @State private var mediaLayout: MediaLayout = .grid
    @State private var selection: ContentSelection = .none
    @State private var showingMovePicker = false
    @State private var showingDeleteConfirm = false
    @State private var isDropTargeted = false
    @State private var showingThumbnailPicker = false
    @State private var showingFileImporter = false
    @State private var showingPhotosPicker = false
    @State private var photosPickerItems: [PhotosPickerItem] = []
    @State private var sortAscending: Bool = false

    private var isRoot: Bool { album == nil }

    private var title: String {
        let albumNames = path.compactMap { if case .album(let a) = $0 { return a.name.uppercased() } else { return nil } }
        guard !albumNames.isEmpty else { return "ALBUMS" }
        return albumNames.joined(separator: " / ")
    }

    private var ancestors: Set<FfiAlbumUuid> {
        Set(path.compactMap { if case .album(let a) = $0 { return a.albumId } else { return nil } })
    }

    private var childAlbums: [FfiAlbum] {
        albumModel.albums(parentID: album?.albumId).filter { !$0.deleted }
    }

    // MARK: Body

    var body: some View {
        GeometryReader { geo in
            let columns = geo.size.width > 500 ? 3 : 2
            let gridColumns = Array(repeating: GridItem(.flexible(), spacing: 12), count: columns)
            let mediaGridColumns = Array(repeating: GridItem(.flexible(), spacing: 3), count: columns)

            ZStack(alignment: .top) {
                ScrollView {
                    #if canImport(UIKit)
                    LazyVStack(alignment: .leading, spacing: 24, pinnedViews: [.sectionHeaders]) {
                        Section {
                            VStack(alignment: .leading, spacing: 24) {
                                albumContent(gridColumns: gridColumns, mediaGridColumns: mediaGridColumns)
                            }
                            .padding(.horizontal, 20)
                        } header: {
                            stickyHeader
                                .padding(.horizontal, 20)
                                .background(theme.bg)
                        }
                    }
                    #else
                    VStack(alignment: .leading, spacing: 24) {
                        headerBar
                            .opacity(selection.isSelecting ? 0 : 1)
                        albumContent(gridColumns: gridColumns, mediaGridColumns: mediaGridColumns)
                    }
                    .padding(.horizontal, 20)
                    #endif
                }
                #if os(macOS)
                .onDrop(of: [UTType.fileURL, UTType.image, UTType.movie], isTargeted: $isDropTargeted) { providers in
                    guard !isRoot, let album else { return false }
                    Task {
                        var urls: [URL] = []
                        for provider in providers {
                            if let url = await resolveFileURL(from: provider) {
                                urls.append(url)
                            }
                        }
                        guard !urls.isEmpty else {
                            toastManager.show(error: "Could not read dropped files")
                            return
                        }
                        if let err = await albumModel.importMediaAsync(urls: urls, albumID: album.albumId) {
                            toastManager.show(error: err)
                        } else {
                            toastManager.show(ok: "Imported \(urls.count) item(s) to \(album.name)")
                        }
                    }
                    return true
                }
                #endif

                #if canImport(UIKit)
                theme.bg
                    .frame(maxWidth: .infinity)
                    .frame(height: geo.safeAreaInsets.top)
                    .ignoresSafeArea(edges: .top)
                    .allowsHitTesting(false)
                #endif

                if selection.isSelecting {
                    selectionBar
                        .transition(.move(edge: .top).combined(with: .opacity))
                }

                #if os(macOS)
                if importCoordinator.isImporting {
                    ZStack {
                        Color.black.opacity(0.45)
                        VStack(spacing: 12) {
                            ProgressView()
                                .progressViewStyle(.circular)
                                .tint(theme.ink)
                            Text("Importing…")
                                .font(LascoFont.body())
                                .foregroundStyle(theme.ink)
                        }
                    }
                    .allowsHitTesting(true)
                }

                if isDropTargeted {
                    RoundedRectangle(cornerRadius: 0)
                        .strokeBorder(theme.ink, lineWidth: 3)
                        .allowsHitTesting(false)
                }
                #endif
            }
            .animation(.easeInOut(duration: 0.2), value: selection.isSelecting)
        }
        .background(theme.bg)
        .scrollContentBackground(.hidden)
        .toolbarBackButton(action: { dismiss() }, isVisible: !isRoot)
        .onAppear {
            AppLogger.log(.info, "album shown — '\(album?.name ?? "root")' (\(album?.albumId ?? "root"))")
        }
        .task(id: album?.albumId) {
            await albumModel.load(parentID: album?.albumId)
            guard let albumID = album?.albumId else {
                detailModel = nil
                return
            }
            let model = AlbumDetailModel(albumID: albumID, repository: repository)
            model.ascending = sortAscending
            detailModel = model
            await model.start()
        }
        .sheet(isPresented: $showingNewAlbum) {
            NewAlbumSheet(title: "NEW ALBUM", name: $newAlbumName) {
                let trimmed = newAlbumName.trimmingCharacters(in: .whitespaces)
                guard !trimmed.isEmpty else { return }
                albumModel.createAlbum(name: trimmed, parentID: album?.albumId)
                showingNewAlbum = false
            }
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
        .sheet(item: $albumToRename) { target in
            NewAlbumSheet(title: "RENAME ALBUM", name: $renameText) {
                let trimmed = renameText.trimmingCharacters(in: .whitespaces)
                guard !trimmed.isEmpty else { return }
                albumModel.renameAlbum(id: target.albumId, name: trimmed)
                albumToRename = nil
            }
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showingAddMedia) {
            AddMediaView(targetAlbum: album)
                .environment(albumModel)
                .environment(repository)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showingMovePicker) {
            movePickerSheet
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showingThumbnailPicker) {
            if let albumId = album?.albumId {
                ThumbnailPickerSheet(albumId: albumId, albumName: title, media: albumMediaItems) { mediaId in
                    albumModel.setAlbumThumbnail(albumID: albumId, mediaID: mediaId)
                    showingThumbnailPicker = false
                }
                .environment(repository)
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
            }
        }
        #if canImport(UIKit)
        .fileImporter(
            isPresented: $showingFileImporter,
            allowedContentTypes: [.image, .movie],
            allowsMultipleSelection: true
        ) { result in
            guard let albumId = album?.albumId else { return }
            switch result {
            case .failure(let err):
                toastManager.show(error: err.localizedDescription)
            case .success(let urls):
                Task {
                    if let err = await albumModel.importMediaAsync(urls: urls, albumID: albumId) {
                        toastManager.show(error: err)
                    } else {
                        toastManager.show(ok: "Imported \(urls.count) item(s)")
                    }
                }
            }
        }
        .photosPicker(
            isPresented: $showingPhotosPicker,
            selection: $photosPickerItems,
            maxSelectionCount: 0,
            matching: .any(of: [.images, .videos]),
            photoLibrary: .shared()
        )
        .onChange(of: photosPickerItems) { items in
            guard let albumId = album?.albumId, !items.isEmpty else { return }
            let captured = items
            photosPickerItems = []
            Task {
                var urls: [URL] = []
                for item in captured {
                    if let data = try? await item.loadTransferable(type: Data.self) {
                        let ext: String
                        if item.supportedContentTypes.contains(where: { $0.conforms(to: .movie) }) {
                            ext = "mov"
                        } else {
                            ext = "jpg"
                        }
                        let tmp = FileManager.default.temporaryDirectory
                            .appendingPathComponent(UUID().uuidString)
                            .appendingPathExtension(ext)
                        try? data.write(to: tmp)
                        urls.append(tmp)
                    }
                }
                guard !urls.isEmpty else { return }
                if let err = await albumModel.importMediaAsync(urls: urls, albumID: albumId) {
                    toastManager.show(error: err)
                } else {
                    toastManager.show(ok: "Imported \(urls.count) item(s)")
                }
            }
        }
        #endif
        .confirmationDialog(
            "Delete \(selection.selectedAlbumIds.count) album\(selection.selectedAlbumIds.count == 1 ? "" : "s")?",
            isPresented: $showingDeleteConfirm,
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                for id in selection.selectedAlbumIds {
                    albumModel.deleteAlbum(id: id)
                }
                selection = .none
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This cannot be undone.")
        }
    }

    private var albumMediaItems: [FfiMediaItem] {
        albumItems.compactMap { if case .media(let m) = $0 { return m } else { return nil } }
    }

    private var albumItems: [AlbumItem] {
        detailModel?.items.compactMap { item in
            switch item.kind {
            case "media": return item.media.map { .media($0) }
            case "group": return item.group.map { .group($0) }
            default: return nil
            }
        } ?? []
    }

    private func openDetail(at position: Int) {
        guard let albumID = album?.albumId else { return }
        path.append(.mediaDetail(MediaDetailState(
            source: .albumByDate(albumID: albumID, ascending: sortAscending),
            startPosition: position
        )))
    }

    // MARK: Shared content body

    @ViewBuilder
    private func albumContent(gridColumns: [GridItem], mediaGridColumns: [GridItem]) -> some View {
        if childAlbums.isEmpty && albumItems.isEmpty {
            emptyState
        } else {
            if !childAlbums.isEmpty {
                LazyVGrid(columns: gridColumns, spacing: 12) {
                    ForEach(childAlbums, id: \.albumId) { child in
                        albumCell(child)
                            .onAppear {
                                guard child.albumId == childAlbums.last?.albumId else { return }
                                Task { await albumModel.loadMore(parentID: album?.albumId) }
                            }
                    }
                }
            }

            if !albumItems.isEmpty {
                switch mediaLayout {
                case .list:
                    VStack(spacing: 0) {
                        ForEach(Array(albumItems.enumerated()), id: \.element.id) { idx, item in
                            switch item {
                            case .media(let m):
                                mediaListRow(m)
                            case .group(let g):
                                groupListRow(g)
                            }
                            if idx < albumItems.count - 1 {
                                Divider().background(theme.bgDeep)
                            }
                            if idx == albumItems.count - 1 {
                                Color.clear.frame(height: 1).onAppear {
                                    Task { await detailModel?.loadMore() }
                                }
                            }
                        }
                    }
                    .background(theme.bg)
                case .grid:
                    LazyVGrid(columns: mediaGridColumns, spacing: 3) {
                        ForEach(albumItems) { item in
                            Group {
                                switch item {
                                case .media(let m): mediaGridCell(m)
                                case .group(let g): groupGridCell(g)
                                }
                            }
                            .onAppear {
                                guard item.id == albumItems.last?.id else { return }
                                Task { await detailModel?.loadMore() }
                            }
                        }
                    }
                }
            }
        }

        Spacer(minLength: 40)
    }

    // MARK: Header (iOS sticky / macOS scrolling)

    #if canImport(UIKit)
    private var stickyHeader: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 12) {
                if !isRoot {
                    LascoBackButton(action: { dismiss() })
                }
                Text(title)
                    .font(LascoFont.categoryLarge())
                    .foregroundStyle(theme.ink)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer()
                if !isRoot {
                    addMenu
                    thumbnailMenu
                } else {
                    newAlbumButton
                }
            }
            if !albumItems.isEmpty {
                HStack {
                    Spacer()
                    sortToggle
                    layoutToggle
                }
            }
        }
        .padding(.top, 20)
        .padding(.bottom, 8)
        .opacity(selection.isSelecting ? 0 : 1)
    }
    #endif

    private var headerBar: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                Text(title)
                    .font(LascoFont.categoryLarge())
                    .foregroundStyle(theme.ink)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer()
                HStack(spacing: 16) {
                    if !isRoot {
                        addMenu
                        thumbnailMenu
                    } else {
                        newAlbumButton
                    }
                }
            }
            if !albumItems.isEmpty {
                HStack {
                    Spacer()
                    sortToggle
                    layoutToggle
                }
            }
        }
        .padding(.top, 20)
    }

    private var newAlbumButton: some View {
        Button {
            newAlbumName = ""
            showingNewAlbum = true
        } label: {
            Image("plus").renderingMode(.template).resizable().frame(width: 18, height: 18)
                .font(.system(size: 20, weight: .medium))
        }
        .buttonStyle(.plain)
        .foregroundStyle(theme.ink)
        .accessibilityLabel("Create album")
    }

    private var addMenu: some View {
        Menu {
            #if canImport(UIKit)
            Button {
                showingPhotosPicker = true
            } label: {
                Label("Import from Photos…", systemImage: "photo.on.rectangle")
            }
            Button {
                showingFileImporter = true
            } label: {
                Label("Import from Files…", systemImage: "square.and.arrow.down")
            }
            #endif
            Button {
                showingAddMedia = true
            } label: {
                Label("Add from library…", systemImage: "photo.on.rectangle.angled")
            }
            Button {
                newAlbumName = ""
                showingNewAlbum = true
            } label: {
                Label("Create album", systemImage: "folder.badge.plus")
            }
        } label: {
            Image("plus").renderingMode(.template).resizable().frame(width: 18, height: 18)
                .font(.system(size: 20, weight: .medium))
                .foregroundStyle(theme.ink)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Add to album")
    }

    private var thumbnailMenu: some View {
        Menu {
            Button {
                showingThumbnailPicker = true
            } label: {
                Label("Set thumbnail…", systemImage: "photo.badge.checkmark")
            }
        } label: {
            Image("ellipses-horizontal").renderingMode(.template).resizable().frame(width: 18, height: 18)
                .font(.system(size: 20, weight: .medium))
                .foregroundStyle(theme.ink)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Album actions")
    }

    // MARK: Selection bar

    private var selectionBar: some View {
        HStack(spacing: 0) {
            Button {
                selection = .none
            } label: {
                Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(theme.ink)

            if selection.count > 1 {
                Text("\(selection.count) selected")
                    .font(LascoFont.categorySmall())
                    .foregroundStyle(theme.ink)
                    .padding(.leading, 8)
            }

            Spacer()
            selectionActionsMenu
        }
        .padding(.horizontal, 12)
        .background(theme.pink)
        .overlay(Rectangle().stroke(theme.ink, lineWidth: 2).ignoresSafeArea(edges: .top))
    }

    @ViewBuilder
    private var selectionActionsMenu: some View {
        let mediaIds = selection.selectedMediaIds
        let groupIds = selection.selectedGroupIds
        let selectedAlbumIds = selection.selectedAlbumIds
        let canRename = selectedAlbumIds.count == 1
        let canGroup = mediaIds.count > 1 && groupIds.isEmpty && album != nil
        let canAddToGroup = groupIds.count == 1 && !mediaIds.isEmpty
        let canMove = groupIds.isEmpty && (!mediaIds.isEmpty || !selectedAlbumIds.isEmpty)
        let canRemove = !mediaIds.isEmpty || !groupIds.isEmpty || !selectedAlbumIds.isEmpty

        if canRename || canGroup || canAddToGroup || canMove || canRemove {
            Menu {
                if canRename,
                   let albumId = selectedAlbumIds.first,
                   let target = childAlbums.first(where: { $0.albumId == albumId }) {
                    Button("Rename album") {
                        albumToRename = target
                        renameText = target.name
                    }
                }
                if canGroup, let albumId = album?.albumId {
                    Button("Group together") {
                        albumModel.createGroupFromSelectedMedia(mediaIDs: Array(mediaIds), albumID: albumId)
                        selection = .none
                    }
                }
                if canAddToGroup,
                   let groupId = groupIds.first,
                   let group = albumItems.compactMap({ if case .group(let g) = $0, g.groupId == groupId { return g } else { return nil } }).first {
                    Button("Add to group") {
                        let existingIds = Set(group.mediaIds)
                        for mediaId in mediaIds where !existingIds.contains(mediaId) {
                            albumModel.addMediaToGroup(groupID: groupId, mediaID: mediaId)
                        }
                        selection = .none
                    }
                }
                if canMove {
                    Button("Move to…") { showingMovePicker = true }
                }
                if canRemove {
                    Button(selectedAlbumIds.isEmpty ? "Remove from album" : "Delete", role: .destructive) {
                        handleRemove()
                    }
                }
            } label: {
                Image("ellipses-horizontal").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 18, weight: .medium))
                    .frame(width: 48, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(theme.ink)
            .accessibilityLabel("Selected item actions")
        }
    }

    // MARK: Layout toggle

    private var layoutToggle: some View {
        HStack(spacing: 2) {
            Button {
                mediaLayout = .list
            } label: {
                Image(mediaLayout == .list ? "bullet-list-solid" : "bullet-list").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .foregroundStyle(mediaLayout == .list ? theme.ink : theme.inkMuted)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Button {
                mediaLayout = .grid
            } label: {
                Image(mediaLayout == .grid ? "grid-solid" : "grid").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .foregroundStyle(mediaLayout == .grid ? theme.ink : theme.inkMuted)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    private var sortToggle: some View {
        Button {
            sortAscending.toggle()
            Task { await detailModel?.setSortAscending(sortAscending) }
        } label: {
            Image(systemName: sortAscending ? "arrow.up" : "arrow.down")
                .foregroundStyle(theme.ink)
                .frame(width: 36, height: 36)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: Move picker sheet

    private var movePickerSheet: some View {
        AlbumPickerView(repository: repository, title: "Move to", onSelect: { targetAlbum in
            handleMoveTo(targetAlbumId: targetAlbum.albumId)
            showingMovePicker = false
        }, onCancel: {
            showingMovePicker = false
        })
        .environment(albumModel)
        .environment(repository)
    }

    // MARK: Actions

    private func handleRemove() {
        switch selection {
        case .none:
            break
        case .items(let mediaIds, let groupIds):
            guard let albumId = album?.albumId else { break }
            for id in mediaIds {
                albumModel.removeMediaFromAlbum(albumID: albumId, mediaID: id)
            }
            for id in groupIds {
                albumModel.deleteGroup(groupID: id)
            }
            selection = .none
        case .albums:
            showingDeleteConfirm = true
        }
    }

    private func handleMoveTo(targetAlbumId: FfiAlbumUuid) {
        switch selection {
        case .none:
            break
        case .items(let mediaIds, _):
            guard let fromAlbumId = album?.albumId else { break }
            for id in mediaIds {
                albumModel.moveMediaToAlbum(mediaID: id, fromAlbumID: fromAlbumId, toAlbumID: targetAlbumId)
            }
            selection = .none
        case .albums(let ids):
            for id in ids {
                albumModel.reparentAlbum(id: id, parentID: targetAlbumId)
            }
            selection = .none
        }
    }

    // MARK: Album cells

    @ViewBuilder
    private func albumCell(_ child: FfiAlbum) -> some View {
        if ancestors.contains(child.albumId) {
            AlbumCell(album: child)
                .opacity(0.5)
        } else {
            albumCellContent(child, parentInfo: nil)
        }
    }

    @ViewBuilder
    private func albumCellContent(_ child: FfiAlbum, parentInfo: String?) -> some View {
        let isSelected = selection.containsAlbum(child.albumId)
        AlbumCell(album: child, parentInfo: parentInfo, isSelected: isSelected)
            .onTapGesture {
                if selection.isSelecting {
                    selection.toggleAlbum(child.albumId)
                } else {
                    path.append(.album(child))
                }
            }
        #if os(macOS)
            .simultaneousGesture(LongPressGesture(minimumDuration: 0.4).onEnded { _ in
                selection.toggleAlbum(child.albumId)
            })
        #endif
        .contextMenu {
            if selection.isSelecting {
                Button("Move selected to…") { showingMovePicker = true }
                Button("Delete selected", role: .destructive) { showingDeleteConfirm = true }
            } else {
                Button("Rename") {
                    albumToRename = child
                    renameText = child.name
                }
                Button("Move to…") {
                    selection = .albums([child.albumId])
                    showingMovePicker = true
                }
                Button("Delete", role: .destructive) {
                    selection = .albums([child.albumId])
                    showingDeleteConfirm = true
                }
            }
        }
    }

    // MARK: Group cells

    // MARK: Group cells

    @ViewBuilder
    private func groupGridCell(_ group: FfiGroup) -> some View {
        let isSelected = selection.containsGroup(group.groupId)
        GroupGridCell(group: group, isSelected: isSelected)
            .onTapGesture {
                if selection.isSelecting {
                    selection.toggleGroup(group.groupId)
                } else if let idx = albumItems.firstIndex(of: .group(group)) {
                    openDetail(at: idx)
                }
            }
            .onLongPressGesture { selection.toggleGroup(group.groupId) }
    }

    @ViewBuilder
    private func groupListRow(_ group: FfiGroup) -> some View {
        let isSelected = selection.containsGroup(group.groupId)
        GroupListRow(group: group, isSelected: isSelected)
            .onTapGesture {
                if selection.isSelecting {
                    selection.toggleGroup(group.groupId)
                } else if let idx = albumItems.firstIndex(of: .group(group)) {
                    openDetail(at: idx)
                }
            }
            .onLongPressGesture { selection.toggleGroup(group.groupId) }
    }

    // MARK: Media cells

    @ViewBuilder
    private func mediaListRow(_ item: FfiMediaItem) -> some View {
        let isSelected = selection.containsMedia(item.mediaId)
        MediaRow(item: item, isSelected: isSelected)
            .onTapGesture {
                if selection.isSelecting {
                    selection.toggleMedia(item.mediaId)
                } else if let idx = albumItems.firstIndex(of: .media(item)) {
                    openDetail(at: idx)
                }
            }
            .onLongPressGesture { selection.toggleMedia(item.mediaId) }
        #if os(macOS)
            .contextMenu { mediaContextMenu(item) }
        #endif
    }

    @ViewBuilder
    private func mediaGridCell(_ item: FfiMediaItem) -> some View {
        let isSelected = selection.containsMedia(item.mediaId)
        MediaGridCell(item: item, isSelected: isSelected)
            .onTapGesture {
                if selection.isSelecting {
                    selection.toggleMedia(item.mediaId)
                } else if let idx = albumItems.firstIndex(of: .media(item)) {
                    openDetail(at: idx)
                }
            }
            .onLongPressGesture { selection.toggleMedia(item.mediaId) }
        #if os(macOS)
            .contextMenu { mediaContextMenu(item) }
        #endif
    }

    @ViewBuilder
    private func mediaContextMenu(_ item: FfiMediaItem) -> some View {
        if selection.isSelecting {
            Button("Move selected to…") { showingMovePicker = true }
            if album != nil {
                Button("Remove selected from album", role: .destructive) { handleRemove() }
            }
        } else {
            if let albumId = album?.albumId {
                Button("Set as album thumbnail") {
                    albumModel.setAlbumThumbnail(albumID: albumId, mediaID: item.mediaId)
                }
                Button("Move to…") {
                    selection = .items(mediaIds: [item.mediaId], groupIds: [])
                    showingMovePicker = true
                }
                Button("Remove from album", role: .destructive) {
                    albumModel.removeMediaFromAlbum(albumID: albumId, mediaID: item.mediaId)
                }
            }
        }
    }

    // MARK: Misc

    private var emptyState: some View {
        Group {
            if isRoot {
                VStack(alignment: .leading, spacing: 6) {
                    Text("No albums yet.")
                        .font(LascoFont.title())
                        .foregroundStyle(theme.inkSub)
                    Text("Albums will appear here once your library is synced.")
                        .font(LascoFont.body())
                        .foregroundStyle(theme.inkMuted)
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
                .lascoPanel()
            } else {
                Text("Empty album.")
                    .font(LascoFont.title())
                    .foregroundStyle(theme.inkSub)
            }
        }
    }

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(LascoFont.categoryLarge())
            .foregroundStyle(theme.ink)
    }
}

// MARK: - File URL resolution

#if os(macOS)
private func resolveFileURL(from provider: NSItemProvider) async -> URL? {
    await withCheckedContinuation { continuation in
        guard let typeId = provider.registeredTypeIdentifiers.first else {
            continuation.resume(returning: nil)
            return
        }
        provider.loadInPlaceFileRepresentation(forTypeIdentifier: typeId) { url, _, _ in
            continuation.resume(returning: url)
        }
    }
}
#endif

// MARK: - Date formatting

let iso8601Formatter: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return f
}()

let iso8601FormatterNoFrac: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime]
    return f
}()

let displayDateFormatter: DateFormatter = {
    let f = DateFormatter()
    f.dateStyle = .medium
    f.timeStyle = .none
    return f
}()

func formatMediaDate(_ raw: String) -> String {
    let date = iso8601Formatter.date(from: raw) ?? iso8601FormatterNoFrac.date(from: raw)
    guard let date else { return raw }
    return displayDateFormatter.string(from: date)
}

// MARK: - AlbumCell

struct AlbumCell: View {
    let album: FfiAlbum
    var parentInfo: String? = nil
    var isSelected: Bool = false
    @Environment(LibraryRepository.self) private var repository
    @Environment(\.lascoTheme) var theme
    @State private var thumbnail: Image? = nil

    var body: some View {
        ZStack(alignment: .topTrailing) {
            VStack(alignment: .leading, spacing: 8) {
                theme.bgDeep
                    .aspectRatio(1, contentMode: .fit)
                    .overlay {
                        if let thumbnail {
                            thumbnail
                                .resizable()
                                .scaledToFill()
                        } else {
                            Image("image").renderingMode(.template).resizable().frame(width: 28, height: 28)
                                .foregroundStyle(theme.inkMuted)
                        }
                    }
                    .clipped()

                VStack(alignment: .leading, spacing: 2) {
                    Text(album.name)
                        .font(LascoFont.body())
                        .foregroundStyle(theme.ink)
                        .lineLimit(1)
                    if let parentInfo {
                        Text(parentInfo)
                            .font(LascoFont.pixel())
                            .foregroundStyle(theme.inkMuted)
                            .lineLimit(1)
                    }
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 8)
            }
            .lascoPanel()

            if isSelected {
                Image("check-circle-solid").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 20))
                    .foregroundStyle(theme.pink)
                    .padding(6)
            }
        }
        .task(id: album.albumId) {
            thumbnail = nil
            let items = (try? await repository.albumItems(albumID: album.albumId, ascending: false, offset: 0, limit: 1)) ?? []
            if let mediaId = album.thumbnailMediaId ?? items.compactMap({ $0.media?.mediaId }).first,
               let data = try? await repository.thumbnailAsync(mediaID: mediaId) {
                thumbnail = Image(data: data)
            }
        }
    }
}

// MARK: - ThumbnailPickerSheet

private struct ThumbnailPickerSheet: View {
    let albumId: FfiAlbumUuid
    let albumName: String
    let media: [FfiMediaItem]
    let onPick: (FfiMediaUuid) -> Void
    @Environment(LibraryRepository.self) private var repository
    @Environment(\.lascoTheme) var theme
    @Environment(\.dismiss) private var dismiss

    private let columns = [GridItem(.flexible(), spacing: 4), GridItem(.flexible(), spacing: 4), GridItem(.flexible(), spacing: 4)]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Set thumbnail")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                    Text("for \(albumName)")
                        .font(LascoFont.categorySmall())
                        .foregroundStyle(theme.inkMuted)
                }
                Spacer()
                Button { dismiss() } label: {
                    Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                        .font(.system(size: 16, weight: .medium))
                        .foregroundStyle(theme.ink)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)

            if media.isEmpty {
                Text("No media in this album.")
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkMuted)
                    .padding(20)
            } else {
                ScrollView {
                    LazyVGrid(columns: columns, spacing: 4) {
                        ForEach(media, id: \.mediaId) { item in
                            ThumbnailPickerCell(item: item)
                                .onTapGesture { onPick(item.mediaId) }
                        }
                    }
                    .padding(4)
                }
            }
        }
        .background(theme.bg)
    }
}

private struct ThumbnailPickerCell: View {
    let item: FfiMediaItem
    @Environment(LibraryRepository.self) private var repository
    @Environment(\.lascoTheme) var theme
    @State private var thumbnail: Image? = nil

    var body: some View {
        theme.bgDeep
            .aspectRatio(1, contentMode: .fit)
            .overlay {
                if let thumbnail {
                    thumbnail.resizable().scaledToFill()
                } else {
                    Image("image").renderingMode(.template).resizable().frame(width: 18, height: 18)
                        .font(.system(size: 20))
                        .foregroundStyle(theme.inkMuted)
                }
            }
            .clipped()
            .contentShape(Rectangle())
            .task(id: item.mediaId) {
                if let data = try? await repository.thumbnailAsync(mediaID: item.mediaId) {
                    thumbnail = Image(data: data)
                }
            }
    }
}

// MARK: - NewAlbumSheet

private struct NewAlbumSheet: View {
    let title: String
    @Binding var name: String
    var onConfirm: () -> Void
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(title)
                .font(LascoFont.categoryLarge())
                .foregroundStyle(theme.ink)

            TextField("Album name", text: $name)
                .font(LascoFont.body())
                .textFieldStyle(.plain)
                .padding(12)
                .lascoPanel()
                .focused($focused)
                .onSubmit { onConfirm() }

            HStack(spacing: 12) {
                Button("Cancel") { dismiss() }
                    .buttonStyle(.plain)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkMuted)

                Spacer()

                Button("Confirm") { onConfirm() }
                    .buttonStyle(.plain)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(24)
        .background(theme.bg)
        .presentationDetents([.height(220)])
        .onAppear { focused = true }
    }
}
