package com.calc.emulator

import android.content.Context
import android.graphics.Bitmap
import android.util.Log

/**
 * JNI bridge to the emulator core with dynamic backend support.
 * Supports switching between Rust and CEmu backends at runtime.
 */
class EmulatorBridge {
    companion object {
        private const val TAG = "EmulatorBridge"

        // Singleton-like initialization tracking
        private var initialized = false

        // Track which backend libraries are available
        private val loadedBackends = mutableListOf<String>()

        init {
            // Load JNI bridge first
            try {
                System.loadLibrary("emu_jni")
                Log.i(TAG, "JNI loader library loaded successfully")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load JNI loader library", e)
            }

            // Preload backend libraries via System.loadLibrary to ensure they're
            // packaged in the APK. dlopen() will then find them in the native lib dir.
            tryLoadBackend("rust")
            tryLoadBackend("cemu")
        }

        private fun tryLoadBackend(name: String) {
            try {
                System.loadLibrary("emu_$name")
                loadedBackends.add(name)
                Log.i(TAG, "Backend library loaded: emu_$name")
            } catch (e: UnsatisfiedLinkError) {
                Log.d(TAG, "Backend library not available: emu_$name")
            }
        }

        /**
         * Initialize the native library with the native library directory.
         * Must be called once before any other operations.
         */
        fun initialize(context: Context) {
            if (!initialized) {
                val nativeLibDir = context.applicationInfo.nativeLibraryDir
                val cacheDir = context.cacheDir.absolutePath
                nativeInit(nativeLibDir, cacheDir)
                initialized = true
                Log.i(TAG, "Initialized with native lib dir: $nativeLibDir, cache dir: $cacheDir")
            }
        }

        /**
         * Get list of available backend names.
         * @return List of backend names (e.g., ["rust", "cemu"])
         */
        fun getAvailableBackends(): List<String> {
            return nativeGetAvailableBackends()?.toList() ?: emptyList()
        }

        /**
         * Check if multiple backends are available (show settings toggle).
         */
        fun hasMultipleBackends(): Boolean {
            return getAvailableBackends().size > 1
        }

        // Static native methods
        @JvmStatic
        private external fun nativeInit(nativeLibDir: String, cacheDir: String)

        @JvmStatic
        private external fun nativeGetAvailableBackends(): Array<String>?
    }

    // Native handle (pointer to Emu struct)
    private var handle: Long = 0

    // Cached framebuffer dimensions
    private var width: Int = 320
    private var height: Int = 240

    // Pixel buffer for framebuffer transfer
    private var pixelBuffer: IntArray = IntArray(width * height)

    /**
     * Get the currently active backend name.
     * @return Backend name or null if none loaded
     */
    fun getCurrentBackend(): String? {
        return nativeGetCurrentBackend()
    }

    /**
     * Switch to a different backend.
     * This will destroy the current emulator instance if one exists.
     * After calling this, you must call create() again.
     *
     * @param backendName Name of the backend (e.g., "rust" or "cemu")
     * @return true if successful
     */
    fun setBackend(backendName: String): Boolean {
        if (handle != 0L) {
            Log.w(TAG, "Destroying existing emulator before backend switch")
            destroy()
        }

        val success = nativeSetBackend(backendName)
        if (success) {
            Log.i(TAG, "Backend switched to: $backendName")
        } else {
            Log.e(TAG, "Failed to switch backend to: $backendName")
        }
        return success
    }

    /**
     * Create the emulator instance.
     * Must be called before any other methods.
     * @return true if successful
     */
    fun create(): Boolean {
        if (handle != 0L) {
            Log.w(TAG, "Emulator already created")
            return true
        }

        handle = nativeCreate()
        if (handle == 0L) {
            Log.e(TAG, "Failed to create emulator")
            return false
        }

        // Get framebuffer dimensions
        width = nativeGetWidth(handle)
        height = nativeGetHeight(handle)
        pixelBuffer = IntArray(width * height)

        Log.i(TAG, "Emulator created: ${width}x${height} (backend: ${getCurrentBackend()})")
        return true
    }

    /**
     * Destroy the emulator instance.
     * Must be called when done to free resources.
     */
    fun destroy() {
        if (handle != 0L) {
            nativeDestroy(handle)
            handle = 0
            Log.i(TAG, "Emulator destroyed")
        }
    }

    /**
     * Load ROM data into the emulator.
     * @param romBytes ROM file contents
     * @return 0 on success, negative error code on failure
     */
    fun loadRom(romBytes: ByteArray): Int {
        if (handle == 0L) {
            Log.e(TAG, "loadRom: emulator not created")
            return -1
        }
        return nativeLoadRom(handle, romBytes)
    }

    /**
     * Reset the emulator to initial state.
     */
    fun reset() {
        if (handle != 0L) {
            nativeReset(handle)
        }
    }

    /**
     * Power on the emulator (simulate ON key press+release).
     * Must be called after loadRom() to start execution.
     */
    fun powerOn() {
        if (handle != 0L) {
            nativePowerOn(handle)
        }
    }

