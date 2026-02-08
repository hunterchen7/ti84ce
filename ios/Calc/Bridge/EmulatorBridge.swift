//
//  EmulatorBridge.swift
//  Calc
//
//  Swift wrapper for the emulator core C API.
//  Provides thread-safe access to emulator functions.
//  Supports runtime backend switching when multiple backends are available.
//

import Foundation
import CoreGraphics
import os.log

/// Bridge to the native emulator core.
/// Thread-safe wrapper around the C API defined in emu_backend.h.
/// Supports runtime switching between Rust and CEmu backends when both are available.
class EmulatorBridge {
    private static let logger = Logger(subsystem: "com.calc.emulator", category: "EmulatorBridge")

    /// Native emulator handle (opaque pointer to Emu struct)
    private var handle: OpaquePointer?

    /// Lock for thread-safe access to emulator
    private let lock = NSLock()

    /// Cached framebuffer dimensions
    private(set) var width: Int32 = 320
    private(set) var height: Int32 = 240

    /// Log buffer for messages from emulator
    private var logBuffer: [String] = []
    private let logLock = NSLock()
    private static let maxLogs = 200

    /// Static log callback for C interop
    private static var sharedInstance: EmulatorBridge?

    init() {
        EmulatorBridge.sharedInstance = self
    }

    deinit {
        destroy()
        if EmulatorBridge.sharedInstance === self {
            EmulatorBridge.sharedInstance = nil
        }
    }

    // MARK: - Lifecycle

    /// Create the emulator instance.
    /// Must be called before any other methods.
    /// - Returns: true if successful
    func create() -> Bool {
        lock.lock()
        defer { lock.unlock() }

        if handle != nil {
            Self.logger.warning("Emulator already created")
            return true
        }

        // Set up log callback
        emu_set_log_callback { message in
            guard let message = message else { return }
            let str = String(cString: message)
            EmulatorBridge.sharedInstance?.appendLog(str)
        }

        handle = emu_create()

        if handle == nil {
            Self.logger.error("Failed to create emulator instance")
            return false
        }

        // Get framebuffer dimensions
        var w: Int32 = 0
        var h: Int32 = 0
        _ = emu_framebuffer(handle, &w, &h)
        width = w > 0 ? w : 320
        height = h > 0 ? h : 240

        Self.logger.info("Emulator created: \(self.width)x\(self.height)")
        return true
    }

    /// Destroy the emulator instance.
    /// Must be called when done to free resources.
    func destroy() {
        lock.lock()
        defer { lock.unlock() }

        if let h = handle {
            emu_destroy(h)
            handle = nil
            Self.logger.info("Emulator destroyed")
        }
    }

    /// Check if emulator is created.
    var isCreated: Bool {
        lock.lock()
        defer { lock.unlock() }
        return handle != nil
    }

    // MARK: - ROM Loading

    /// Load ROM data into the emulator.
    /// - Parameter data: ROM file contents
    /// - Returns: 0 on success, negative error code on failure
    func loadRom(_ data: Data) -> Int32 {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else {
            Self.logger.error("loadRom: emulator not created")
            return -1
        }

        Self.logger.info("Loading ROM: \(data.count) bytes")

        let result = data.withUnsafeBytes { (bytes: UnsafeRawBufferPointer) -> Int32 in
            guard let ptr = bytes.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return -2
            }
            return emu_load_rom(h, ptr, data.count)
        }

        if result != 0 {
            Self.logger.error("loadRom: emu_load_rom returned \(result)")
        }

