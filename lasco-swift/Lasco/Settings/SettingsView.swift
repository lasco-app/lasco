import SwiftUI
#if os(macOS)
import AppKit
#endif

struct SettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    @AppStorage("expertMode") var expertMode = false
    @State private var showLicense = false

    private var storageURL: URL? {
        FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("lasco")
    }

    var body: some View {
        ZStack {
            theme.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text("Settings")
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
                .padding(.bottom, 32)

                VStack(alignment: .leading, spacing: 0) {
                    #if os(macOS)
                    Button {
                        if let url = storageURL {
                            NSWorkspace.shared.activateFileViewerSelecting([url])
                        }
                    } label: {
                        HStack {
                            Text("Open storage location")
                                .font(LascoFont.body())
                                .foregroundStyle(theme.inkSub)
                            Spacer()
                            Text("→")
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.inkMuted)
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 14)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    #endif

                    Divider()
                        .background(theme.inkMuted.opacity(0.2))

                    if let logURL = AppLogger.logFileURL_ifExists {
                        #if DEBUG
                        Button {
                            #if canImport(UIKit)
                            UIPasteboard.general.string = logURL.path
                            #else
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(logURL.path, forType: .string)
                            #endif
                        } label: {
                            HStack {
                                Text("Copy log path")
                                    .font(LascoFont.body())
                                    .foregroundStyle(theme.inkSub)
                                Spacer()
                                Image("clipboard").renderingMode(.template).resizable().frame(width: 18, height: 18)
                                    .foregroundStyle(theme.inkMuted)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 14)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)

                        Divider()
                            .background(theme.inkMuted.opacity(0.2))
                        #endif

                        ShareLink(item: logURL) {
                            HStack {
                                Text("Share log file")
                                    .font(LascoFont.body())
                                    .foregroundStyle(theme.inkSub)
                                Spacer()
                                Image("share").renderingMode(.template).resizable().frame(width: 18, height: 18)
                                    .foregroundStyle(theme.inkMuted)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 14)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)

                        #if os(macOS)
                        Divider()
                            .background(theme.inkMuted.opacity(0.2))

                        Button {
                            NSWorkspace.shared.activateFileViewerSelecting([logURL])
                        } label: {
                            HStack {
                                Text("Reveal log in Finder")
                                    .font(LascoFont.body())
                                    .foregroundStyle(theme.inkSub)
                                Spacer()
                                Text("→")
                                    .font(LascoFont.mono())
                                    .foregroundStyle(theme.inkMuted)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 14)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        #endif

                        Divider()
                            .background(theme.inkMuted.opacity(0.2))
                    }

                    Button {
                        showLicense = true
                    } label: {
                        HStack {
                            Text("Licenses")
                                .font(LascoFont.body())
                                .foregroundStyle(theme.inkSub)
                            Spacer()
                            Text("→")
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.inkMuted)
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 14)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)

                    Divider()
                        .background(theme.inkMuted.opacity(0.2))

                    Link(destination: URL(string: "https://getlasco.app/privacy-policy")!) {
                        HStack {
                            Text("Privacy Policy")
                                .font(LascoFont.body())
                                .foregroundStyle(theme.inkSub)
                            Spacer()
                            Text("→")
                                .font(LascoFont.mono())
                                .foregroundStyle(theme.inkMuted)
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 14)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)

                    Divider()
                        .background(theme.inkMuted.opacity(0.2))

                    HStack {
                        Text("Expert mode")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkSub)
                        Spacer()
                        Toggle("", isOn: $expertMode)
                            .toggleStyle(LascoToggleStyle())
                            .labelsHidden()
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 14)
                }
                .lascoPanel()
                .padding(.horizontal, 32)

                Spacer()
            }
        }
        .sheet(isPresented: $showLicense) {
            LicenseView()
                .environment(\.lascoTheme, theme)
        }
    }
}

#Preview {
    SettingsView()
}
