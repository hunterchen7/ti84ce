//
//  EmulatorView.swift
//  Calc
//
//  Main emulator screen with LCD display, keypad, and sidebar menu.
//

import SwiftUI

/// Main emulator view with screen display and keypad
struct EmulatorView: View {
    @ObservedObject var state: EmulatorState
    @State private var showingSidebar = false
    @State private var showingRomPicker = false
    @State private var showingBackendPicker = false
    @State private var sidebarDragOffset: CGFloat = 0
    @State private var isDraggingToClose = false

    private let sidebarWidth: CGFloat = 280
    private let edgeSwipeWidth: CGFloat = 30

    var body: some View {
        GeometryReader { _ in
            ZStack(alignment: .topLeading) {
                // Main content
                VStack(spacing: 8) {
                    // Screen display
                    screenDisplay
                        .aspectRatio(320.0 / 240.0, contentMode: .fit)
                        .background(Color.black)
                        .cornerRadius(4)
                        .padding(.horizontal, 8)
                        .padding(.top, 8)

                    // Keypad
                    KeypadView(
                        onKeyDown: { row, col in state.keyDown(row: row, col: col) },
                        onKeyUp: { row, col in state.keyUp(row: row, col: col) }
                    )
                }

                // Invisible edge swipe area for opening sidebar
                if !showingSidebar {
                    Color.clear
                        .frame(width: edgeSwipeWidth)
                        .contentShape(Rectangle())
                        .gesture(
                            DragGesture(minimumDistance: 10)
                                .onChanged { value in
                                    if value.translation.width > 0 {
                                        sidebarDragOffset = min(value.translation.width, sidebarWidth)
                                    }
                                }
                                .onEnded { value in
                                    withAnimation(.easeOut(duration: 0.25)) {
                                        if value.translation.width > sidebarWidth * 0.3 {
                                            showingSidebar = true
                                        }
                                        sidebarDragOffset = 0
                                    }
                                }
                        )
                }

                // Sidebar overlay with swipe-to-close
                if showingSidebar || sidebarDragOffset > 0 {
                    sidebarOverlay
                }

                // Debug overlay
                if state.showDebug {
                    DebugOverlayView(state: state)
                        .padding(.top, 50)
                }
            }
        }
        .sheet(isPresented: $showingRomPicker) {
            DocumentPicker { url in
                loadRom(from: url)
            }
        }
        .sheet(isPresented: $showingBackendPicker) {
            BackendPickerView(state: state)
        }
    }

    /// LCD screen display
    @ViewBuilder
    private var screenDisplay: some View {
        if state.isLcdOn, let image = state.screenImage {
            Image(decorative: image, scale: 1.0)
                .resizable()
                .interpolation(.none)
                .aspectRatio(contentMode: .fit)
        } else {
            // LCD off - show black
            Color.black
        }
    }

