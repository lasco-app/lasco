import SwiftUI

struct TrashView: View {
    let repository: LibraryRepository
    @State private var model: TrashMediaModel
    @State private var showingEmptyTrashConfirm = false
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
                    Button("Delete All", role: .destructive) {
                        showingEmptyTrashConfirm = true
                    }
                    .buttonStyle(.plain)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.error)
                    .frame(minHeight: 44)
                    .disabled(model.media.isEmpty)
                    .confirmationDialog(
                        "Permanently delete all items in Trash?",
                        isPresented: $showingEmptyTrashConfirm,
                        titleVisibility: .visible,
                    ) {
                        Button("Delete All", role: .destructive) {
                            model.emptyTrash()
                        }
                        Button("Cancel", role: .cancel) {}
                    } message: {
                        Text("This can't be undone.")
                    }
                }

                if model.media.isEmpty && !model.isLoading {
                    ContentUnavailableView("Trash is empty", systemImage: "trash")
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 48)
                } else {
                    LazyVStack(spacing: 12) {
                        ForEach(model.media, id: \.mediaId) { item in
                            TrashMediaRow(
                                item: item,
                                onRestore: { model.restore(item.mediaId) },
                                onDelete: { model.hardDelete(item.mediaId) }
                            )
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
