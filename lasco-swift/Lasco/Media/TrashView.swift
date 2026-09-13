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
                    LazyVStack(spacing: 12) {
                        ForEach(model.media, id: \.mediaId) { item in
                            TrashMediaRow(item: item, onRestore: { model.restore(item.mediaId) })
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
