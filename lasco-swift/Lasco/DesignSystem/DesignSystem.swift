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

// MARK: - Plaster Theme — Colors (static fallbacks, prefer LascoTheme env)

extension Color {
    enum Lasco {
        static let bg          = Color(hex: "e6e2d4")
        static let bgDeep      = Color(hex: "d2cdba")
        static let surface     = Color(hex: "f3efe2")
        static let surfaceAlt  = Color(hex: "ffffff")
        static let ink         = Color(hex: "1a1a1a")
        static let inkSub      = Color(hex: "4a4a48")
        static let inkMuted    = Color(hex: "8a8682")
        static let accent      = Color(hex: "0a0f2e")
        static let accentPress = Color(hex: "04071a")
        static let ok          = Color(hex: "5b8b3e")
        static let warn        = Color(hex: "d9a23a")
        static let error       = Color(hex: "c44a3e")
        static let pink        = Color(hex: "FFB8D9")
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

    static let plaster = LascoTheme(
        bg:          Color(hex: "e6e2d4"),
        bgDeep:      Color(hex: "d2cdba"),
        surface:     Color(hex: "f3efe2"),
        surfaceAlt:  Color(hex: "ffffff"),
        ink:         Color(hex: "1a1a1a"),
        inkSub:      Color(hex: "4a4a48"),
        inkMuted:    Color(hex: "8a8682"),
        accent:      Color(hex: "0a0f2e"),
        accentPress: Color(hex: "04071a"),
        ok:          Color(hex: "5b8b3e"),
        warn:        Color(hex: "d9a23a"),
        error:       Color(hex: "c44a3e"),
        pink:        Color(hex: "E84A8A")
    )

    static let dark = LascoTheme(
        bg:          .black,
        bgDeep:      Color(hex: "111111"),
        surface:     Color(hex: "1a1a1a"),
        surfaceAlt:  Color(hex: "222222"),
        ink:         .white,
        inkSub:      Color(hex: "cccccc"),
        inkMuted:    Color(hex: "888888"),
        accent:      Color(hex: "0a0f2e"),
        accentPress: Color(hex: "04071a"),
        ok:          Color(hex: "5b8b3e"),
        warn:        Color(hex: "d9a23a"),
        error:       Color(hex: "c44a3e"),
        pink:        Color(hex: "FFB8D9")
    )
}

private struct LascoThemeKey: EnvironmentKey {
    static let defaultValue: LascoTheme = .plaster
}

extension EnvironmentValues {
    var lascoTheme: LascoTheme {
        get { self[LascoThemeKey.self] }
        set { self[LascoThemeKey.self] = newValue }
    }
}

// MARK: - Typography
// Requires Jersey10-Regular, VT323-Regular, SpaceGrotesk-Bold, SpaceGrotesk-Regular,
// JetBrainsMono-Regular to be added to the Xcode project target (Info.plist UIAppFonts).

enum LascoFont {
    // Jersey 10 — ALL CAPS pixel titles
    static func categoryLarge(_ size: CGFloat = 36) -> Font { .custom("Jersey10-Regular", size: size) }
    static func categorySmall(_ size: CGFloat = 22) -> Font { .custom("Jersey10-Regular", size: size) }
    // VT323 — pixel subtitle / metadata / overlays
    static func subtitle(_ size: CGFloat = 18) -> Font { .custom("VT323-Regular", size: size) }
    static func pixel(_ size: CGFloat = 15) -> Font  { .custom("VT323-Regular", size: size) }
    // Space Grotesk — statement titles and body
    static func title(_ size: CGFloat = 22) -> Font  { .custom("SpaceGrotesk-Bold", size: size) }
    static func body(_ size: CGFloat = 15) -> Font   { .custom("SpaceGrotesk-Regular", size: size) }
    static let button: Font = .custom("SpaceGrotesk-Bold", size: 14)
    // JetBrains Mono — paths, sizes, timestamps
    static func mono(_ size: CGFloat = 12) -> Font   { .custom("JetBrainsMono-Regular", size: size) }
}

// MARK: - Panels
// No border-radius. Ever.
// Flat: 2px ink border, surfaceAlt bg.
// Hard: flat + 5px hard offset ink shadow.

struct LascoFlatPanel: ViewModifier {
    @Environment(\.lascoTheme) var theme

