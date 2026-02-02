//
//  RomLoadingView.swift
//  Calc
//
//  Initial screen for loading a ROM file.
//

import SwiftUI

/// Initial ROM loading screen
struct RomLoadingView: View {
    @ObservedObject var state: EmulatorState
    @State private var showingPicker = false

    var body: some View {
        VStack(spacing: 0) {
            Spacer()

            // Title
            Text("TI-84 Plus CE")
                .font(.system(size: 28, weight: .bold))
                .foregroundColor(.white)

            Text("Emulator")
                .font(.system(size: 20))
                .foregroundColor(.gray)
                .padding(.bottom, 48)

            // Import button
            Button(action: { showingPicker = true }) {
                Text("Import ROM")
                    .font(.system(size: 18))
                    .foregroundColor(.white)
                    .frame(maxWidth: .infinity)
                    .frame(height: 56)
                    .background(Color(red: 0.298, green: 0.686, blue: 0.314)) // #4CAF50
                    .cornerRadius(8)
            }
            .padding(.horizontal, 32)

            Spacer().frame(height: 16)

            Text("Select a TI-84 Plus CE ROM file to begin")
                .font(.system(size: 14))
                .foregroundColor(.gray)

            // Error display
            if let error = state.loadError {
                Spacer().frame(height: 24)
                Text(error)
                    .font(.system(size: 14))
                    .foregroundColor(Color(red: 1.0, green: 0.341, blue: 0.133)) // #FF5722
            }

            Spacer().frame(height: 48)

            Text("You must provide your own legally obtained ROM file.")
                .font(.system(size: 12))
                .foregroundColor(Color(white: 0.3))

            // Backend selector (only show if multiple backends available)
            if EmulatorBridge.isBackendSwitchingAvailable() {
                Spacer().frame(height: 32)

                HStack {
                    Text("Backend:")
                        .font(.system(size: 14))
                        .foregroundColor(.gray)

                    Picker("Backend", selection: Binding(
                        get: { EmulatorBridge.getCurrentBackend() ?? "rust" },
                        set: { newBackend in
                            if EmulatorBridge.setBackend(newBackend) {
                                EmulatorPreferences.setPreferredBackend(newBackend)
                            }
                        }
                    )) {
                        ForEach(EmulatorBridge.getAvailableBackends(), id: \.self) { backend in
                            Text(backend.capitalized).tag(backend)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 160)
                }
                .padding(.horizontal, 32)
            }

            Spacer()
        }
        .sheet(isPresented: $showingPicker) {
            DocumentPicker { url in
                loadRom(from: url)
            }
        }
    }

    private func loadRom(from url: URL) {
        do {
            // Start accessing security-scoped resource
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

#Preview {
    RomLoadingView(state: EmulatorState())
        .preferredColorScheme(.dark)
}
