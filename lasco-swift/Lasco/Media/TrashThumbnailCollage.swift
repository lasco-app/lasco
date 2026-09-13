import SwiftUI

struct TrashThumbnailCollage: View {
    let repository: LibraryRepository
    @Environment(\.lascoTheme) private var theme
    @State private var media: [FfiMediaItem] = []

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 1), count: 3)

    var body: some View {
        LazyVGrid(columns: columns, spacing: 1) {
            ForEach(media, id: \.mediaId) { item in
                MediaGridCell(item: item)
            }
            ForEach(0..<Swift.max(0, 9 - media.count), id: \.self) { _ in
                theme.bgDeep
                    .aspectRatio(1, contentMode: .fit)
            }
        }
        .onAppear {
            Task { await loadMedia() }
        }
        .accessibilityHidden(true)
    }

    private func loadMedia() async {
        media = (try? await repository.trashedMediaByDate(offset: 0, limit: 9)) ?? []
    }
}
