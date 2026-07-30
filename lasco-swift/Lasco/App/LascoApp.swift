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
    @State private var directory = LibraryDirectoryModel()
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
                if directory.isOpen,
                   let repository = directory.activeRepository,
                   let session = directory.session,
                   let syncCoordinator = directory.syncCoordinator,
                   let importCoordinator = directory.importCoordinator {
                    MainView(
                        repository: repository,
                        session: session,
                        syncCoordinator: syncCoordinator,
                        importCoordinator: importCoordinator
                    )
                        .environment(toastManager)
                        .preferredColorScheme(.dark)
                        .toastOverlay(toastManager)
                } else if directory.showOnboarding {
                    OnboardingView()
                        .environment(directory)
                        .environment(toastManager)
                        #if os(macOS)
                        .frame(width: 390, height: 700)
                        #endif
                        .toastOverlay(toastManager)
                } else {
                    LibraryListView()
                        .environment(directory)
                        .environment(toastManager)
                        #if os(macOS)
                        .frame(width: 390, height: 700)
                        #endif
                        .toastOverlay(toastManager)
                }
            }
            .modifier(RemoveTitleToolbarModifier())
            .hideSystemNavigationBar()
            .environment(directory)
            .environment(\.lascoTheme, directory.isOpen ? .dark : .plaster)
            .tint(directory.isOpen ? LascoTheme.dark.pink : LascoTheme.plaster.pink)
            .task { await directory.start() }
        }
        .windowResizability(directory.isOpen ? .contentMinSize : .contentSize)
        #if canImport(UIKit)
        .onChange(of: scenePhase) { _, newPhase in
            guard newPhase == .active,
                  let syncCoordinator = directory.syncCoordinator,
                  let importCoordinator = directory.importCoordinator else { return }
            Task {
                await syncCoordinator.fetchDefaultRemote()
                await importCoordinator.autoImportFromPhotoLibrary()
            }
        }
        #endif
    }
}
