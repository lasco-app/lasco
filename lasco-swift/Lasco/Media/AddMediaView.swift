import SwiftUI

struct AddMediaView: View {
    @Environment(AlbumListModel.self) private var model
    let targetAlbum: FfiAlbum?
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    @State private var selectedMediaIds: Set<FfiMediaUuid> = []
    @State private var path: [FfiAlbum] = []

    var body: some View {
        NavigationStack(path: $path) {
            AddMediaAlbumBrowser(album: nil, targetAlbumName: targetAlbum?.name ?? "album", path: $path, selectedMediaIds: $selectedMediaIds)
                .navigationTitle("")
                .hideSheetNavigationBar()
                .navigationDestination(for: FfiAlbum.self) { album in
                    AddMediaAlbumBrowser(album: album, targetAlbumName: targetAlbum?.name ?? "album", path: $path, selectedMediaIds: $selectedMediaIds)
                        .navigationBarBackButtonHidden(true)
                        .navigationTitle("")
                        .hideSheetNavigationBar()
                }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            confirmBar
        }
        .background(theme.bg)
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

            Button("Cancel") { dismiss() }
                .buttonStyle(LascoGhostButtonStyle())

            if !selectedMediaIds.isEmpty {
                Button("Add \(selectedMediaIds.count)") {
                    if let albumId = targetAlbum?.albumId {
                        for mediaId in selectedMediaIds {
                            Task { try? await model.addMedia(mediaID: mediaId, albumID: albumId) }
                        }
                    }
                    dismiss()
                }
                .buttonStyle(LascoPrimaryButtonStyle())
                .frame(maxWidth: 140)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .background(theme.surfaceAlt)
        .overlay(Rectangle().stroke(theme.ink, lineWidth: 2).ignoresSafeArea(edges: .bottom))
    }
}

struct AddMediaAlbumBrowser: View {
    @Environment(AlbumListModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    let album: FfiAlbum?
    let targetAlbumName: String
    @Binding var path: [FfiAlbum]
    @Binding var selectedMediaIds: Set<FfiMediaUuid>

    @State private var albumMedia: [FfiMediaItem] = []
    @State private var mediaLayout: MediaLayout = .grid

    private var isRoot: Bool { album == nil }
    private var title: String { album?.name.uppercased() ?? "ALL ALBUMS" }

    private var childAlbums: [FfiAlbum] {
        model.albums(parentID: album?.albumId).filter { !$0.deleted }
    }

    var body: some View {
        GeometryReader { geo in
            let columns = geo.size.width > 500 ? 3 : 2
            let gridColumns = Array(repeating: GridItem(.flexible(), spacing: 12), count: columns)
            let mediaGridColumns = Array(repeating: GridItem(.flexible(), spacing: 3), count: columns)

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 4) {
                        if !isRoot {
                            LascoBackButton(action: { dismiss() })
                        }
                        HStack(alignment: .firstTextBaseline) {
                            Text(title)
                                .font(LascoFont.categoryLarge())
                                .foregroundStyle(theme.ink)
                            Spacer()
                            Text("Select media to add to \(targetAlbumName)")
                                .font(LascoFont.subtitle())
                                .foregroundStyle(theme.inkMuted)
                        }
                    }
                    .padding(.top, 20)

                    if !childAlbums.isEmpty {
                        LazyVGrid(columns: gridColumns, spacing: 12) {
                            ForEach(childAlbums, id: \.albumId) { child in
                                NavigationLink(value: child) {
                                    AlbumCell(album: child)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }

                    if !albumMedia.isEmpty {
                        if !childAlbums.isEmpty {
                            HStack {
                                Text("MEDIA")
                                    .font(LascoFont.categoryLarge())
                                    .foregroundStyle(theme.ink)
                                Spacer()
                                layoutToggle
                            }
                        } else {
                            HStack {
                                Spacer()
                                layoutToggle
                            }
                        }

                        switch mediaLayout {
                        case .grid:
                            LazyVGrid(columns: mediaGridColumns, spacing: 3) {
                                ForEach(albumMedia, id: \.mediaId) { item in
                                    MediaGridCell(item: item, isSelected: selectedMediaIds.contains(item.mediaId))
                                        .onTapGesture {
                                            if selectedMediaIds.contains(item.mediaId) { selectedMediaIds.remove(item.mediaId) }
                                            else { selectedMediaIds.insert(item.mediaId) }
                                        }
                                }
                            }
                        case .list:
                            VStack(spacing: 0) {
                                ForEach(albumMedia, id: \.mediaId) { item in
                                    MediaRow(item: item, isSelected: selectedMediaIds.contains(item.mediaId))
                                        .onTapGesture {
                                            if selectedMediaIds.contains(item.mediaId) { selectedMediaIds.remove(item.mediaId) }
                                            else { selectedMediaIds.insert(item.mediaId) }
                                        }
                                    if item.mediaId != albumMedia.last?.mediaId {
                                        Divider().background(theme.bgDeep)
                                    }
                                }
                            }
                            .lascoPanel()
                        }
                    }

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
        }
        .background(theme.bg)
        .onAppear {
            if let album {
                Task { albumMedia = await model.mediaInAlbum(albumID: album.albumId) }
            }
        }
    }

    private var layoutToggle: some View {
        HStack(spacing: 2) {
            Button { mediaLayout = .list } label: {
                Image(mediaLayout == .list ? "bullet-list-solid" : "bullet-list").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .foregroundStyle(mediaLayout == .list ? theme.ink : theme.inkMuted)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Button { mediaLayout = .grid } label: {
                Image(mediaLayout == .grid ? "grid-solid" : "grid").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .foregroundStyle(mediaLayout == .grid ? theme.ink : theme.inkMuted)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }
}
