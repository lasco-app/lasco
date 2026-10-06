import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

// MARK: - Cross-platform image from Data

extension Image {
    init?(data: Data) {
        #if canImport(UIKit)
        guard let img = UIImage(data: data) else { return nil }
        self.init(uiImage: img)
        #else
        guard let img = NSImage(data: data) else { return nil }
        self.init(nsImage: img)
        #endif
    }
}

// MARK: - Website palette (static fallbacks, prefer LascoTheme env)

extension Color {
    enum Lasco {
        static let bg          = Color(hex: "111315")
        static let bgDeep      = Color(hex: "191c1f")
        static let surface     = Color(hex: "191c1f")
        static let surfaceAlt  = Color(hex: "22262a")
        static let ink         = Color(hex: "f1f0eb")
        static let inkSub      = Color(hex: "b2b8b9")
        static let inkMuted    = Color(hex: "b2b8b9")
        static let accent      = Color(hex: "f4b8d5")
        static let accentPress = Color(hex: "dc8eb5")
        static let ok          = Color(hex: "a5d7aa")
        static let warn        = Color(hex: "efc879")
        static let error       = Color(hex: "f3a5a5")
        static let pink        = Color(hex: "f4b8d5")
        static let border      = Color(hex: "343a3b")
    }

    init(hex: String) {
        let scanner = Scanner(string: hex)
        var rgb: UInt64 = 0
        scanner.scanHexInt64(&rgb)
        self.init(
            red:   Double((rgb >> 16) & 0xFF) / 255,
            green: Double((rgb >> 8)  & 0xFF) / 255,
            blue:  Double( rgb        & 0xFF) / 255
        )
    }
}

// MARK: - Theme

struct LascoTheme {
    let bg: Color
    let bgDeep: Color
    let surface: Color
    let surfaceAlt: Color
    let ink: Color
    let inkSub: Color
    let inkMuted: Color
    let accent: Color
    let accentPress: Color
    let ok: Color
    let warn: Color
    let error: Color
    let pink: Color
    let border: Color

    static let dark = LascoTheme(
        bg: Color.Lasco.bg, bgDeep: Color.Lasco.bgDeep,
        surface: Color.Lasco.surface, surfaceAlt: Color.Lasco.surfaceAlt,
        ink: Color.Lasco.ink, inkSub: Color.Lasco.inkSub, inkMuted: Color.Lasco.inkMuted,
        accent: Color.Lasco.accent, accentPress: Color.Lasco.accentPress,
        ok: Color.Lasco.ok, warn: Color.Lasco.warn, error: Color.Lasco.error,
        pink: Color.Lasco.pink, border: Color.Lasco.border
    )
}

private struct LascoThemeKey: EnvironmentKey {
    static let defaultValue: LascoTheme = .dark
}

extension EnvironmentValues {
    var lascoTheme: LascoTheme {
        get { self[LascoThemeKey.self] }
        set { self[LascoThemeKey.self] = newValue }
    }
}

// MARK: - Typography
// Space Grotesk matches the website; JetBrains Mono is reserved for code and paths.

enum LascoFont {
    static func categoryLarge(_ size: CGFloat = 36) -> Font { .custom("SpaceGrotesk-Bold", size: size, relativeTo: .largeTitle) }
    static func categorySmall(_ size: CGFloat = 22) -> Font { .custom("SpaceGrotesk-Bold", size: size, relativeTo: .headline) }
    static func subtitle(_ size: CGFloat = 18) -> Font { .custom("SpaceGrotesk-Regular", size: size, relativeTo: .subheadline) }
    // Retained for existing metadata call sites; no pixel font is rendered.
    static func pixel(_ size: CGFloat = 15) -> Font { .custom("SpaceGrotesk-Regular", size: size, relativeTo: .body) }
    static func title(_ size: CGFloat = 22) -> Font { .custom("SpaceGrotesk-Bold", size: size, relativeTo: .title2) }
    static func body(_ size: CGFloat = 15) -> Font { .custom("SpaceGrotesk-Regular", size: size, relativeTo: .body) }
    static let button: Font = .custom("SpaceGrotesk-Bold", size: 14, relativeTo: .body)
    static func mono(_ size: CGFloat = 12) -> Font { .custom("JetBrainsMono-Regular", size: size, relativeTo: .caption) }
}

enum LascoShape {
    static let panel = RoundedRectangle(cornerRadius: 16)
    static let control = RoundedRectangle(cornerRadius: 9)
}

// MARK: - Panels
// Only panels and album cards clip their children; media-grid cells remain square.

struct LascoFlatPanel: ViewModifier {
    @Environment(\.lascoTheme) var theme

    func body(content: Content) -> some View {
        content
            .background(theme.surface)
            .clipShape(LascoShape.panel)
            .overlay(LascoShape.panel.strokeBorder(theme.border, lineWidth: 1))
    }
}

struct LascoHardShadowPanel: ViewModifier {
    @Environment(\.lascoTheme) var theme

