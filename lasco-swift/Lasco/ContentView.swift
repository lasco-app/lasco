import SwiftUI
import UniformTypeIdentifiers
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
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(ToastManager.self) var toastManager
    @Environment(\.lascoTheme) var theme

    let openAlbum: (FfiAlbum) -> Void

    @AppStorage("devMode") private var devMode = false
    @State private var showingImportMedia = false
    @State private var pendingImportUrls: [URL] = []
    @State private var showingAlbumPicker = false
    @State private var path: [LibraryDestination] = []
    @State private var selection: Set<String> = []
    @State private var isSelecting = false
    @State private var albumsForMedia: AlbumList? = nil
    @State private var showingDefaultAlbumPicker = false

    var body: some View {
        NavigationStack(path: $path) {
            GeometryReader { geo in
                let columns = geo.size.width > 500 ? 3 : 2
                let gridColumns = Array(repeating: GridItem(.flexible(), spacing: 3), count: columns)
                let media = libraryModel.media

                ZStack(alignment: .top) {
                    ScrollView {
                        #if canImport(UIKit)
                        LazyVStack(alignment: .leading, spacing: 24, pinnedViews: [.sectionHeaders]) {
                            Section {
                                VStack(alignment: .leading, spacing: 24) {
                                    // mascotBanner
                                    mediaContent(media: media, gridColumns: gridColumns)
                                }
                                .padding(.horizontal, 20)
                            } header: {
                                header
                                    .padding(.horizontal, 20)
                                    .background(theme.bg)
                                    .opacity(isSelecting ? 0 : 1)
                            }
                        }
                        #else
                        VStack(alignment: .leading, spacing: 24) {
                            header
                                .padding(.horizontal, 20)
                                .opacity(isSelecting ? 0 : 1)
                            // mascotBanner
                            //     .padding(.horizontal, 20)
                            mediaContent(media: media, gridColumns: gridColumns)
                                .padding(.horizontal, 20)
                        }
                        #endif
                    }
                    .background(theme.bg)
                    .scrollContentBackground(.hidden)

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
                    MediaDetailView(media: state.items, startIndex: state.startIndex, startThumbnail: state.startThumbnail, onAlbumTap: openAlbum)
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
        }
        .background(theme.bg)
        .sheet(isPresented: $showingAlbumPicker) {
            AlbumPickerView(title: "Choose import destination") { album in
                showingAlbumPicker = false
                doImport(urls: pendingImportUrls, album: album)
                pendingImportUrls = []
            } onCancel: {
                showingAlbumPicker = false
                pendingImportUrls = []
            }
            .environmentObject(libraryModel)
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showingDefaultAlbumPicker) {
            AlbumPickerView(title: "Default upload album") { album in
                libraryModel.setDefaultUploadAlbum(albumId: album.albumId)
                showingDefaultAlbumPicker = false
            } onCancel: {
                showingDefaultAlbumPicker = false
            }
            .environmentObject(libraryModel)
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
        .fileImporter(
            isPresented: $showingImportMedia,
            allowedContentTypes: [.image, .movie],
            allowsMultipleSelection: true
        ) { result in
            switch result {
            case .failure(let err):
                toastManager.show(error: err.localizedDescription)
            case .success(let urls):
                if let defaultAlbum = libraryModel.defaultUploadAlbum {
                    doImport(urls: urls, album: defaultAlbum)
                } else {
                    pendingImportUrls = urls
                    showingAlbumPicker = true
                }
            }
        }
        .onAppear {
            AppLogger.log(.info, "home screen shown — \(libraryModel.media.count) media items")
        }
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 12) {
            Text("LIBRARY")
                .font(LascoFont.categoryLarge())
                .foregroundStyle(theme.ink)
            Spacer()
            Button {
                guard libraryModel.isOpen else {
                    toastManager.show(error: "No library open")
                    return
                }
                showingImportMedia = true
            } label: {
                Image("upload").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 20, weight: .medium))
                    .foregroundStyle(theme.ink)
            }
            .buttonStyle(.plain)
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

            if selection.count == 1, let mediaId = selection.first {
                Button {
                    triggerOpenAlbum(for: mediaId)
                } label: {
                    Image("folder").renderingMode(.template).resizable().frame(width: 18, height: 18)
                        .font(.system(size: 18, weight: .medium))
                        .frame(width: 48, height: 44)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(theme.ink)
            }
        }
        .padding(.horizontal, 12)
        .background(theme.pink)
        .overlay(Rectangle().stroke(theme.ink, lineWidth: 2).ignoresSafeArea(edges: .top))
    }

    // MARK: Media content

    @ViewBuilder
    private func mediaContent(media: [FfiMediaItem], gridColumns: [GridItem]) -> some View {
        if media.isEmpty {
            Text("No media yet.")
                .font(LascoFont.title())
                .foregroundStyle(theme.inkSub)
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            if devMode {
                HStack {
                    Text("DEV MODE")
                        .font(LascoFont.categorySmall())
                        .foregroundStyle(theme.ink)
                    Spacer()
                    Button("Import random media") {
                        importRandomMedia()
                    }
                    .buttonStyle(.plain)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .background(theme.pink)
            }

            if libraryModel.defaultUploadAlbumId == nil {
                HStack(spacing: 12) {
                    Text("Auto-import paused — no default upload album set.")
                        .font(LascoFont.body())
                        .foregroundStyle(theme.ink)
                    Spacer()
                    Button("Set album →") {
                        showingDefaultAlbumPicker = true
                    }
                    .buttonStyle(.plain)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .background(theme.pink)
            }

            LazyVGrid(columns: gridColumns, spacing: 3) {
                ForEach(media, id: \.mediaId) { item in
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
                            } else if let idx = media.firstIndex(where: { $0.mediaId == item.mediaId }) {
                                let thumb = libraryModel.thumbnail(for: item.mediaId)
                                path.append(.mediaDetail(MediaDetailState(items: media.map { .media($0) }, startIndex: idx, startThumbnail: thumb)))
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
                }
            }
        }

        Spacer(minLength: 40)
    }

    // MARK: Open album

    private func triggerOpenAlbum(for mediaId: String) {
        let containing = libraryModel.albumsContainingMedia(mediaId: mediaId)
        guard !containing.isEmpty else { return }
        if containing.count == 1 {
            selection = []
            isSelecting = false
            openAlbum(containing[0])
        } else {
            guard let mediaItem = libraryModel.showMedia(mediaId: mediaId) else { return }
            albumsForMedia = AlbumList(media: mediaItem, albums: containing)
        }
    }

    // MARK: Import helpers

    private func doImport(urls: [URL], album: FfiAlbum) {
        if let err = libraryModel.importMedia(urls: urls, albumId: album.albumId) {
            toastManager.show(error: err)
        } else {
            toastManager.show(ok: "Imported \(urls.count) item(s) to \(album.name)")
        }
    }

    private func importRandomMedia() {
        guard libraryModel.isOpen else {
            toastManager.show(error: "No library open")
            return
        }
        guard let url = generateRandomImage() else {
            toastManager.show(error: "Failed to generate random image")
            return
        }
        if let defaultAlbum = libraryModel.defaultUploadAlbum {
            doImport(urls: [url], album: defaultAlbum)
        } else {
            pendingImportUrls = [url]
            showingAlbumPicker = true
        }
    }

    private func generateRandomImage() -> URL? {
        let size = CGSize(width: 512, height: 512)
        let r = CGFloat.random(in: 0...1)
        let g = CGFloat.random(in: 0...1)
        let b = CGFloat.random(in: 0...1)

        let data: Data?
        #if canImport(UIKit)
        let renderer = UIGraphicsImageRenderer(size: size)
        let image = renderer.image { ctx in
            UIColor(red: r, green: g, blue: b, alpha: 1).setFill()
            ctx.fill(CGRect(origin: .zero, size: size))
        }
        data = image.pngData()
        #else
        let image = NSImage(size: size)
        image.lockFocus()
        NSColor(red: r, green: g, blue: b, alpha: 1).setFill()
        NSRect(origin: .zero, size: size).fill()
        image.unlockFocus()
        if let tiff = image.tiffRepresentation,
           let bitmap = NSBitmapImageRep(data: tiff) {
            data = bitmap.representation(using: .png, properties: [:])
        } else {
            data = nil
        }
        #endif

        guard let pngData = data else { return nil }
        let name = randomMediaName()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(name).png")
        try? pngData.write(to: url)
        return url
    }

    private func randomMediaName() -> String {
        let adjectives = ["crimson", "azure", "golden", "silver", "jade", "amber", "coral", "violet", "indigo", "scarlet"]
        let nouns = ["horizon", "drift", "pulse", "echo", "flare", "trace", "bloom", "spark", "tide", "mist"]
        return "\(adjectives.randomElement()!)-\(nouns.randomElement()!)-\(Int.random(in: 1000...9999))"
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
    @EnvironmentObject var libraryModel: LibraryModel
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
            if let data = libraryModel.thumbnail(for: media.mediaId) {
                thumbnail = Image(data: data)
            }
        }
    }
}
