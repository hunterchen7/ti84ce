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
    // MARK: - Keys

    private static let backendKey = "preferredBackend"
    private static let speedMultiplierKey = "speedMultiplier"
    private static let autoSaveKey = "autoSaveEnabled"
    private static let lastRomHashKey = "lastRomHash"
    private static let lastRomNameKey = "lastRomName"
    private static let calculatorScaleKey = "calculatorScale"
    private static let calculatorYOffsetKey = "calculatorYOffset"

    // MARK: - Backend

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

    // MARK: - Speed

    /// Speed multiplier (default 1.0)
    static var speedMultiplier: Float {
        get {
            let value = UserDefaults.standard.float(forKey: speedMultiplierKey)
            return value > 0 ? value : 1.0
        }
        set {
            UserDefaults.standard.set(newValue, forKey: speedMultiplierKey)
        }
    }

    // MARK: - Display

    /// Calculator scale (default 1.0)
    static var calculatorScale: Float {
        get {
            let value = UserDefaults.standard.float(forKey: calculatorScaleKey)
            return value > 0 ? value : 1.0
        }
        set {
            UserDefaults.standard.set(newValue, forKey: calculatorScaleKey)
        }
    }

    /// Calculator Y offset (default 0)
    static var calculatorYOffset: Float {
        get { UserDefaults.standard.float(forKey: calculatorYOffsetKey) }
        set { UserDefaults.standard.set(newValue, forKey: calculatorYOffsetKey) }
    }

    // MARK: - Auto-save

    /// Whether auto-save is enabled (default true)
    static var autoSaveEnabled: Bool {
        get {
            // Default to true if not set
            if UserDefaults.standard.object(forKey: autoSaveKey) == nil {
                return true
            }
            return UserDefaults.standard.bool(forKey: autoSaveKey)
        }
        set {
            UserDefaults.standard.set(newValue, forKey: autoSaveKey)
        }
    }

    // MARK: - Last ROM

    /// Hash of the last loaded ROM
    static var lastRomHash: String? {
        get { UserDefaults.standard.string(forKey: lastRomHashKey) }
        set { UserDefaults.standard.set(newValue, forKey: lastRomHashKey) }
    }

    /// Name of the last loaded ROM
    static var lastRomName: String? {
        get { UserDefaults.standard.string(forKey: lastRomNameKey) }
        set { UserDefaults.standard.set(newValue, forKey: lastRomNameKey) }
    }

    // MARK: - Clear

    /// Clear all saved ROM/state info (for fresh start)
    static func clearLastRomInfo() {
        lastRomHash = nil
        lastRomName = nil
    }
}
