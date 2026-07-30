import SwiftUI

struct AlbumPickerView: View {
    let repository: LibraryRepository
    let title: String
    let onSelect: (FfiAlbum) -> Void
    let onCancel: () -> Void

    @State private var path: [FfiAlbum] = []
    @State private var model: AlbumListModel
    @Environment(\.lascoTheme) var theme

    init(repository: LibraryRepository, title: String, onSelect: @escaping (FfiAlbum) -> Void, onCancel: @escaping () -> Void) {
        self.repository = repository
        self.title = title
        self.onSelect = onSelect
        self.onCancel = onCancel
        _model = State(initialValue: AlbumListModel(repository: repository))
    }

    var body: some View {
        VStack(spacing: 0) {
            NavigationStack(path: $path) {
                AlbumPickerBrowser(album: nil, path: $path, onSelect: onSelect)
                    .navigationTitle("")
                    .hideSheetNavigationBar()
                    .navigationDestination(for: FfiAlbum.self) { album in
                        AlbumPickerBrowser(album: album, path: $path, onSelect: onSelect)
                            .navigationBarBackButtonHidden(true)
                            .navigationTitle("")
                            .hideSheetNavigationBar()
                    }
            }
            .background(theme.bg)

            HStack(spacing: 12) {
                if let current = path.last {
                    Button("Import to \"\(current.name)\"") {
                        onSelect(current)
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                }
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
        .environment(model)
        .task { await model.start() }
        #if os(macOS)
        .frame(minWidth: 480, minHeight: 400)
        #endif
    }
}

private struct AlbumPickerBrowser: View {
    @Environment(AlbumListModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    let album: FfiAlbum?
    @Binding var path: [FfiAlbum]
    let onSelect: (FfiAlbum) -> Void

    private var isRoot: Bool { album == nil }

    private var childAlbums: [FfiAlbum] {
        model.albums.filter {
            $0.parentAlbumId == album?.albumId && !$0.deleted && !$0.isDisconnected
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    if !isRoot {
                        LascoBackButton(action: { dismiss() })
                    }
                    HStack(alignment: .firstTextBaseline) {
                        Text(album?.name.uppercased() ?? "ALL ALBUMS")
                            .font(LascoFont.categoryLarge())
                            .foregroundStyle(theme.ink)
                        Spacer()
                        Text("Choose a destination album")
                            .font(LascoFont.subtitle())
                            .foregroundStyle(theme.inkMuted)
                    }
                }
                .padding(.top, 20)

                if childAlbums.isEmpty {
                    Text("No sub-albums.")
                        .font(LascoFont.body())
                        .foregroundStyle(theme.inkMuted)
                        .padding(20)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .lascoPanel()
                } else {
                    VStack(spacing: 0) {
                        ForEach(childAlbums, id: \.albumId) { child in
                            albumRow(child)
                            if child.albumId != childAlbums.last?.albumId {
                                Divider().background(theme.bgDeep)
                            }
                        }
                    }
                    .lascoPanel()
                }

                Spacer(minLength: 40)
            }
            .padding(.horizontal, 20)
        }
        .background(theme.bg)
    }

    @ViewBuilder
    private func albumRow(_ child: FfiAlbum) -> some View {
        Button {
            path.append(child)
        } label: {
            HStack(spacing: 10) {
                Image("folder").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 15))
                    .foregroundStyle(theme.inkMuted)
                    .frame(width: 20)
                Text(child.name)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                Spacer()
                Text("\(child.mediaCount)")
                    .font(LascoFont.pixel())
                    .foregroundStyle(theme.inkMuted)
                Image("angle-right").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(theme.inkMuted)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
