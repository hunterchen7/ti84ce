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

    // Store last loaded ROM for backend switching
    var lastLoadedRomData: Data?

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

    init() {
        // Initialize backend from saved preference
        if let preferredBackend = EmulatorPreferences.getEffectiveBackend() {
            _ = EmulatorBridge.setBackend(preferredBackend)
        }
    }

    deinit {
        stopEmulation()
        emulator.destroy()
    }

    /// Load ROM from data
    func loadRom(_ data: Data, name: String) {
        let result = emulator.loadRom(data)

        if result == 0 {
            romLoaded = true
            romName = name
            romSize = data.count
            lastLoadedRomData = data  // Store for backend switching
            loadError = nil
            totalCyclesExecuted = 0
            frameCounter = 0
            logs.removeAll()
            isRunning = true
            startEmulation()
        } else {
            loadError = "Failed to load ROM (error: \(result))"
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
                guard await self.isRunning else {
                    try? await Task.sleep(nanoseconds: 16_000_000) // 16ms
                    continue
                }

                let frameStart = Date()
                let cycles = await self.cyclesPerTick
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
