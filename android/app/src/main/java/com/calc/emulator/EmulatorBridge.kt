package com.calc.emulator

import android.graphics.Bitmap
import android.util.Log

/**
 * JNI bridge to the Rust emulator core.
 * Wraps native functions with a Kotlin-friendly API.
 */
class EmulatorBridge {
    companion object {
        private const val TAG = "EmulatorBridge"

        init {
            try {
                System.loadLibrary("emu_jni")
                Log.i(TAG, "Native library loaded successfully")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load native library", e)
            }
        }
    }

    // Native handle (pointer to Emu struct)
    private var handle: Long = 0

    // Cached framebuffer dimensions
    private var width: Int = 320
    private var height: Int = 240

    // Pixel buffer for framebuffer transfer
    private var pixelBuffer: IntArray = IntArray(width * height)

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

        Log.i(TAG, "Emulator created: ${width}x${height}")
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

    // Native methods
    private external fun nativeCreate(): Long
    private external fun nativeDestroy(handle: Long)
    private external fun nativeLoadRom(handle: Long, romBytes: ByteArray): Int
    private external fun nativeReset(handle: Long)
    private external fun nativeRunCycles(handle: Long, cycles: Int): Int
    private external fun nativeGetWidth(handle: Long): Int
    private external fun nativeGetHeight(handle: Long): Int
    private external fun nativeCopyFramebuffer(handle: Long, outArgb: IntArray): Int
    private external fun nativeSetKey(handle: Long, row: Int, col: Int, down: Boolean)
    private external fun nativeSaveStateSize(handle: Long): Long
    private external fun nativeSaveState(handle: Long, outData: ByteArray): Int
    private external fun nativeLoadState(handle: Long, stateData: ByteArray): Int
}