    /// Sidebar menu overlay
    private var sidebarOverlay: some View {
        // When dragging to close, use sidebarDragOffset; otherwise use full width if open
        let effectiveOffset = isDraggingToClose ? sidebarDragOffset : (showingSidebar ? sidebarWidth : sidebarDragOffset)
        let progress = effectiveOffset / sidebarWidth

        return HStack(spacing: 0) {
            // Sidebar content
            VStack(alignment: .leading, spacing: 0) {
                Spacer().frame(height: 24)

                Text("TI-84 Plus CE")
                    .font(.system(size: 20, weight: .bold))
                    .foregroundColor(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)

                Divider()
                    .background(Color(red: 0.2, green: 0.2, blue: 0.267))
                    .padding(.vertical, 8)

                // Load ROM
                sidebarButton(title: "Load ROM", color: .white) {
                    showingSidebar = false
                    showingRomPicker = true
                }

                // Pause/Run toggle
                sidebarButton(
                    title: state.isRunning ? "Pause Emulation" : "Run Emulation",
                    color: state.isRunning
                        ? Color(red: 1.0, green: 0.341, blue: 0.133)
                        : Color(red: 0.298, green: 0.686, blue: 0.314)
                ) {
                    state.isRunning.toggle()
                    showingSidebar = false
                }

                // Reset
                sidebarButton(title: "Reset", color: .white) {
                    state.reset()
                    showingSidebar = false
                }

                Divider()
                    .background(Color(red: 0.2, green: 0.2, blue: 0.267))
                    .padding(.vertical, 8)

                // Debug toggle
                sidebarButton(
                    title: state.showDebug ? "Hide Debug Info" : "Show Debug Info",
                    color: state.showDebug
                        ? Color(red: 0.612, green: 0.153, blue: 0.690)
                        : .white
                ) {
                    state.showDebug.toggle()
                }

                // Backend settings (only show if multiple backends available)
                if EmulatorBridge.isBackendSwitchingAvailable() {
                    sidebarButton(
                        title: "Backend: \(EmulatorBridge.getCurrentBackend() ?? "None")",
                        color: Color(red: 0.129, green: 0.588, blue: 0.953)
                    ) {
                        showingSidebar = false
                        showingBackendPicker = true
                    }
                }

                Divider()
                    .background(Color(red: 0.2, green: 0.2, blue: 0.267))
                    .padding(.vertical, 8)

                // Speed control
                VStack(alignment: .leading, spacing: 4) {
                    Text("Speed: \(Int(state.speedMultiplier))x")
                        .font(.system(size: 14))
                        .foregroundColor(.white)
                        .padding(.horizontal, 16)

                    Slider(
                        value: $state.speedMultiplier,
                        in: 1...10,
                        step: 1
                    )
                    .tint(Color(red: 0.298, green: 0.686, blue: 0.314))
                    .padding(.horizontal, 16)
                }

                Spacer()

                // ROM info at bottom
                if let romName = state.romName {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("ROM: \(romName)")
                            .font(.system(size: 12))
                            .foregroundColor(.gray)
                        Text("Size: \(state.romSize / 1024) KB")
                            .font(.system(size: 12))
                            .foregroundColor(.gray)
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 16)
                }
            }
            .frame(width: sidebarWidth)
            .background(Color(red: 0.102, green: 0.102, blue: 0.180)) // #1A1A2E
            .offset(x: effectiveOffset - sidebarWidth)
            .gesture(
                DragGesture(minimumDistance: 10)
                    .onChanged { value in
                        if value.translation.width < 0 {
                            isDraggingToClose = true
                            sidebarDragOffset = max(sidebarWidth + value.translation.width, 0)
                        }
                    }
                    .onEnded { value in
                        withAnimation(.easeOut(duration: 0.25)) {
                            if value.translation.width < -sidebarWidth * 0.3 ||
                               value.predictedEndTranslation.width < -sidebarWidth * 0.5 {
                                showingSidebar = false
                            }
                            sidebarDragOffset = 0
                            isDraggingToClose = false
                        }
                    }
            )

            // Tap outside to close
            Color.black.opacity(0.3 * progress)
                .onTapGesture {
                    withAnimation(.easeOut(duration: 0.25)) { showingSidebar = false }
                }
        }
        .edgesIgnoringSafeArea(.all)
    }

    /// Sidebar button helper
    private func sidebarButton(title: String, color: Color, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 16))
                .foregroundColor(color)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
        }
    }

    /// Load ROM from URL
    private func loadRom(from url: URL) {
        do {
            guard url.startAccessingSecurityScopedResource() else {
                state.loadError = "Cannot access file"
                return
            }
            defer { url.stopAccessingSecurityScopedResource() }

            let data = try Data(contentsOf: url)
            let name = url.lastPathComponent
            state.loadRom(data, name: name)
        } catch {
            state.loadError = "Error: \(error.localizedDescription)"
        }
    }
}

/// Backend picker sheet for switching between emulator backends
struct BackendPickerView: View {
    @ObservedObject var state: EmulatorState
    @Environment(\.dismiss) var dismiss

    var body: some View {
        NavigationView {
            List {
                ForEach(EmulatorBridge.getAvailableBackends(), id: \.self) { backend in
                    Button(action: {
                        switchBackend(to: backend)
                    }) {
                        HStack {
                            VStack(alignment: .leading) {
                                Text(backend.capitalized)
                                    .foregroundColor(.primary)
                                Text(backendDescription(backend))
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                            Spacer()
                            if EmulatorBridge.getCurrentBackend() == backend {
                                Image(systemName: "checkmark")
                                    .foregroundColor(.blue)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Select Backend")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }

    private func backendDescription(_ backend: String) -> String {
        switch backend {
        case "rust":
            return "Native Rust emulator core"
        case "cemu":
            return "CEmu reference emulator"
        default:
            return ""
        }
    }

    private func switchBackend(to backend: String) {
        // Stop emulation
        state.stopEmulation()

        // Destroy current emulator
        state.emulator.destroy()

        // Switch backend
        if EmulatorBridge.setBackend(backend) {
            // Save preference
            EmulatorPreferences.setPreferredBackend(backend)

            // Recreate emulator with new backend
            if state.emulator.create() {
                // Reload ROM if we had one
                if let romData = state.lastLoadedRomData, let romName = state.romName {
                    state.loadRom(romData, name: romName)
                }
            }
        }

        dismiss()
    }
}

#Preview {
    EmulatorView(state: EmulatorState())
        .preferredColorScheme(.dark)
}
