//
//  EmulatorPreferences.swift
//  Calc
//
//  Preferences helper for emulator settings.
//  Stores user preferences in UserDefaults.
//

import Foundation

/// Manages persistent emulator preferences.
class EmulatorPreferences {
    private static let backendKey = "preferredBackend"

    /// Get the user's preferred backend name.
    /// - Returns: Backend name or nil to use default
    static func getPreferredBackend() -> String? {
        return UserDefaults.standard.string(forKey: backendKey)
    }

    /// Set the user's preferred backend name.
    /// - Parameter name: Backend name to save
    static func setPreferredBackend(_ name: String) {
        UserDefaults.standard.set(name, forKey: backendKey)
    }

    /// Get the effective backend to use.
    /// Returns the preferred backend if available, otherwise the first available backend.
    /// - Returns: Backend name to use, or nil if no backends available
    static func getEffectiveBackend() -> String? {
        let preferred = getPreferredBackend()
        let available = EmulatorBridge.getAvailableBackends()

        // If preferred backend is available, use it
        if let preferred = preferred, available.contains(preferred) {
            return preferred
        }

        // Otherwise use the first available
        return available.first
    }
}
