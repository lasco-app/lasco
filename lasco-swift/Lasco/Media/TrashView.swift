import SwiftUI

struct TrashView: View {
    let repository: LibraryRepository
    @State private var model: TrashMediaModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) private var theme

    init(repository: LibraryRepository) {
        self.repository = repository
        _model = State(initialValue: TrashMediaModel(repository: repository))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    LascoBackButton(action: { dismiss() })
                    Text("TRASH")
                        .font(LascoFont.categoryLarge())
                        .foregroundStyle(theme.ink)
                    Spacer()
                }

                if model.media.isEmpty && !model.isLoading {
                    ContentUnavailableView("Trash is empty", systemImage: "trash")
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 48)
                } else {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), spacing: 12)], spacing: 12) {
                        ForEach(model.media, id: \.mediaId) { item in
                            VStack(spacing: 0) {
                                MediaGridCell(item: item)
                                Button("Restore") { model.restore(item.mediaId) }
                                    .buttonStyle(.plain)
                                    .font(LascoFont.body())
                                    .foregroundStyle(theme.ink)
                                    .frame(maxWidth: .infinity, minHeight: 44)
                                    .background(theme.surfaceAlt)
                                    .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
                            }
                            .onAppear {
                                guard item.mediaId == model.media.last?.mediaId else { return }
                                Task { await model.loadMore() }
                            }
                        }
                    }
                }
            }
            .padding(20)
        }
        .background(theme.bg)
        .task { await model.start() }
    }
}
