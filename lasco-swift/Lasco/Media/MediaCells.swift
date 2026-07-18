import SwiftUI

enum MediaLayout { case list, grid }

// MARK: - MediaRow

struct MediaRow: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.lascoTheme) var theme
    let item: FfiMediaItem
    var isSelected: Bool = false
    @State private var thumbnail: Image? = nil

    private var formattedSize: String {
        let mb = Double(item.sizeBytes) / 1_048_576
        if mb < 1 { return String(format: "%.0f KB", Double(item.sizeBytes) / 1024) }
        return String(format: "%.1f MB", mb)
    }

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                if let thumbnail {
                    thumbnail
                        .resizable()
                        .scaledToFill()
                        .frame(width: 44, height: 44)
                        .clipped()
                } else {
                    theme.bgDeep
                        .frame(width: 44, height: 44)
                    Image("image").renderingMode(.template).resizable().frame(width: 28, height: 28)
                        .font(.system(size: 16))
                        .foregroundStyle(theme.inkMuted)
                }
                if isSelected {
                    theme.pink.opacity(0.5)
                    Image("check-circle-solid").renderingMode(.template).resizable().frame(width: 18, height: 18)
                        .font(.system(size: 16))
                        .foregroundStyle(theme.pink)
                }
            }
            .frame(width: 44, height: 44)
            .clipped()

            VStack(alignment: .leading, spacing: 2) {
                Text(item.name ?? "")
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                    .lineLimit(1)
                Text(formatMediaDate(item.date))
                    .font(LascoFont.pixel())
                    .foregroundStyle(theme.inkMuted)
            }

            Spacer()

            Text(formattedSize)
                .font(LascoFont.pixel())
                .foregroundStyle(theme.inkMuted)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(isSelected ? theme.bgDeep : theme.bg)
        .contentShape(Rectangle())
        .task(id: item.mediaId) {
            guard thumbnail == nil else { return }
            if let data = await libraryModel.thumbnailAsync(for: item.mediaId) {
                thumbnail = Image(data: data)
            }
        }
    }
}

// MARK: - GroupListRow

struct GroupListRow: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.lascoTheme) var theme
    let group: FfiGroup
    var isSelected: Bool = false
    @State private var thumbnail: Image? = nil

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                theme.bgDeep.frame(width: 44, height: 44)
                if let thumbnail {
                    thumbnail.resizable().scaledToFill().frame(width: 44, height: 44).clipped()
                } else {
                    Image("image").renderingMode(.template).resizable().frame(width: 28, height: 28)
                        .foregroundStyle(theme.inkMuted)
                }
                Text("G")
                    .font(LascoFont.pixel())
                    .foregroundStyle(theme.bg)
                    .padding(.horizontal, 3)
                    .padding(.vertical, 1)
                    .background(theme.pink)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .padding(2)
            }
            .frame(width: 44, height: 44)
            .clipped()

            VStack(alignment: .leading, spacing: 2) {
                Text("Group")
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                    .lineLimit(1)
                Text("\(group.mediaIds.count) item\(group.mediaIds.count == 1 ? "" : "s")")
                    .font(LascoFont.pixel())
                    .foregroundStyle(theme.inkMuted)
            }
            Spacer()

            if isSelected {
                Image("check-circle-solid").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .foregroundStyle(theme.pink)
                    .padding(.trailing, 4)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(theme.bg)
        .contentShape(Rectangle())
        .task(id: group.groupId) {
            thumbnail = nil
            if let firstId = group.mediaIds.first,
               let data = await libraryModel.thumbnailAsync(for: firstId) {
                thumbnail = Image(data: data)
            }
        }
    }
}

// MARK: - GroupGridCell

struct GroupGridCell: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.lascoTheme) var theme
    let group: FfiGroup
    var isSelected: Bool = false
    @State private var thumbnail: Image? = nil

    var body: some View {
        ZStack(alignment: .topLeading) {
            theme.bgDeep
                .aspectRatio(1, contentMode: .fit)
                .overlay {
                    if let thumbnail {
                        thumbnail
                            .resizable()
                            .scaledToFill()
                    } else {
                        Image("image").renderingMode(.template).resizable().frame(width: 28, height: 28)
                            .font(.system(size: 28))
                            .foregroundStyle(theme.inkMuted)
                    }
                }
                .clipped()

            Text("G")
                .font(LascoFont.pixel())
                .foregroundStyle(theme.bg)
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(theme.pink)
                .padding(6)

            if isSelected {
                Image("check-circle-solid").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .foregroundStyle(theme.pink)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                    .padding(6)
            }
        }
        .contentShape(Rectangle())
        .task(id: group.groupId) {
            thumbnail = nil
            if let firstId = group.mediaIds.first,
               let data = await libraryModel.thumbnailAsync(for: firstId) {
                thumbnail = Image(data: data)
            }
        }
    }
}

// MARK: - MediaGridCell

struct MediaGridCell: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.lascoTheme) var theme
    let item: FfiMediaItem
    var isSelected: Bool = false
    @State private var thumbnail: Image? = nil

    var body: some View {
        ZStack(alignment: .topTrailing) {
            theme.bgDeep
                .aspectRatio(1, contentMode: .fit)
                .overlay {
                    if let thumbnail {
                        thumbnail
                            .resizable()
                            .scaledToFill()
                    } else {
                        Image("image").renderingMode(.template).resizable().frame(width: 28, height: 28)
                            .font(.system(size: 28))
                            .foregroundStyle(theme.inkMuted)
                    }
                }
                .clipped()

            if isSelected {
                Image("check-circle-solid").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 20))
                    .foregroundStyle(theme.pink)
                    .padding(6)
            }
        }
        .contentShape(Rectangle())
        .task(id: item.mediaId) {
            guard thumbnail == nil else { return }
            if let data = await libraryModel.thumbnailAsync(for: item.mediaId) {
                thumbnail = Image(data: data)
            }
        }
    }
}
