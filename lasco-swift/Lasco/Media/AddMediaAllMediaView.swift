import SwiftUI

struct AddMediaAllMediaView: View {
    @Bindable var model: RecentMediaModel
    @Binding var selectedMediaIds: Set<FfiMediaUuid>
    let disabledMediaIds: Set<FfiMediaUuid>
    @Environment(\.lascoTheme) private var theme

    var body: some View {
        GeometryReader { geo in
            let columns = Array(repeating: GridItem(.flexible(), spacing: 3), count: geo.size.width > 500 ? 3 : 2)
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Toggle(isOn: $model.showingOrphans) {
                        Text(model.showingOrphans ? "Orphan" : "All")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkSub)
                    }
                    .accessibilityLabel("Media filter")
                    .accessibilityValue(model.showingOrphans ? "Orphan media" : "All media")

                    if model.media.isEmpty {
                        Text(model.showingOrphans ? "No orphan media." : "No media yet.")
                            .font(LascoFont.body()).foregroundStyle(theme.inkMuted)
                    } else {
                        LazyVGrid(columns: columns, spacing: 3) {
                            ForEach(model.media, id: \.mediaId) { item in
                                AddableMediaCell(item: item, isSelected: selectedMediaIds.contains(item.mediaId), isDisabled: disabledMediaIds.contains(item.mediaId)) { toggle(item.mediaId) }
                                    .onAppear {
                                        guard item.mediaId == model.media.last?.mediaId else { return }
                                        Task { await model.loadMore() }
                                    }
                            }
                        }
                    }
                    Spacer(minLength: 40)
                }
                .padding(.horizontal, 20)
                .padding(.top, 8)
            }
        }
        .background(theme.bg)
        .onChange(of: model.showingOrphans) { _, _ in Task { await model.load() } }
    }

    private func toggle(_ mediaID: FfiMediaUuid) {
        guard !disabledMediaIds.contains(mediaID) else { return }
        if selectedMediaIds.contains(mediaID) { selectedMediaIds.remove(mediaID) }
        else { selectedMediaIds.insert(mediaID) }
    }
}