    func body(content: Content) -> some View {
        content
            .background(theme.surfaceAlt)
            .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
    }
}

struct LascoHardShadowPanel: ViewModifier {
    @Environment(\.lascoTheme) var theme

    func body(content: Content) -> some View {
        content
            .background(theme.surfaceAlt)
            .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
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
            .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
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
            .background(theme.bg)
            .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
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

// MARK: - Win98 Bevel

private struct EdgeLines: Shape {
    var edges: [Edge]

    func path(in rect: CGRect) -> Path {
        var path = Path()
        for edge in edges {
            switch edge {
            case .top:
                path.move(to: CGPoint(x: rect.minX, y: rect.minY))
                path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
            case .leading:
                path.move(to: CGPoint(x: rect.minX, y: rect.minY))
                path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
            case .bottom:
                path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
                path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
            case .trailing:
                path.move(to: CGPoint(x: rect.maxX, y: rect.minY))
                path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
            }
        }
        return path
    }
}

private extension View {
    func bevel(pressed: Bool, hi: Color, lo: Color, ink: Color) -> some View {
        overlay(
            ZStack {
                Rectangle().stroke(ink, lineWidth: 2)
                EdgeLines(edges: [.top, .leading])
                    .stroke(pressed ? lo : hi, lineWidth: 1)
                    .padding(1)
                EdgeLines(edges: [.bottom, .trailing])
                    .stroke(pressed ? hi : lo, lineWidth: 1)
                    .padding(1)
            }
        )
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
                .foregroundStyle(Color.white)
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity)
                .background(configuration.isPressed ? theme.accentPress : theme.accent)
                .bevel(pressed: configuration.isPressed,
                       hi: Color(hex: "2a3060"),
                       lo: Color(hex: "000000"),
                       ink: theme.ink)
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
                .frame(maxWidth: .infinity)
                .background(configuration.isPressed ? theme.bgDeep : theme.bg)
                .bevel(pressed: configuration.isPressed,
                       hi: theme.surfaceAlt,
                       lo: theme.inkSub,
                       ink: theme.ink)
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
                .overlay(Rectangle().stroke(theme.inkSub, lineWidth: 1))
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
                .frame(maxWidth: .infinity)
                .background(
                    ZStack {
                        (configuration.isPressed ? theme.bgDeep : theme.bg)
                        Canvas { ctx, size in
                            for x in stride(from: CGFloat(0), to: size.width, by: 2) {
                                for y in stride(from: CGFloat(0), to: size.height, by: 2) {
                                    ctx.fill(
                                        Path(CGRect(x: x, y: y, width: 1, height: 1)),
                                        with: .color(theme.warn.opacity(0.35))
                                    )
                                }
                            }
                        }
                    }
                )
                .bevel(pressed: configuration.isPressed,
                       hi: theme.surfaceAlt,
                       lo: theme.inkSub,
                       ink: theme.ink)
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
                .foregroundStyle(Color.white)
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .background(configuration.isPressed ? Color(hex: "a03530") : theme.error)
                .bevel(pressed: configuration.isPressed,
                       hi: Color(hex: "e07068"),
                       lo: Color(hex: "7a2020"),
                       ink: theme.ink)
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
            let isOn = configuration.isOn
            ZStack(alignment: isOn ? .trailing : .leading) {
                Rectangle()
                    .fill(isOn ? theme.ink : theme.surfaceAlt)
                    .frame(width: 36, height: 22)
                    .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
                Rectangle()
                    .fill(isOn ? theme.bg : theme.inkMuted)
                    .frame(width: 14, height: 14)
                    .padding(3)
            }
            .frame(width: 36, height: 22)
            .onTapGesture { configuration.isOn.toggle() }
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
                    Rectangle()
                        .fill(isOn ? theme.ink : theme.surfaceAlt)
                        .frame(width: 20, height: 20)
                        .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
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
        .overlay(Rectangle().stroke(theme.error, lineWidth: 1))
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
                        .foregroundStyle(selectedTab == tab ? theme.ink : theme.inkMuted)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .background(theme.surfaceAlt)
        .overlay(Rectangle().stroke(theme.ink, lineWidth: 2))
    }
}
