import SwiftUI

struct OperationsView: View {
    @EnvironmentObject var libraryModel: LibraryModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme

    private var groups: [FfiOperationGroup] {
        libraryModel.listOperationGroups().reversed()
    }

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text("Operations")
                        .font(LascoFont.title())
                        .foregroundStyle(theme.ink)
                    Spacer()
                    Button("Done") { dismiss() }
                        .font(LascoFont.body(14))
                        .foregroundStyle(theme.inkMuted)
                        .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.top, 40)
                .padding(.bottom, 16)

                if groups.isEmpty {
                    VStack {
                        Spacer()
                        Text("No operations yet.")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkMuted)
                        Spacer()
                    }
                    .frame(maxWidth: .infinity)
                } else {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 12) {
                            ForEach(groups, id: \.opId) { group in
                                OperationGroupRow(group: group)
                            }
                        }
                        .padding(.horizontal, 20)
                        .padding(.bottom, 40)
                    }
                }
            }
        }
    }
}

private struct OperationGroupRow: View {
    let group: FfiOperationGroup
    @Environment(\.lascoTheme) var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(group.opId.prefix(12))
                        .font(LascoFont.mono())
                        .foregroundStyle(theme.ink)
                    if let parentId = group.parentOpId {
                        HStack(spacing: 4) {
                            Text("↑")
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.inkMuted)
                            Text(parentId.prefix(12))
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.inkMuted)
                        }
                    }
                }
                Spacer()
                Text(group.author)
                    .font(LascoFont.mono())
                    .foregroundStyle(theme.inkMuted)
            }

            ForEach(Array(group.operations.enumerated()), id: \.offset) { _, op in
                OperationRow(operation: op)
            }
        }
        .padding(14)
        .lascoPanel()
    }
}

private struct OperationRow: View {
    let operation: FfiOperation
    @Environment(\.lascoTheme) var theme

    private var formattedTimestamp: String {
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let iso2 = ISO8601DateFormatter()
        iso2.formatOptions = [.withInternetDateTime]
        let date = iso.date(from: operation.timestamp) ?? iso2.date(from: operation.timestamp)
        guard let date else { return operation.timestamp }
        let f = DateFormatter()
        f.dateStyle = .short
        f.timeStyle = .medium
        return f.string(from: date)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(operation.kind)
                    .font(LascoFont.mono().bold())
                    .foregroundStyle(theme.ink)
                Spacer()
                Text(formattedTimestamp)
                    .font(LascoFont.mono())
                    .foregroundStyle(theme.inkMuted)
                    .lineLimit(1)
            }

            ForEach(operation.args, id: \.key) { arg in
                HStack(alignment: .top, spacing: 6) {
                    Text(arg.key)
                        .font(LascoFont.mono())
                        .foregroundStyle(theme.inkMuted)
                        .frame(minWidth: 80, alignment: .leading)
                    Text(arg.value.isEmpty ? "—" : arg.value)
                        .font(LascoFont.mono())
                        .foregroundStyle(theme.inkSub)
                        .lineLimit(2)
                        .truncationMode(.middle)
                }
            }
        }
        .padding(.vertical, 4)
    }
}