    func body(content: Content) -> some View {
        content
            .background(theme.surface)
            .clipShape(LascoShape.panel)
            .overlay(LascoShape.panel.strokeBorder(theme.border, lineWidth: 1))
    }
}

// MARK: - Input

struct LascoInputModifier: ViewModifier {
    @Environment(\.lascoTheme) var theme

    func body(content: Content) -> some View {
        content
            .font(LascoFont.body())
            .foregroundStyle(theme.ink)
            .padding(.horizontal, 10)
            .padding(.vertical, 9)
            .background(theme.surfaceAlt)
            .clipShape(LascoShape.control)
            .overlay(LascoShape.control.strokeBorder(theme.border, lineWidth: 1))
            .tint(theme.pink)
    }
}

// MARK: - Back button

struct LascoBackButtonLabel: View {
    @Environment(\.lascoTheme) var theme

    var body: some View {
        Image("angle-left")
            .renderingMode(.template)
            .resizable()
            .frame(width: 16, height: 16)
            .foregroundStyle(theme.ink)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .frame(minWidth: 44, minHeight: 44)
            .background(theme.surface)
            .clipShape(LascoShape.control)
            .overlay(LascoShape.control.strokeBorder(theme.border, lineWidth: 1))
    }
}

struct LascoBackButton: View {
    let action: () -> Void
    var body: some View {
        Button(action: action) { LascoBackButtonLabel() }
            .buttonStyle(.plain)
    }
}

private struct ToolbarBackButtonModifier: ViewModifier {
    let action: () -> Void
    var isVisible: Bool = true

    func body(content: Content) -> some View {
        #if canImport(UIKit)
        content
        #else
        content.toolbar {
            if isVisible {
                ToolbarItem(placement: .navigation) {
                    Button(action: action) { LascoBackButtonLabel() }
                        .buttonStyle(.borderless)
                }
            }
        }
        #endif
    }
}

// MARK: - Navigation bar hiding

private struct HideSystemNavBarModifier: ViewModifier {
    @Environment(\.lascoTheme) var theme

    func body(content: Content) -> some View {
        #if canImport(UIKit)
        content.toolbar(.hidden, for: .navigationBar)
        #else
        content.toolbarBackground(theme.bg, for: .windowToolbar)
        #endif
    }
}

struct RemoveTitleToolbarModifier: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 18, macOS 15, *) {
            content.toolbar(removing: .title)
        } else {
            content
        }
    }
}

extension View {
    func lascoPanel() -> some View     { modifier(LascoFlatPanel()) }
    func lascoPanelHard() -> some View { modifier(LascoHardShadowPanel()) }
    func lascoInput() -> some View     { modifier(LascoInputModifier()) }

    func hideSystemNavigationBar() -> some View {
        modifier(HideSystemNavBarModifier())
    }

    func toolbarBackButton(action: @escaping () -> Void, isVisible: Bool = true) -> some View {
        modifier(ToolbarBackButtonModifier(action: action, isVisible: isVisible))
    }

    // Fully hides the navigation bar/toolbar for sheet-hosted NavigationStacks.
    func hideSheetNavigationBar() -> some View {
        #if canImport(UIKit)
        self.toolbar(.hidden, for: .navigationBar)
        #else
        self.toolbar(.hidden, for: .windowToolbar)
        #endif
    }
}

// MARK: - Button Styles

struct LascoPrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration)
    }
    private struct Inner: View {
        @Environment(\.lascoTheme) var theme
        let configuration: ButtonStyleConfiguration
        var body: some View {
            configuration.label
                .font(LascoFont.button)
                .foregroundStyle(theme.bg)
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .frame(minHeight: 44)
                .frame(maxWidth: .infinity)
                .background(configuration.isPressed ? theme.accentPress : theme.accent)
                .clipShape(LascoShape.control)
                .overlay(LascoShape.control.strokeBorder(theme.border, lineWidth: 1))
        }
    }
}

struct LascoSecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration)
    }
    private struct Inner: View {
        @Environment(\.lascoTheme) var theme
        let configuration: ButtonStyleConfiguration
        var body: some View {
            configuration.label
                .font(LascoFont.button)
                .foregroundStyle(theme.ink)
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .frame(minHeight: 44)
                .frame(maxWidth: .infinity)
                .background(configuration.isPressed ? theme.surfaceAlt : theme.surface)
                .clipShape(LascoShape.control)
                .overlay(LascoShape.control.strokeBorder(theme.border, lineWidth: 1))
        }
    }
}

struct LascoGhostButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration)
    }
    private struct Inner: View {
        @Environment(\.lascoTheme) var theme
        let configuration: ButtonStyleConfiguration
        var body: some View {
            configuration.label
                .font(LascoFont.button)
                .foregroundStyle(theme.inkSub)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .background(configuration.isPressed ? theme.bg : Color.clear)
                .clipShape(LascoShape.control)
                .overlay(LascoShape.control.strokeBorder(theme.border, lineWidth: 1))
                .contentShape(Rectangle())
        }
    }
}

