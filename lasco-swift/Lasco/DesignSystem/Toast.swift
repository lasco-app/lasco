import SwiftUI
import Observation

@Observable
class ToastManager {
    enum Kind { case ok, error }

    struct Message: Identifiable {
        let id = UUID()
        let kind: Kind
        let text: String
    }

    var current: Message? = nil

    func show(ok text: String) { post(Message(kind: .ok, text: text)) }
    func show(error text: String) { post(Message(kind: .error, text: text)) }

    private func post(_ msg: Message) {
        current = msg
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 3_000_000_000)
            if self.current?.id == msg.id { self.current = nil }
        }
    }
}

private struct ToastView: View {
    let message: ToastManager.Message
    @Environment(\.lascoTheme) var theme

    var color: Color {
        message.kind == .ok ? theme.ok : theme.error
    }
    var glyph: String {
        message.kind == .ok ? "✓" : "✗"
    }

    var body: some View {
        HStack(spacing: 10) {
            Text(glyph)
                .font(LascoFont.mono())
                .foregroundStyle(color)
            Text(message.text)
                .font(LascoFont.body(13))
                .foregroundStyle(color)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(theme.surface)
        .overlay(Rectangle().stroke(color, lineWidth: 2))
        .shadow(color: color.opacity(0.25), radius: 0, x: 3, y: 3)
    }
}

extension View {
    func toastOverlay(_ manager: ToastManager) -> some View {
        self.overlay(alignment: .bottom) {
            if let msg = manager.current {
                ToastView(message: msg)
                    .padding(.bottom, 32)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .animation(.easeOut(duration: 0.2), value: manager.current?.id)
                    .zIndex(999)
            }
        }
        .animation(.easeOut(duration: 0.2), value: manager.current?.id)
    }
}
