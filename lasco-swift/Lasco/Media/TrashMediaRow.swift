import SwiftUI

struct TrashMediaRow: View {
    let item: FfiMediaItem
    let onRestore: () -> Void
    let onDelete: () -> Void
    @State private var showingDeleteConfirm = false
    @Environment(\.lascoTheme) private var theme

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            MediaGridCell(item: item)
                .frame(width: 88, height: 88)
                .clipped()

            VStack(alignment: .leading, spacing: 4) {
                Text(item.name ?? item.filenameOriginal)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
                    .lineLimit(2)

                Text(item.trashedBy.map { "Trashed by \($0)" } ?? "Trashed")
                    .font(LascoFont.pixel())
                    .foregroundStyle(theme.inkMuted)

                if let trashedAt = item.trashedAt {
                    Text(formatTrashTimestamp(trashedAt))
                        .font(LascoFont.pixel())
                        .foregroundStyle(theme.inkMuted)
                }

                HStack(spacing: 16) {
                    Button("Restore", action: onRestore)
                        .buttonStyle(.plain)
                        .font(LascoFont.body())
                        .foregroundStyle(theme.ink)
                        .frame(minHeight: 44)

                    Button("Delete", role: .destructive) {
                        showingDeleteConfirm = true
                    }
                        .buttonStyle(.plain)
                        .font(LascoFont.body())
                        .foregroundStyle(theme.error)
                        .frame(minHeight: 44)
                        .confirmationDialog(
                            "Permanently delete this item?",
                            isPresented: $showingDeleteConfirm,
                            titleVisibility: .visible,
                        ) {
                            Button("Delete", role: .destructive, action: onDelete)
                            Button("Cancel", role: .cancel) {}
                        } message: {
                            Text("This can't be undone.")
                        }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(8)
        .background(theme.surfaceAlt)
    }
}

private let trashTimestampFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateStyle = .medium
    formatter.timeStyle = .short
    return formatter
}()

private func formatTrashTimestamp(_ timestamp: String) -> String {
    guard let date = iso8601Formatter.date(from: timestamp) ?? iso8601FormatterNoFrac.date(from: timestamp) else {
        return timestamp
    }
    return trashTimestampFormatter.string(from: date)
}
