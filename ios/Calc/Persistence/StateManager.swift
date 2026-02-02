//
//  StateManager.swift
//  Calc
//
//  Manages save state persistence for the emulator.
//  Handles saving/loading emulator state and ROM data to app storage.
//

import Foundation
import CryptoKit
import os.log

/// Manages save state persistence for the emulator.
class StateManager {
    private static let logger = Logger(subsystem: "com.calc.emulator", category: "StateManager")

    /// Directory for save states
    private let statesDirectory: URL

    /// Directory for ROM copies
    private let romsDirectory: URL

    /// Shared instance
    static let shared = StateManager()

    private init() {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!

        statesDirectory = appSupport.appendingPathComponent("SaveStates", isDirectory: true)
        romsDirectory = appSupport.appendingPathComponent("ROMs", isDirectory: true)

        // Create directories if needed
        try? FileManager.default.createDirectory(at: statesDirectory, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: romsDirectory, withIntermediateDirectories: true)
    }

    // MARK: - ROM Hash

    /// Compute SHA-256 hash of ROM data (truncated to 16 hex chars for filenames)
    func romHash(_ data: Data) -> String {
        let hash = SHA256.hash(data: data)
        // Use first 8 bytes (16 hex chars) for reasonable uniqueness + short filenames
        return hash.prefix(8).map { String(format: "%02x", $0) }.joined()
    }

    // MARK: - ROM Persistence

    /// Get ROM file path for a hash
    func romFilePath(for hash: String) -> URL {
        romsDirectory.appendingPathComponent("\(hash).rom")
    }

    /// Save ROM data to app storage (creates our own copy)
    func saveRom(_ data: Data, hash: String) -> Bool {
        let romPath = romFilePath(for: hash)

        // Skip if already saved
        if FileManager.default.fileExists(atPath: romPath.path) {
            Self.logger.info("ROM already cached: \(hash)")
            return true
        }

        do {
            try data.write(to: romPath, options: .atomic)
            Self.logger.info("Saved ROM copy: \(hash) (\(data.count) bytes)")
            return true
        } catch {
            Self.logger.error("Failed to save ROM: \(error.localizedDescription)")
            return false
        }
    }

    /// Load ROM data from app storage
    func loadRom(hash: String) -> Data? {
        let romPath = romFilePath(for: hash)

        guard FileManager.default.fileExists(atPath: romPath.path) else {
            Self.logger.info("No cached ROM for hash \(hash)")
            return nil
        }

        do {
            let data = try Data(contentsOf: romPath)
            Self.logger.info("Loaded cached ROM: \(hash) (\(data.count) bytes)")
            return data
        } catch {
            Self.logger.error("Failed to load ROM: \(error.localizedDescription)")
            return nil
        }
    }

    /// Check if ROM exists in app storage
    func hasRom(hash: String) -> Bool {
        FileManager.default.fileExists(atPath: romFilePath(for: hash).path)
    }

    // MARK: - State Persistence

    /// Get state file path for a ROM hash
    func stateFilePath(for romHash: String) -> URL {
        statesDirectory.appendingPathComponent("\(romHash).state")
    }

    /// Save current emulator state
    func saveState(emulator: EmulatorBridge, romHash: String) -> Bool {
        let statePath = stateFilePath(for: romHash)

        guard let stateData = emulator.saveState() else {
            Self.logger.error("Failed to get state data from emulator")
            return false
        }

        do {
            try stateData.write(to: statePath, options: .atomic)
            Self.logger.info("Saved state: \(statePath.lastPathComponent) (\(stateData.count) bytes)")
            return true
        } catch {
            Self.logger.error("Failed to write state file: \(error.localizedDescription)")
            return false
        }
    }

    /// Load saved state for a ROM
    func loadState(emulator: EmulatorBridge, romHash: String) -> Bool {
        let statePath = stateFilePath(for: romHash)

        guard FileManager.default.fileExists(atPath: statePath.path) else {
            Self.logger.info("No saved state for ROM hash \(romHash)")
            return false
        }

        do {
            let stateData = try Data(contentsOf: statePath)
            let result = emulator.loadState(stateData)

            if result == 0 {
                Self.logger.info("Loaded state from \(statePath.lastPathComponent)")
                return true
            } else {
                Self.logger.error("Failed to load state: error \(result) - \(Self.stateErrorDescription(result))")
                // Delete corrupted/incompatible state file
                try? FileManager.default.removeItem(at: statePath)
                return false
            }
        } catch {
            Self.logger.error("Failed to read state file: \(error.localizedDescription)")
            return false
        }
    }

    /// Check if a saved state exists for a ROM
    func hasState(for romHash: String) -> Bool {
        FileManager.default.fileExists(atPath: stateFilePath(for: romHash).path)
    }

    /// Delete saved state for a ROM
    func deleteState(for romHash: String) {
        let statePath = stateFilePath(for: romHash)
        try? FileManager.default.removeItem(at: statePath)
        Self.logger.info("Deleted state for ROM hash \(romHash)")
    }

    // MARK: - Error Descriptions

    /// Descriptive error for state operations
    static func stateErrorDescription(_ code: Int32) -> String {
        switch code {
        case -100: return "State persistence not available"
        case -101: return "Buffer too small"
        case -102: return "Invalid state file format"
        case -103: return "State file version mismatch"
        case -104: return "State was saved with a different ROM"
        case -105: return "State file is corrupted"
        default: return "Unknown error (\(code))"
        }
    }
}
