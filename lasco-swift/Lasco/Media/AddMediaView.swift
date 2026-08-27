import SwiftUI

private enum AddMediaPickerTab: Hashable {
    case allMedia
    case albums
}

struct AddMediaView: View {
    @Environment(AlbumListModel.self) private var albumModel
    @Environment(ToastManager.self) private var toastManager
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) private var theme

    let targetAlbum: FfiAlbum
    let repository: LibraryRepository

    @State private var selectedMediaIds: Set<FfiMediaUuid> = []
    @State private var alreadyAddedMediaIds: Set<FfiMediaUuid> = []
    @State private var selectedTab: AddMediaPickerTab = .allMedia
    @State private var albumPath: [FfiAlbum] = []
    @State private var recentMediaModel: RecentMediaModel

    init(targetAlbum: FfiAlbum, repository: LibraryRepository) {
        self.targetAlbum = targetAlbum
        self.repository = repository
        _recentMediaModel = State(initialValue: RecentMediaModel(repository: repository))
    }

    var body: some View {
        VStack(spacing: 0) {
            AddMediaPickerHeader(selectedTab: $selectedTab)

            switch selectedTab {
            case .allMedia:
                AddMediaAllMediaView(
                    model: recentMediaModel,
                    selectedMediaIds: $selectedMediaIds,
                    disabledMediaIds: alreadyAddedMediaIds
                )
            case .albums:
                NavigationStack(path: $albumPath) {
                    AddMediaAlbumBrowser(
                        album: nil,
                        targetAlbumName: targetAlbum.name,
                        path: $albumPath,
                        selectedMediaIds: $selectedMediaIds,
                        disabledMediaIds: alreadyAddedMediaIds
                    )
                    .navigationTitle("")
                    .hideSheetNavigationBar()
                    .navigationDestination(for: FfiAlbum.self) { album in
                        AddMediaAlbumBrowser(
                            album: album,
                            targetAlbumName: targetAlbum.name,
                            path: $albumPath,
                            selectedMediaIds: $selectedMediaIds,
                            disabledMediaIds: alreadyAddedMediaIds
                        )
                        .navigationBarBackButtonHidden(true)
                        .navigationTitle("")
                        .hideSheetNavigationBar()
                    }
                }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) { confirmBar }
        .background(theme.bg)
        .task { await recentMediaModel.start() }
        .task {
            let existingMedia = (try? await repository.mediaInAlbum(albumID: targetAlbum.albumId)) ?? []
            alreadyAddedMediaIds = Set(existingMedia.map(\.mediaId))
        }
        #if os(macOS)
        .frame(minWidth: 640, minHeight: 520)
        #endif
    }

    private var confirmBar: some View {
        HStack(spacing: 12) {
            Text(selectedMediaIds.isEmpty ? "No items selected" : "\(selectedMediaIds.count) selected")
                .font(LascoFont.body())
                .foregroundStyle(theme.inkMuted)
            Spacer()
            Button("Cancel", action: dismiss.callAsFunction)
                .buttonStyle(LascoGhostButtonStyle())
            Button("Add \(selectedMediaIds.count)", action: addSelectedMedia)
                .buttonStyle(LascoPrimaryButtonStyle())
                .frame(maxWidth: 140)
                .disabled(selectedMediaIds.isEmpty)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .background(theme.surfaceAlt)
        .overlay(Rectangle().stroke(theme.ink, lineWidth: 2).ignoresSafeArea(edges: .bottom))
    }

    private func addSelectedMedia() {
        let mediaIds = selectedMediaIds
        Task {
            do {
                for mediaId in mediaIds {
                    try await albumModel.addMedia(mediaID: mediaId, albumID: targetAlbum.albumId)
                }
                toastManager.show(ok: "Added \(mediaIds.count) item(s) to \(targetAlbum.name)")
                dismiss()
            } catch {
                toastManager.show(error: error.localizedDescription)
            }
        }
    }
}

private struct AddMediaPickerHeader: View {
    @Binding var selectedTab: AddMediaPickerTab
    @Environment(\.lascoTheme) private var theme

    var body: some View {
        Picker("Source", selection: $selectedTab) {
            Text("All media").tag(AddMediaPickerTab.allMedia)
            Text("Albums").tag(AddMediaPickerTab.albums)
        }
        .pickerStyle(.segmented)
        .padding(.horizontal, 20)
        .padding(.top, 20)
        .padding(.bottom, 12)
        .background(theme.bg)
        .accessibilityLabel("Media source")
    }
}

struct AddMediaAlbumBrowser: View {
    @Environment(AlbumListModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) private var theme
    let album: FfiAlbum?
    let targetAlbumName: String
    @Binding var path: [FfiAlbum]
    @Binding var selectedMediaIds: Set<FfiMediaUuid>
    let disabledMediaIds: Set<FfiMediaUuid>

    @State private var albumMedia: [FfiMediaItem] = []
    @State private var mediaLayout: MediaLayout = .grid

    private var isRoot: Bool { album == nil }
    private var title: String { album?.name.uppercased() ?? "ALL ALBUMS" }
    private var childAlbums: [FfiAlbum] { model.albums(parentID: album?.albumId).filter { !$0.deleted } }

    var body: some View {
        GeometryReader { geo in
            let columns = geo.size.width > 500 ? 3 : 2
            let albumColumns = Array(repeating: GridItem(.flexible(), spacing: 12), count: columns)
            let mediaColumns = Array(repeating: GridItem(.flexible(), spacing: 3), count: columns)

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    pickerTitle
                    if !childAlbums.isEmpty {
                        LazyVGrid(columns: albumColumns, spacing: 12) {
                            ForEach(childAlbums, id: \.albumId) { child in
                                NavigationLink(value: child) { AlbumCell(album: child) }
                                    .buttonStyle(.plain)
                            }
                        }
                    }
                    if !albumMedia.isEmpty { mediaSection(columns: mediaColumns, hasChildAlbums: !childAlbums.isEmpty) }
                    if childAlbums.isEmpty && albumMedia.isEmpty {
                        Text("No media in this album.")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkMuted)
                            .padding(20)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .lascoPanel()
                    }
                    Spacer(minLength: 40)
                }
                .padding(.horizontal, 20)
            }
        }
        .task(id: album?.albumId) {
            await model.load(parentID: album?.albumId)
            if let album {
                albumMedia = await model.mediaInAlbum(albumID: album.albumId)
            } else {
                albumMedia = []
            }
        }
        .background(theme.bg)
    }

    private var pickerTitle: some View {
        VStack(alignment: .leading, spacing: 4) {
            if !isRoot { LascoBackButton(action: dismiss.callAsFunction) }
            HStack(alignment: .firstTextBaseline) {
                Text(title).font(LascoFont.categoryLarge()).foregroundStyle(theme.ink)
                Spacer()
                Text("Select media to add to \(targetAlbumName)")
                    .font(LascoFont.subtitle())
                    .foregroundStyle(theme.inkMuted)
            }
        }
        .padding(.top, 8)
    }

    @ViewBuilder
    private func mediaSection(columns: [GridItem], hasChildAlbums: Bool) -> some View {
        HStack {
            if hasChildAlbums { Text("MEDIA").font(LascoFont.categoryLarge()).foregroundStyle(theme.ink) }
            Spacer()
            layoutToggle
        }
        switch mediaLayout {
        case .grid:
            LazyVGrid(columns: columns, spacing: 3) {
                ForEach(albumMedia, id: \.mediaId) { item in
                    AddableMediaCell(item: item, isSelected: selectedMediaIds.contains(item.mediaId), isDisabled: disabledMediaIds.contains(item.mediaId)) { toggle(item.mediaId) }
                }
            }
        case .list:
            VStack(spacing: 0) {
                ForEach(albumMedia, id: \.mediaId) { item in
                    AddableMediaCell(item: item, isSelected: selectedMediaIds.contains(item.mediaId), isDisabled: disabledMediaIds.contains(item.mediaId), layout: .list) { toggle(item.mediaId) }
                    if item.mediaId != albumMedia.last?.mediaId { Divider().background(theme.bgDeep) }
                }
            }
            .lascoPanel()
        }
    }

    private var layoutToggle: some View {
        HStack(spacing: 2) {
            Button("List", systemImage: "list.bullet") { mediaLayout = .list }
                .labelStyle(.iconOnly).buttonStyle(.plain)
                .foregroundStyle(mediaLayout == .list ? theme.ink : theme.inkMuted)
            Button("Grid", systemImage: "square.grid.2x2") { mediaLayout = .grid }
                .labelStyle(.iconOnly).buttonStyle(.plain)
                .foregroundStyle(mediaLayout == .grid ? theme.ink : theme.inkMuted)
        }
    }

    private func toggle(_ mediaID: FfiMediaUuid) {
        guard !disabledMediaIds.contains(mediaID) else { return }
        if selectedMediaIds.contains(mediaID) { selectedMediaIds.remove(mediaID) }
        else { selectedMediaIds.insert(mediaID) }
    }
}

struct AddableMediaCell: View {
    enum Layout { case grid, list }
    let item: FfiMediaItem
    let isSelected: Bool
    let isDisabled: Bool
    var layout: Layout = .grid
    let toggleSelection: () -> Void
    @Environment(\.lascoTheme) private var theme

    var body: some View {
        Button(action: toggleSelection) {
            Group {
                switch layout {
                case .grid: MediaGridCell(item: item, isSelected: isSelected)
                case .list: MediaRow(item: item, isSelected: isSelected)
                }
            }
            .overlay { if isDisabled { theme.bg.opacity(0.58) } }
        }
        .buttonStyle(.plain)
        .disabled(isDisabled)
        .accessibilityLabel(isDisabled ? "Already in album" : "Select media")
        .accessibilityValue(isSelected ? "Selected" : "Not selected")
    }
}
