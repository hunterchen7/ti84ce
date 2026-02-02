//
//  RomStorage.swift
//  Calc
//
//  Handles ROM file storage and persistence.
//  Saves ROMs to documents directory and remembers the last loaded ROM.
//

import Foundation
import os.log

/// Handles ROM file storage and persistence
struct RomStorage {
    private static let logger = Logger(subsystem: "com.calc.emulator", category: "RomStorage")
    private static let romFilenameKey = "lastRomFilename"
    private static let romDirName = "roms"

    /// Get the ROM storage directory, creating it if needed
    private static var romDirectory: URL? {
        let fileManager = FileManager.default
        guard let documentsDir = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first else {
            logger.error("Could not find documents directory")
            return nil
        }

        let romDir = documentsDir.appendingPathComponent(romDirName, isDirectory: true)

        if !fileManager.fileExists(atPath: romDir.path) {
            do {
                try fileManager.createDirectory(at: romDir, withIntermediateDirectories: true)
            } catch {
                logger.error("Failed to create ROM directory: \(error.localizedDescription)")
                return nil
            }
        }

        return romDir
    }

    /// Save ROM data to storage and remember it
    /// - Returns: true if saved successfully
    static func saveRom(_ data: Data, originalName: String) -> Bool {
        guard let romDir = romDirectory else { return false }

        let safeFilename = sanitizeFilename(originalName)
        let romURL = romDir.appendingPathComponent(safeFilename)

        do {
            try data.write(to: romURL)
            UserDefaults.standard.set(safeFilename, forKey: romFilenameKey)
            logger.info("Saved ROM: \(safeFilename) (\(data.count) bytes)")
            return true
        } catch {
            logger.error("Failed to save ROM: \(error.localizedDescription)")
            return false
        }
    }

    /// Load the previously saved ROM if available
    /// - Returns: ROM data and filename, or nil if no saved ROM
    static func loadSavedRom() -> (data: Data, filename: String)? {
        guard let filename = UserDefaults.standard.string(forKey: romFilenameKey),
              let romDir = romDirectory else {
            return nil
        }

        let romURL = romDir.appendingPathComponent(filename)

        guard FileManager.default.fileExists(atPath: romURL.path) else {
            logger.warning("Saved ROM file not found: \(filename)")
            clearSavedRom()
            return nil
        }

        do {
            let data = try Data(contentsOf: romURL)
            logger.info("Loaded saved ROM: \(filename) (\(data.count) bytes)")
            return (data, filename)
        } catch {
            logger.error("Failed to load saved ROM: \(error.localizedDescription)")
            return nil
        }
    }

    /// Check if there's a saved ROM available
    static func hasSavedRom() -> Bool {
        guard let filename = UserDefaults.standard.string(forKey: romFilenameKey),
              let romDir = romDirectory else {
            return false
        }

        let romURL = romDir.appendingPathComponent(filename)
        return FileManager.default.fileExists(atPath: romURL.path)
    }

    /// Clear the saved ROM preference (does not delete the file)
    static func clearSavedRom() {
        UserDefaults.standard.removeObject(forKey: romFilenameKey)
    }

    // MARK: - Emulator State Persistence

    private static let stateFilename = "emulator.state"

    /// Get the state file URL
    private static var stateFileURL: URL? {
        let fileManager = FileManager.default
        guard let documentsDir = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first else {
            return nil
        }
        return documentsDir.appendingPathComponent(stateFilename)
    }

    /// Save emulator state to storage
    /// - Returns: true if saved successfully
    static func saveState(_ data: Data) -> Bool {
        guard let stateURL = stateFileURL else { return false }

        do {
            try data.write(to: stateURL)
            logger.info("Saved emulator state: \(data.count) bytes")
            return true
        } catch {
            logger.error("Failed to save emulator state: \(error.localizedDescription)")
            return false
        }
    }

    /// Load emulator state from storage
    /// - Returns: State data, or nil if no saved state
    static func loadState() -> Data? {
        guard let stateURL = stateFileURL,
              FileManager.default.fileExists(atPath: stateURL.path) else {
            logger.debug("No saved emulator state found")
            return nil
        }

        do {
            let data = try Data(contentsOf: stateURL)
            logger.info("Loaded emulator state: \(data.count) bytes")
            return data
        } catch {
            logger.error("Failed to load emulator state: \(error.localizedDescription)")
            return nil
        }
    }

    /// Check if there's a saved emulator state
    static func hasSavedState() -> Bool {
        guard let stateURL = stateFileURL else { return false }
        return FileManager.default.fileExists(atPath: stateURL.path)
    }

    /// Delete the saved emulator state
    static func clearSavedState() {
        guard let stateURL = stateFileURL,
              FileManager.default.fileExists(atPath: stateURL.path) else {
            return
        }

        do {
            try FileManager.default.removeItem(at: stateURL)
            logger.info("Cleared saved emulator state")
        } catch {
            logger.error("Failed to clear emulator state: \(error.localizedDescription)")
        }
    }

    /// Sanitize filename for safe storage
    private static func sanitizeFilename(_ name: String) -> String {
        // Remove path separators and problematic characters
        let sanitized = name.replacingOccurrences(of: "[/\\\\:*?\"<>|]", with: "_", options: .regularExpression)
        // Ensure it has .rom extension if no extension
        return sanitized.contains(".") ? sanitized : "\(sanitized).rom"
    }
}