        return result
    }

    /// Reset the emulator to initial state.
    func reset() {
        lock.lock()
        defer { lock.unlock() }

        if let h = handle {
            Self.logger.info("Resetting emulator")
            emu_reset(h)
        }
    }

    /// Power on the emulator (simulate ON key press+release to wake from reset).
    /// Must be called after loadRom() to start execution.
    func powerOn() {
        lock.lock()
        defer { lock.unlock() }

        if let h = handle {
            Self.logger.info("Powering on emulator")
            emu_power_on(h)
        }
    }

    // MARK: - Execution

    /// Run emulation for the specified number of cycles.
    /// - Parameter cycles: Number of cycles to execute
    /// - Returns: Number of cycles actually executed
    func runCycles(_ cycles: Int32) -> Int32 {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else { return 0 }
        return emu_run_cycles(h, cycles)
    }

    // MARK: - Framebuffer

    /// Get the current framebuffer as ARGB8888 pixels.
    /// - Returns: Tuple of (pointer to pixel data, width, height), or nil if unavailable
    func framebuffer() -> (UnsafePointer<UInt32>?, width: Int32, height: Int32) {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else {
            return (nil, 0, 0)
        }

        var w: Int32 = 0
        var h32: Int32 = 0
        let fb = emu_framebuffer(h, &w, &h32)

        return (fb, w, h32)
    }

    /// Create a CGImage from the current framebuffer.
    /// - Returns: CGImage of the screen, or nil if unavailable
    func makeImage() -> CGImage? {
        let (fb, w, h) = framebuffer()

        guard let fb = fb, w > 0, h > 0 else {
            return nil
        }

        let colorSpace = CGColorSpaceCreateDeviceRGB()

        // ARGB8888 format: alpha in high byte, then R, G, B
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)

        guard let context = CGContext(
            data: UnsafeMutableRawPointer(mutating: fb),
            width: Int(w),
            height: Int(h),
            bitsPerComponent: 8,
            bytesPerRow: Int(w) * 4,
            space: colorSpace,
            bitmapInfo: bitmapInfo.rawValue
        ) else {
            return nil
        }

        return context.makeImage()
    }

    // MARK: - Input

    /// Set key state.
    /// - Parameters:
    ///   - row: Key row (0-7)
    ///   - col: Key column (0-7)
    ///   - down: true if pressed, false if released
    func setKey(row: Int32, col: Int32, down: Bool) {
        lock.lock()
        defer { lock.unlock() }

        if let h = handle {
            Self.logger.debug("setKey: row=\(row) col=\(col) down=\(down)")
            emu_set_key(h, row, col, down ? 1 : 0)
        }
    }

    // MARK: - LCD State

    /// Check if LCD is on (should display content).
    /// - Returns: true when LCD should show content, false when LCD is off (show black)
    func isLcdOn() -> Bool {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else { return false }
        return emu_is_lcd_on(h) != 0
    }

    /// Get the backlight brightness level.
    /// - Returns: 0-255, where 0 = off (screen black)
    func getBacklight() -> UInt8 {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else { return 0 }
        return emu_get_backlight(h)
    }

    // MARK: - Save State

    /// Get the size required for save state buffer.
    /// - Returns: Size in bytes, or 0 if not available
    func saveStateSize() -> Int {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else { return 0 }
        return emu_save_state_size(h)
    }

    /// Save the current emulator state.
    /// - Returns: State data, or nil on failure
    func saveState() -> Data? {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else { return nil }

        let size = emu_save_state_size(h)
        guard size > 0 else { return nil }

        var buffer = Data(count: size)
        let result = buffer.withUnsafeMutableBytes { (bytes: UnsafeMutableRawBufferPointer) -> Int32 in
            guard let ptr = bytes.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return -1
            }
            return emu_save_state(h, ptr, size)
        }

        return result >= 0 ? buffer : nil
    }

    /// Load a saved emulator state.
    /// - Parameter data: Previously saved state data
    /// - Returns: 0 on success, negative error code on failure
    func loadState(_ data: Data) -> Int32 {
        lock.lock()
        defer { lock.unlock() }

        guard let h = handle else { return -1 }

        return data.withUnsafeBytes { (bytes: UnsafeRawBufferPointer) -> Int32 in
            guard let ptr = bytes.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return -2
            }
            return emu_load_state(h, ptr, data.count)
        }
    }

    // MARK: - Backend Management

    /// Get the list of available backends.
    /// - Returns: Array of backend names (e.g., ["rust", "cemu"])
    static func getAvailableBackends() -> [String] {
        guard let cString = emu_backend_get_available() else {
            return []
        }
        let available = String(cString: cString)
        if available.isEmpty {
            return []
        }
        return available.components(separatedBy: ",")
    }

    /// Get the number of available backends.
    /// - Returns: Number of backends (0, 1, or 2)
    static func getBackendCount() -> Int {
        return Int(emu_backend_count())
    }

    /// Get the current backend name.
    /// - Returns: Current backend name, or nil if no backend is loaded
    static func getCurrentBackend() -> String? {
        guard let cString = emu_backend_get_current() else {
            return nil
        }
        return String(cString: cString)
    }

    /// Set the active backend.
    /// Note: This must be called before creating an emulator instance,
    /// or after destroying the current instance.
    /// - Parameter name: Backend name ("rust" or "cemu")
    /// - Returns: true if successful
    static func setBackend(_ name: String) -> Bool {
        let result = emu_backend_set(name)
        if result == 0 {
            logger.info("Backend switched to: \(name)")
            return true
        } else {
            logger.error("Failed to switch backend to: \(name)")
            return false
        }
    }

    /// Check if backend switching is available (multiple backends linked).
    /// - Returns: true if more than one backend is available
    static func isBackendSwitchingAvailable() -> Bool {
        return getBackendCount() > 1
    }

    // MARK: - Logging

    private func appendLog(_ message: String) {
        logLock.lock()
        defer { logLock.unlock() }

        logBuffer.append(message)
        if logBuffer.count > Self.maxLogs {
            logBuffer.removeFirst(logBuffer.count - Self.maxLogs)
        }
    }

    /// Drain pending log messages.
    /// - Returns: Array of log messages since last drain
    func drainLogs() -> [String] {
        logLock.lock()
        defer { logLock.unlock() }

        let logs = logBuffer
        logBuffer.removeAll()
        return logs
    }
}
