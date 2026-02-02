//
//  CalcApp.swift
//  Calc
//
//  TI-84 Plus CE Emulator for iOS
//

import SwiftUI
import os.log

@main
struct CalcApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var appState = AppState.shared

    var body: some Scene {
        WindowGroup {
            ContentView()
                .preferredColorScheme(.dark)
                .environmentObject(appState)
        }
        .onChange(of: scenePhase) { newPhase in
            switch newPhase {
            case .background:
                appState.handleBackground()
            case .active:
                appState.handleForeground()
            case .inactive:
                // Optionally pause emulation
                break
            @unknown default:
                break
            }
        }
    }
}

/// Global app state for lifecycle management
class AppState: ObservableObject {
    private static let logger = Logger(subsystem: "com.calc.emulator", category: "AppState")

    static let shared = AppState()

    /// Weak reference to the current emulator state (set by ContentView)
    weak var emulatorState: EmulatorState?

    private init() {}

    /// Handle app going to background - auto-save state
    func handleBackground() {
        Self.logger.info("App entering background")

        guard let state = emulatorState else {
            Self.logger.warning("Skipping auto-save: emulatorState is nil")
            return
        }

        guard EmulatorPreferences.autoSaveEnabled else {
            Self.logger.info("Skipping auto-save: disabled in preferences")
            return
        }

        guard state.romLoaded else {
            Self.logger.info("Skipping auto-save: no ROM loaded")
            return
        }

        guard let romHash = state.currentRomHash else {
            Self.logger.warning("Skipping auto-save: no ROM hash")
            return
        }

        Self.logger.info("Attempting to save state for ROM \(romHash)")

        // Get state size first
        let stateSize = state.emulator.saveStateSize()
        Self.logger.info("State size from emulator: \(stateSize) bytes")

        if stateSize == 0 {
            Self.logger.error("Emulator returned state size of 0 - save_state not implemented?")
            return
        }

        if StateManager.shared.saveState(emulator: state.emulator, romHash: romHash) {
            Self.logger.info("Auto-saved state for ROM \(romHash)")
        } else {
            Self.logger.error("Failed to auto-save state for ROM \(romHash)")
        }
    }

    /// Handle app coming to foreground
    func handleForeground() {
        Self.logger.info("App entering foreground")
        // State restoration happens in EmulatorState.loadRom()
    }
}
