//
//  LascoApp.swift
//  Lasco
//
//  Created by Pierre on 22/05/2026.
//

import SwiftUI
import CoreText

@main
struct LascoApp: App {
    @StateObject private var libraryModel = LibraryModel()
    @State private var toastManager = ToastManager()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        AppLogger.setup()

        let fonts = [
            "Jersey10-Regular",
            "VT323-Regular",
            "SpaceGrotesk-Regular",
            "SpaceGrotesk-Bold",
            "JetBrainsMono-Regular",
        ]
        for name in fonts {
            if let url = Bundle.main.url(forResource: name, withExtension: "ttf") {
                CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
            }
        }
    }

    var body: some Scene {
        WindowGroup {
            Group {
                if libraryModel.isOpen {
                    MainView()
                        .environmentObject(libraryModel)
                        .environment(toastManager)
                        .preferredColorScheme(.dark)
                        .toastOverlay(toastManager)
                } else if libraryModel.showOnboarding {
                    OnboardingView()
                        .environmentObject(libraryModel)
                        .environment(toastManager)
                        #if os(macOS)
                        .frame(width: 390, height: 700)
                        #endif
                        .toastOverlay(toastManager)
                } else {
                    LibraryListView()
                        .environmentObject(libraryModel)
                        .environment(toastManager)
                        #if os(macOS)
                        .frame(width: 390, height: 700)
                        #endif
                        .toastOverlay(toastManager)
                }
            }
            .toolbar(removing: .title)
            .hideSystemNavigationBar()
            .environment(\.lascoTheme, libraryModel.isOpen ? .dark : .plaster)
            .tint(libraryModel.isOpen ? LascoTheme.dark.pink : LascoTheme.plaster.pink)
        }
        .windowResizability(libraryModel.isOpen ? .contentMinSize : .contentSize)
        #if canImport(UIKit)
        .onChange(of: scenePhase) { _, newPhase in
            guard newPhase == .active, libraryModel.isOpen else { return }
            Task { await libraryModel.autoImportFromPhotoLibrary() }
            Task { await libraryModel.fetchDefaultRemote() }
        }
        #endif
    }
}
