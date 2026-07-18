import SwiftUI

struct AlbumPickerView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    let title: String
    let onSelect: (FfiAlbum) -> Void
    let onCancel: () -> Void

    @State private var path: [FfiAlbum] = []
    @Environment(\.lascoTheme) var theme

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
        #if os(macOS)
        .frame(minWidth: 480, minHeight: 400)
        #endif
    }
}

private struct AlbumPickerBrowser: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    let album: FfiAlbum?
    @Binding var path: [FfiAlbum]
    let onSelect: (FfiAlbum) -> Void

    private var isRoot: Bool { album == nil }

    private var childAlbums: [FfiAlbum] {
        libraryModel.albums.filter {
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