struct LascoDevButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration)
    }
    private struct Inner: View {
        @Environment(\.lascoTheme) var theme
        let configuration: ButtonStyleConfiguration
        var body: some View {
            configuration.label
                .font(LascoFont.button)
                .foregroundStyle(theme.ink)
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .frame(minHeight: 44)
                .frame(maxWidth: .infinity)
                .background(theme.warn.opacity(configuration.isPressed ? 0.2 : 0.1))
                .clipShape(LascoShape.control)
                .overlay(LascoShape.control.strokeBorder(theme.border, lineWidth: 1))
        }
    }
}

struct LascoDangerButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration)
    }
    private struct Inner: View {
        @Environment(\.lascoTheme) var theme
        let configuration: ButtonStyleConfiguration
        var body: some View {
            configuration.label
                .font(LascoFont.button)
                .foregroundStyle(theme.bg)
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .frame(minHeight: 44)
                .background(configuration.isPressed ? theme.error.opacity(0.8) : theme.error)
                .clipShape(LascoShape.control)
                .overlay(LascoShape.control.strokeBorder(theme.border, lineWidth: 1))
        }
    }
}

// MARK: - Toggle Style

struct LascoToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration)
    }
    private struct Inner: View {
        @Environment(\.lascoTheme) var theme
        let configuration: ToggleStyleConfiguration
        var body: some View {
            Toggle(configuration)
                .toggleStyle(.switch)
                .labelsHidden()
                .tint(theme.pink)
        }
    }
}

// MARK: - Checkbox

struct LascoCheckbox: View {
    @Binding var isOn: Bool
    let label: String
    @Environment(\.lascoTheme) var theme

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            HStack(alignment: .top, spacing: 10) {
                ZStack {
                    RoundedRectangle(cornerRadius: 4)
                        .fill(isOn ? theme.pink : theme.surfaceAlt)
                        .frame(width: 20, height: 20)
                        .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(isOn ? theme.pink : theme.border, lineWidth: 1))
                    if isOn {
                        Image(systemName: "checkmark")
                            .font(.system(size: 12, weight: .bold))
                            .foregroundStyle(theme.bg)
                    }
                }
                Text(label)
                    .font(LascoFont.body(13))
                    .foregroundStyle(theme.inkSub)
                    .fixedSize(horizontal: false, vertical: true)
                    .multilineTextAlignment(.leading)
            }
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Reusable Components

struct StatCard: View {
    let value: String
    let label: String
    var valueColor: Color? = nil
    @Environment(\.lascoTheme) var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value)
                .font(LascoFont.title(26))
                .foregroundStyle(valueColor ?? theme.ink)
            Text(label.uppercased())
                .font(LascoFont.categorySmall(11))
                .foregroundStyle(theme.inkMuted)
                .tracking(1)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .lascoPanel()
    }
}

struct FieldLabel: View {
    let text: String
    var size: CGFloat = 11
    @Environment(\.lascoTheme) var theme
    var body: some View {
        Text(text.uppercased())
            .font(LascoFont.categorySmall(size))
            .foregroundStyle(theme.inkSub)
            .tracking(1.5)
    }
}

struct ErrorBanner: View {
    let message: String
    @Environment(\.lascoTheme) var theme
    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text("✗")
                .font(LascoFont.mono())
                .foregroundStyle(theme.error)
            Text(message)
                .font(LascoFont.body(13))
                .foregroundStyle(theme.error)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(10)
        .background(theme.error.opacity(0.08))
        .clipShape(LascoShape.control)
        .overlay(LascoShape.control.strokeBorder(theme.error, lineWidth: 1))
    }
}

// MARK: - Floating Tab Bar

enum AppTab: CaseIterable, Hashable {
    case home, albums, status, manage

    var icon: String {
        switch self {
        case .home:   return "home"
        case .albums: return "image"
        case .status: return "disc"
        case .manage: return "cog"
        }
    }

    var selectedIcon: String {
        switch self {
        case .home:   return "home-solid"
        case .albums: return "image-solid"
        case .status: return "disc-solid"
        case .manage: return "cog-solid"
        }
    }

    var label: String {
        switch self {
        case .home:   return "HOME"
        case .albums: return "ALBUMS"
        case .status: return "STATUS"
        case .manage: return "MANAGE"
        }
    }
}

struct FloatingTabBar: View {
    @Binding var selectedTab: AppTab
    @Environment(\.lascoTheme) var theme

    var body: some View {
        HStack(spacing: 0) {
            ForEach(AppTab.allCases, id: \.self) { tab in
                Button {
                    selectedTab = tab
                } label: {
                    Image(selectedTab == tab ? tab.selectedIcon : tab.icon)
                        .renderingMode(.template)
                        .resizable()
                        .frame(width: 20, height: 20)
                        .foregroundStyle(selectedTab == tab ? theme.pink : theme.inkMuted)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(tab.label)
                .accessibilityIdentifier("tab.\(tab.label.lowercased())")
            }
        }
        .lascoPanel()
    }
}