    /**
     * Run emulation for the specified number of cycles.
     * @param cycles Number of cycles to execute
     * @return Number of cycles actually executed
     */
    fun runCycles(cycles: Int): Int {
        if (handle == 0L) return 0
        return nativeRunCycles(handle, cycles)
    }

    /**
     * Get the framebuffer width.
     */
    fun getWidth(): Int = width

    /**
     * Get the framebuffer height.
     */
    fun getHeight(): Int = height

    /**
     * Copy the current framebuffer to a bitmap.
     * @param bitmap Target bitmap (must be width x height, ARGB_8888)
     * @return true on success
     */
    fun copyFramebufferToBitmap(bitmap: Bitmap): Boolean {
        if (handle == 0L) return false

        val result = nativeCopyFramebuffer(handle, pixelBuffer)
        if (result != 0) {
            Log.e(TAG, "Failed to copy framebuffer: $result")
            return false
        }

        bitmap.setPixels(pixelBuffer, 0, width, 0, 0, width, height)
        return true
    }

    /**
     * Set key state.
     * @param row Key row (0-7)
     * @param col Key column (0-7)
     * @param down true if pressed, false if released
     */
    fun setKey(row: Int, col: Int, down: Boolean) {
        if (handle != 0L) {
            nativeSetKey(handle, row, col, down)
        }
    }

    /**
     * Check if emulator is created.
     */
    fun isCreated(): Boolean = handle != 0L

    /**
     * Enable instruction trace for debugging.
     * @param count Number of instructions to trace
     */
    fun enableInstTrace(count: Int) {
        // Debug stub - tracing not yet implemented in JNI
        Log.d(TAG, "enableInstTrace($count) - stub")
    }

    /**
     * Arm instruction trace to start on next wake from HALT.
     * @param count Number of instructions to trace after wake
     */
    fun armInstTraceOnWake(count: Int) {
        // Debug stub - tracing not yet implemented in JNI
        Log.d(TAG, "armInstTraceOnWake($count) - stub")
    }

    /**
     * Drain pending emulator log lines (if any).
     */
    fun drainLogs(): List<String> {
        if (handle == 0L) return emptyList()
        return nativeDrainLogs(handle)?.toList().orEmpty()
    }

    /**
     * Get the backlight brightness level (0-255).
     * Returns 0 when backlight is off (screen should be black).
     */
    fun getBacklight(): Int {
        if (handle == 0L) return 0
        return nativeGetBacklight(handle)
    }

    /**
     * Check if LCD is on (should display content).
     * Returns true when LCD should show content, false when LCD is off (show black).
     * This matches CEmu's "LCD OFF" detection.
     */
    fun isLcdOn(): Boolean {
        if (handle == 0L) return false
        return nativeIsLcdOn(handle)
    }

    // MARK: - State Persistence

    /**
     * Get the size required for save state buffer.
     * @return Size in bytes, or 0 if not available
     */
    fun saveStateSize(): Long {
        if (handle == 0L) return 0
        return nativeSaveStateSize(handle)
    }

    /**
     * Save the current emulator state.
     * @return State data as ByteArray, or null on failure
     */
    fun saveState(): ByteArray? {
        if (handle == 0L) return null

        val size = nativeSaveStateSize(handle)
        if (size <= 0) return null

        val buffer = ByteArray(size.toInt())
        val result = nativeSaveState(handle, buffer)

        return if (result >= 0) buffer else null
    }

    /**
     * Load a saved emulator state.
     * @param stateData Previously saved state data
     * @return 0 on success, negative error code on failure
     */
    fun loadState(stateData: ByteArray): Int {
        if (handle == 0L) return -1
        return nativeLoadState(handle, stateData)
    }

    // Instance native methods
    private external fun nativeGetCurrentBackend(): String?
    private external fun nativeSetBackend(backendName: String): Boolean
    private external fun nativeCreate(): Long
    private external fun nativeDestroy(handle: Long)
    private external fun nativeLoadRom(handle: Long, romBytes: ByteArray): Int
    private external fun nativeReset(handle: Long)
    private external fun nativePowerOn(handle: Long)
    private external fun nativeRunCycles(handle: Long, cycles: Int): Int
    private external fun nativeGetWidth(handle: Long): Int
    private external fun nativeGetHeight(handle: Long): Int
    private external fun nativeCopyFramebuffer(handle: Long, outArgb: IntArray): Int
    private external fun nativeSetKey(handle: Long, row: Int, col: Int, down: Boolean)
    private external fun nativeDrainLogs(handle: Long): Array<String>?
    private external fun nativeGetBacklight(handle: Long): Int
    private external fun nativeIsLcdOn(handle: Long): Boolean
    private external fun nativeSaveStateSize(handle: Long): Long
    private external fun nativeSaveState(handle: Long, outData: ByteArray): Int
    private external fun nativeLoadState(handle: Long, stateData: ByteArray): Int
}
