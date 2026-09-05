import SwiftUI
import UniformTypeIdentifiers
#if canImport(PhotosUI)
import PhotosUI
#endif
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

// MARK: - Navigation destination

enum LibraryDestination: Hashable {
    case mediaDetail(MediaDetailState)
}

// MARK: - ContentView

struct ContentView: View {
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    let openAlbum: (FfiAlbum) -> Void
    let repository: LibraryRepository
    let session: LibrarySessionState
    let importCoordinator: MediaImportCoordinator

    @State private var model: RecentMediaModel

    init(repository: LibraryRepository, session: LibrarySessionState, importCoordinator: MediaImportCoordinator, openAlbum: @escaping (FfiAlbum) -> Void) {
        self.repository = repository
        self.session = session
        self.importCoordinator = importCoordinator
        self.openAlbum = openAlbum
        _model = State(initialValue: RecentMediaModel(repository: repository))
    }

    @State private var showingImportMedia = false
    @State private var showingPhotosPicker = false
    @State private var photosPickerItems: [PhotosPickerItem] = []
    @State private var path: [LibraryDestination] = []
    @State private var selection: Set<FfiMediaUuid> = []
    @State private var isSelecting = false
    @State private var albumsForMedia: AlbumList? = nil
    @State private var showingAddToAlbumPicker = false
    @State private var allMediaScrollPosition: FfiMediaUuid?
    @State private var orphanMediaScrollPosition: FfiMediaUuid?

