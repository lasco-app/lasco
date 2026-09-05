import SwiftUI

/// A stable SwiftUI root for one absolute pager position. The hosting
/// controller keeps this view alive while observation updates only its content.
struct MediaGalleryPageHost<Content: View>: View {
    let gallery: MediaGallerySession
    let position: Int
    private let content: (AlbumItem) -> Content

    init(
        gallery: MediaGallerySession,
        position: Int,
        @ViewBuilder content: @escaping (AlbumItem) -> Content
    ) {
        self.gallery = gallery
        self.position = position
        self.content = content
    }

    var body: some View {
        Group {
            switch gallery.state(at: position) {
            case .loaded(let item):
                content(item)
            case .loading:
                Color.black
                    .overlay {
                        ProgressView()
                            .tint(.white)
                            .accessibilityLabel("Loading media")
                    }
                    .task(id: position) {
                        await gallery.ensureLoaded(at: position)
                    }
            case .failed(let message):
                VStack(spacing: 12) {
                    Text("Could not load this item")
                        .foregroundStyle(.white)
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(Color.white.opacity(0.6))
                        .multilineTextAlignment(.center)
                    Button("Retry") { gallery.retry(position: position) }
                        .buttonStyle(.bordered)
                        .tint(.white)
                }
                .padding(24)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.black)
            }
        }
    }
}
