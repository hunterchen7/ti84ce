//
//  ContentView.swift
//  Calc
//
//  Main content view managing emulator state and UI flow.
//

import SwiftUI
import UniformTypeIdentifiers

/// Main content view for the emulator
struct ContentView: View {
    /// Emulator bridge instance
    @StateObject private var state = EmulatorState()

    /// Track scene phase for background/foreground transitions
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        ZStack {
            Color(red: 0.067, green: 0.067, blue: 0.067)
                .ignoresSafeArea()

            if state.romLoaded {
                EmulatorView(state: state)
            } else {
                RomLoadingView(state: state)
            }
        }
        .onAppear {
            _ = state.emulator.create()
            // Try to load saved ROM and state on launch
            state.tryLoadSavedRom()
        }
        .onChange(of: scenePhase) { newPhase in
            if newPhase == .background {
                // Save state when app goes to background
                state.saveEmulatorState()
            }
        }
    }
}

/// Observable state for the emulator
class EmulatorState: ObservableObject {
    // Emulator bridge
    let emulator = EmulatorBridge()

    // ROM state
    @Published var romLoaded = false
    @Published var romName: String?
    @Published var romSize: Int = 0
    @Published var loadError: String?

    // Emulation state
    @Published var isRunning = false
    @Published var isLcdOn = true

    // Debug info
    @Published var totalCyclesExecuted: Int64 = 0
    @Published var frameCounter: Int = 0
    @Published var showDebug = false
    @Published var lastKeyPress = "None"
    @Published var logs: [String] = []

    // Speed control (1x = 800K cycles/frame at 60FPS = real-time at 48MHz)
    @Published var speedMultiplier: Float = 1.0

    // Screen image
    @Published var screenImage: CGImage?

    // Emulation task
    private var emulationTask: Task<Void, Never>?

    /// Cycles per tick based on speed multiplier
    /// Base: 800K cycles per frame = real-time at 48MHz / 60FPS
    var cyclesPerTick: Int32 {
        Int32(800_000 * speedMultiplier)
    }

    deinit {
        stopEmulation()
        emulator.destroy()
    }

    /// Load ROM from data
    /// - Parameters:
    ///   - data: ROM data
    ///   - name: ROM filename
    ///   - saveToStorage: Whether to save the ROM to internal storage (default: true)
    ///   - tryLoadState: Whether to try loading saved emulator state (default: false)
    func loadRom(_ data: Data, name: String, saveToStorage: Bool = true, tryLoadState: Bool = false) {
        let result = emulator.loadRom(data)

        if result == 0 {
            romLoaded = true
            romName = name
            romSize = data.count
            loadError = nil
            totalCyclesExecuted = 0
            frameCounter = 0
            logs.removeAll()

            // Save to storage for next launch
            if saveToStorage {
                _ = RomStorage.saveRom(data, originalName: name)
            }

            // Try to restore saved emulator state
            if tryLoadState, let stateData = RomStorage.loadState() {
                let loadResult = emulator.loadState(stateData)
                if loadResult == 0 {
                    print("Restored emulator state")
                } else {
                    print("Failed to restore state: \(loadResult)")
                }
            }

            isRunning = true
            startEmulation()
        } else {
            loadError = "Failed to load ROM (error: \(result))"
        }
    }

    /// Try to load saved ROM on app launch
    func tryLoadSavedRom() {
        guard !romLoaded else { return }

        if let saved = RomStorage.loadSavedRom() {
            loadRom(saved.data, name: saved.filename, saveToStorage: false, tryLoadState: true)
        }
    }

    /// Save current emulator state to storage
    func saveEmulatorState() {
        guard romLoaded else { return }

        if let stateData = emulator.saveState() {
            if RomStorage.saveState(stateData) {
                print("Emulator state saved: \(stateData.count) bytes")
            }
        }
    }

    /// Reset emulator
    func reset() {
        emulator.reset()
        totalCyclesExecuted = 0
        frameCounter = 0
        logs.removeAll()
    }

    /// Start emulation loop
    func startEmulation() {
        guard emulationTask == nil else { return }

        emulationTask = Task.detached(priority: .userInitiated) { [weak self] in
            guard let self = self else { return }

            while !Task.isCancelled {
                let running = await MainActor.run { self.isRunning }
                guard running else {
                    try? await Task.sleep(nanoseconds: 16_000_000) // 16ms
                    continue
                }

                let frameStart = Date()
                let cycles = await MainActor.run { self.cyclesPerTick }
                let executed = self.emulator.runCycles(cycles)

                await MainActor.run {
                    self.totalCyclesExecuted += Int64(executed)
                    self.frameCounter += 1
                    self.screenImage = self.emulator.makeImage()
                    self.isLcdOn = self.emulator.isLcdOn()

                    // Drain logs
                    let newLogs = self.emulator.drainLogs()
                    if !newLogs.isEmpty {
                        self.logs.append(contentsOf: newLogs)
                        if self.logs.count > 200 {
                            self.logs.removeFirst(self.logs.count - 200)
                        }
                    }
                }

                // Cap at 60 FPS
                let elapsed = Date().timeIntervalSince(frameStart)
                let remaining = 0.016 - elapsed
                if remaining > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(remaining * 1_000_000_000))
                }
            }
        }
    }

    /// Stop emulation loop
    func stopEmulation() {
        emulationTask?.cancel()
        emulationTask = nil
    }

    /// Handle key press
    func keyDown(row: Int32, col: Int32) {
        lastKeyPress = "(\(row),\(col)) DOWN"
        emulator.setKey(row: row, col: col, down: true)
    }

    /// Handle key release
    func keyUp(row: Int32, col: Int32) {
        lastKeyPress = "(\(row),\(col)) UP"
        emulator.setKey(row: row, col: col, down: false)
    }
}

// MARK: - Document Picker

/// Document picker for selecting ROM files
struct DocumentPicker: UIViewControllerRepresentable {
    let onPick: (URL) -> Void

    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.data, .item])
        picker.delegate = context.coordinator
        picker.allowsMultipleSelection = false
        return picker
    }

    func updateUIViewController(_ uiViewController: UIDocumentPickerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(onPick: onPick)
    }

    class Coordinator: NSObject, UIDocumentPickerDelegate {
        let onPick: (URL) -> Void

        init(onPick: @escaping (URL) -> Void) {
            self.onPick = onPick
        }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            guard let url = urls.first else { return }
            onPick(url)
        }
    }
}

#Preview {
    ContentView()
}