    var body: some View {
        NavigationStack(path: $path) {
            GeometryReader { geo in
                let columns = geo.size.width > 500 ? 3 : 2
                let gridColumns = Array(repeating: GridItem(.flexible(), spacing: 3), count: columns)

                ZStack(alignment: .top) {
                    if model.showingOrphans {
                        RecentMediaScrollView(
                            mode: .orphans,
                            scrollPosition: $orphanMediaScrollPosition,
                            header: { header.opacity(isSelecting ? 0 : 1) },
                            content: {
                                mediaContent(
                                    media: model.media(for: .orphans),
                                    mode: .orphans,
                                    gridColumns: gridColumns
                                )
                            }
                        )
                    } else {
                        RecentMediaScrollView(
                            mode: .all,
                            scrollPosition: $allMediaScrollPosition,
                            header: { header.opacity(isSelecting ? 0 : 1) },
                            content: {
                                mediaContent(
                                    media: model.media(for: .all),
                                    mode: .all,
                                    gridColumns: gridColumns
                                )
                            }
                        )
                    }

                    #if canImport(UIKit)
                    theme.bg
                        .frame(maxWidth: .infinity)
                        .frame(height: geo.safeAreaInsets.top)
                        .ignoresSafeArea(edges: .top)
                        .allowsHitTesting(false)
                    #endif

                    if isSelecting {
                        selectionBar
                            .transition(.move(edge: .top).combined(with: .opacity))
                    }
                }
                .animation(.easeInOut(duration: 0.2), value: isSelecting)
            }
            .background(theme.bg)
            .navigationTitle("")
            .hideSystemNavigationBar()
            .navigationDestination(for: LibraryDestination.self) { dest in
                switch dest {
                case .mediaDetail(let state):
                    MediaDetailView(seed: state.seed, repository: repository, onAlbumTap: openAlbum)
                }
            }
            .sheet(item: $albumsForMedia) { list in
                OpenAlbumPickerSheet(media: list.media, albums: list.albums) { album in
                    albumsForMedia = nil
                    selection = []
                    isSelecting = false
                    openAlbum(album)
                } onCancel: {
                    albumsForMedia = nil
                }
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
            }
            .sheet(isPresented: $showingAddToAlbumPicker) {
                AlbumPickerView(
                    repository: repository,
                    title: "Add to album",
                    onSelect: addSelectionToAlbum,
                    onCancel: { showingAddToAlbumPicker = false }
                )
                .environment(\.lascoTheme, .dark)
                .preferredColorScheme(.dark)
            }
        }
        .background(theme.bg)
        .fileImporter(
            isPresented: $showingImportMedia,
            allowedContentTypes: [.image, .movie],
            allowsMultipleSelection: true
        ) { result in
            switch result {
            case .failure(let err):
                toastManager.show(error: err.localizedDescription)
            case .success(let urls):
                doImport(urls: urls)
            }
        }
        #if canImport(UIKit)
        .photosPicker(
            isPresented: $showingPhotosPicker,
            selection: $photosPickerItems,
            maxSelectionCount: 0,
            matching: .any(of: [.images, .videos]),
            photoLibrary: .shared()
        )
        .onChange(of: photosPickerItems) { _, items in
            guard !items.isEmpty else { return }
            let captured = items
            photosPickerItems = []
            Task {
                let urls = await temporaryURLs(for: captured)
                guard !urls.isEmpty else {
                    toastManager.show(error: "Could not read the selected photos")
                    return
                }
                doImport(urls: urls)
            }
        }
        #endif
        .onAppear {
            AppLogger.log(.info, "home screen shown — \(model.media.count) media items")
        }
        .task { await model.start() }
        .onChange(of: model.showingOrphans) {
            selection = []
            isSelecting = false
            let mode = model.mode
            Task { await model.load(mode: mode) }
        }
        .environment(repository)
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 12) {
            Text("LIBRARY")
                .font(LascoFont.categoryLarge())
                .foregroundStyle(theme.ink)
            Spacer()
            Toggle(isOn: $model.showingOrphans) {
                Text(model.showingOrphans ? "Orphan" : "All")
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkSub)
            }
                .accessibilityLabel("Media filter")
                .accessibilityValue(model.showingOrphans ? "On" : "Off")
            addMenu
        }
        .padding(.top, 20)
        .padding(.bottom, 8)
    }

    // MARK: Mascot banner

    private var mascotBanner: some View {
        GeometryReader { geo in
            HStack(alignment: .center, spacing: 16) {
                Text("Lasco is still new! Happy to hear your feedback at feedback@getlasco.app")
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)

                AsyncImage(url: URL(string: "https://public.getlasco.app/mascot_teleoperator.png")) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFit()
                    default:
                        Color.clear
                    }
                }
                .frame(width: geo.size.width * 0.5)
            }
        }
        .frame(height: 160)
    }

    // MARK: Selection bar

    private var selectionBar: some View {
        HStack(spacing: 0) {
            Button {
                selection = []
                isSelecting = false
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

            Menu {
                Button {
                    showingAddToAlbumPicker = true
                } label: {
                    Label("Add to album", systemImage: "folder.badge.plus")
                }
            } label: {
                Text("...")
                    .font(LascoFont.body())
                    .frame(width: 48, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(theme.ink)
            .accessibilityLabel("Selection actions")
        }
        .padding(.horizontal, 12)
        .background(theme.pink)
        .overlay(Rectangle().stroke(theme.ink, lineWidth: 2).ignoresSafeArea(edges: .top))
    }

    // MARK: Media content

    @ViewBuilder
    private func mediaContent(
        media: [FfiMediaItem],
        mode: RecentMediaMode,
        gridColumns: [GridItem]
    ) -> some View {
        if media.isEmpty {
            Text(mode == .orphans ? "No orphan media." : "No media yet.")
                .font(LascoFont.title())
                .foregroundStyle(theme.inkSub)
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            LazyVGrid(columns: gridColumns, spacing: 3) {
                ForEach(Array(media.enumerated()), id: \.element.mediaId) { position, item in
                    let isSelected = selection.contains(item.mediaId)
                    MediaGridCell(item: item, isSelected: isSelected)
                        .onTapGesture {
                            if isSelecting {
                                if selection.contains(item.mediaId) {
                                    selection.remove(item.mediaId)
                                    if selection.isEmpty { isSelecting = false }
                                } else {
                                    selection.insert(item.mediaId)
                                }
                            } else {
                                path.append(.mediaDetail(MediaDetailState(seed: MediaGallerySeed(
                                    source: mode == .orphans ? .orphansByDate : .homeByDate,
                                    position: position,
                                    item: .media(item),
                                    totalCount: model.totalCount(for: mode)
                                ))))
                            }
                        }
                        .onLongPressGesture {
                            if !isSelecting {
                                isSelecting = true
                                selection = [item.mediaId]
                            } else {
                                selection.insert(item.mediaId)
                            }
                        }
                        #if os(macOS)
                        .contextMenu {
                            Button {
                                triggerOpenAlbum(for: item.mediaId)
                            } label: {
                                Label("Open Album…", systemImage: "folder")
                            }
                        }
                        #endif
                        .onAppear {
                            guard item.mediaId == media.last?.mediaId else { return }
                            Task { await model.loadMore(mode: mode) }
                        }
                }
            }
            .scrollTargetLayout()
        }

        Spacer(minLength: 40)
    }

    // MARK: Open album

    private func triggerOpenAlbum(for mediaId: FfiMediaUuid) {
        Task {
            let containing = await model.albumsContainingMedia(id: mediaId)
            guard !containing.isEmpty else { return }
            if containing.count == 1 {
                selection = []
                isSelecting = false
                openAlbum(containing[0])
            } else if let mediaItem = await model.showMedia(id: mediaId) {
                albumsForMedia = AlbumList(media: mediaItem, albums: containing)
            }
        }
    }

    private func addSelectionToAlbum(_ album: FfiAlbum) {
        let mediaIds = selection
        showingAddToAlbumPicker = false
        Task {
            do {
                for mediaId in mediaIds {
                    try await repository.addMediaToAlbum(albumID: album.albumId, mediaID: mediaId)
                }
                selection = []
                isSelecting = false
                toastManager.show(ok: "Added \(mediaIds.count) item(s) to \(album.name)")
            } catch {
                toastManager.show(error: error.localizedDescription)
            }
        }
    }

    // MARK: Import helpers

    private var addMenu: some View {
        Menu {
            #if canImport(UIKit)
            Button {
                showingPhotosPicker = true
            } label: {
                Label("Import from Photos…", systemImage: "photo.on.rectangle")
            }
            #endif
            Button {
                showingImportMedia = true
            } label: {
                Label("Import from Files…", systemImage: "square.and.arrow.down")
            }
        } label: {
            Image("plus")
                .renderingMode(.template)
                .resizable()
                .frame(width: 18, height: 18)
                .font(.system(size: 20, weight: .medium))
                .foregroundStyle(theme.ink)
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Import orphan media")
    }

    #if canImport(UIKit)
    private func temporaryURLs(for items: [PhotosPickerItem]) async -> [URL] {
        var urls: [URL] = []
        for item in items {
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let fileExtension = item.supportedContentTypes.contains(where: { $0.conforms(to: .movie) }) ? "mov" : "jpg"
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent(UUID().uuidString)
                .appendingPathExtension(fileExtension)
            do {
                try data.write(to: url)
                urls.append(url)
            } catch {
                AppLogger.log(.error, "could not write selected photo to a temporary file: \(error)")
            }
        }
        return urls
    }
    #endif

    private func doImport(urls: [URL]) {
        Task {
            if let err = await importCoordinator.importMedia(urls: urls) {
                toastManager.show(error: err)
            } else {
                toastManager.show(ok: "Imported \(urls.count) item(s) as orphan media")
            }
        }
    }

}

// MARK: - AlbumList wrapper

private struct AlbumList: Identifiable {
    let id = UUID()
    let media: FfiMediaItem
    let albums: [FfiAlbum]
}

// MARK: - OpenAlbumPickerSheet

private struct OpenAlbumPickerSheet: View {
    @Environment(LibraryRepository.self) private var repository
    let media: FfiMediaItem
    let albums: [FfiAlbum]
    let onSelect: (FfiAlbum) -> Void
    let onCancel: () -> Void
    @Environment(\.lascoTheme) var theme
    @State private var thumbnail: Image? = nil

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text("THIS MEDIA IS CONTAINED IN THESE ALBUMS:")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                        .padding(.top, 20)

                    HStack(alignment: .center, spacing: 14) {
                        ZStack {
                            theme.bgDeep
                            if let thumbnail {
                                thumbnail
                                    .resizable()
                                    .scaledToFill()
                            } else {
                                Image("image").renderingMode(.template).resizable().frame(width: 18, height: 18)
                                    .font(.system(size: 18))
                                    .foregroundStyle(theme.inkMuted)
                            }
                        }
                        .frame(width: 64, height: 64)
                        .clipped()
                        .cornerRadius(6)

                        Text(media.name ?? "")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.ink)
                            .lineLimit(2)
                    }

                    VStack(spacing: 0) {
                        ForEach(albums, id: \.albumId) { album in
                            Button {
                                onSelect(album)
                            } label: {
                                HStack(spacing: 10) {
                                    Image("folder").renderingMode(.template).resizable().frame(width: 18, height: 18)
                                        .font(.system(size: 15))
                                        .foregroundStyle(theme.inkMuted)
                                        .frame(width: 20)
                                    Text(album.name)
                                        .font(LascoFont.body())
                                        .foregroundStyle(theme.ink)
                                    Spacer()
                                    Image("angle-right").renderingMode(.template).resizable().frame(width: 18, height: 18)
                                        .font(.system(size: 12, weight: .medium))
                                        .foregroundStyle(theme.inkMuted)
                                }
                                .padding(.horizontal, 16)
                                .padding(.vertical, 12)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            if album.albumId != albums.last?.albumId {
                                Divider().background(theme.bgDeep)
                            }
                        }
                    }
                    .lascoPanel()

                    Spacer(minLength: 40)
                }
                .padding(.horizontal, 20)
            }
            .background(theme.bg)

            HStack {
                Spacer()
                Button("Cancel", action: onCancel)
                    .buttonStyle(LascoGhostButtonStyle())
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 14)
            .background(theme.surfaceAlt)
            .overlay(Rectangle().stroke(theme.ink, lineWidth: 2).ignoresSafeArea(edges: .bottom))
        }
        .background(theme.bg)
        #if os(macOS)
        .frame(minWidth: 400, minHeight: 300)
        #endif
        .task(id: media.mediaId) {
            if let data = try? await repository.thumbnailAsync(mediaID: media.mediaId) {
                thumbnail = Image(data: data)
            }
        }
    }
}
