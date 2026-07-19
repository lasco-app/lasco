import SwiftUI
#if os(iOS)
import Photos

struct IgnoredAssetsView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme

    let ignoredAssets: [PhotoLibraryImporter.IgnoredAsset]

    private struct Group {
        let label: String
        let assets: [PhotoLibraryImporter.IgnoredAsset]
    }

    private var groups: [Group] {
        let byType = Dictionary(grouping: ignoredAssets, by: \.mediaType)
        return byType
            .map { Group(label: Self.label(for: $0.key), assets: $0.value) }
            .sorted { $0.assets.count > $1.assets.count }
    }

    private static func label(for mediaType: PHAssetMediaType) -> String {
        switch mediaType {
        case .audio: return "Audio"
        case .image: return "Photo"
        case .video: return "Video"
        default: return "Unknown"
        }
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            theme.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                HStack {
                    Spacer()
                    Button(action: { dismiss() }) {
                        Image("times").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(theme.ink)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 16)

                Text("Ignored items")
                    .font(LascoFont.title(26))
                    .foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 8)

                Text("These items have no photo or video content Lasco can import, so they are skipped.")
                    .font(LascoFont.body(16))
                    .foregroundStyle(theme.inkSub)
                    .fixedSize(horizontal: false, vertical: true)
                    .lineSpacing(4)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 20)

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        ForEach(groups, id: \.label) { group in
                            VStack(alignment: .leading, spacing: 0) {
                                Text("\(group.label) (\(group.assets.count))")
                                    .font(LascoFont.body(14))
                                    .foregroundStyle(theme.inkSub)
                                    .padding(.horizontal, 16)
                                    .padding(.top, 12)
                                    .padding(.bottom, 8)

                                ForEach(group.assets, id: \.localIdentifier) { asset in
                                    HStack {
                                        Text(dateLabel(for: asset.creationDate))
                                            .font(LascoFont.mono(13))
                                            .foregroundStyle(theme.inkMuted)
                                        Spacer()
                                    }
                                    .padding(.horizontal, 16)
                                    .padding(.vertical, 8)
                                }
                            }
                            .lascoPanel()
                        }

                        Spacer().frame(height: 40)
                    }
                    .padding(.horizontal, 32)
                }
            }
        }
    }

    private func dateLabel(for date: Date?) -> String {
        guard let date else { return "Unknown date" }
        return date.formatted(date: .abbreviated, time: .shortened)
    }
}
#endif
