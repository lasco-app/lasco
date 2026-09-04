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
    #if DEBUG
    @State private var showDevelopmentEndpointPrompt = true
    #endif
    @Environment(\.scenePhase) private var scenePhase
    @State private var releasePolicy = ClientReleasePolicy.shared

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
                   let activeSession = directory.activeSession {
                    MainView(
                        repository: activeSession.repository,
                        session: activeSession.state,
                        syncCoordinator: activeSession.syncCoordinator,
                        importCoordinator: activeSession.mediaImportCoordinator,
                        releasePolicy: releasePolicy
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
            .task { await directory.start(); await releasePolicy.refresh() }
            #if DEBUG
            .sheet(isPresented: $showDevelopmentEndpointPrompt) {
                DevelopmentCloudEndpointView(isPresented: $showDevelopmentEndpointPrompt)
                    .environment(\.lascoTheme, directory.isOpen ? .dark : .plaster)
                    .preferredColorScheme(.dark)
            }
            #endif
        }
        .windowResizability(directory.isOpen ? .contentMinSize : .contentSize)
        #if canImport(UIKit)
        .onChange(of: scenePhase) { _, newPhase in
            guard newPhase == .active else { return }
            Task {
                await releasePolicy.refresh()
                if let activeSession = directory.activeSession {
                    await activeSession.syncCoordinator.fetchDefaultRemote()
                    await activeSession.autoPhotoImportCoordinator.importFromPhotoLibrary()
                }
            }
        }
        #endif
    }
}
