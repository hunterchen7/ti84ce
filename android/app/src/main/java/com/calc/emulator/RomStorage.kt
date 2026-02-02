package com.calc.emulator

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Handles ROM file storage and persistence.
 * Saves ROMs to internal storage and remembers the last loaded ROM.
 */
object RomStorage {
    private const val TAG = "RomStorage"
    private const val PREFS_NAME = "emulator_prefs"
    private const val KEY_ROM_FILENAME = "last_rom_filename"
    private const val ROM_DIR = "roms"
    private const val STATE_FILENAME = "emulator.state"

    /**
     * Save ROM data to internal storage and remember it.
     * @return true if saved successfully
     */
    fun saveRom(context: Context, romBytes: ByteArray, originalName: String): Boolean {
        return try {
            val romDir = File(context.filesDir, ROM_DIR)
            if (!romDir.exists()) {
                romDir.mkdirs()
            }

            // Use a sanitized filename
            val safeFilename = sanitizeFilename(originalName)
            val romFile = File(romDir, safeFilename)
            romFile.writeBytes(romBytes)

            // Remember the filename
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit()
                .putString(KEY_ROM_FILENAME, safeFilename)
                .apply()

            Log.i(TAG, "Saved ROM: $safeFilename (${romBytes.size} bytes)")
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to save ROM", e)
            false
        }
    }

    /**
     * Load the previously saved ROM if available.
     * @return ROM bytes and filename, or null if no saved ROM
     */
    fun loadSavedRom(context: Context): Pair<ByteArray, String>? {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val filename = prefs.getString(KEY_ROM_FILENAME, null) ?: return null

        val romFile = File(File(context.filesDir, ROM_DIR), filename)
        if (!romFile.exists()) {
            Log.w(TAG, "Saved ROM file not found: $filename")
            clearSavedRom(context)
            return null
        }

        return try {
            val bytes = romFile.readBytes()
            Log.i(TAG, "Loaded saved ROM: $filename (${bytes.size} bytes)")
            Pair(bytes, filename)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to load saved ROM", e)
            null
        }
    }

    /**
     * Check if there's a saved ROM available.
     */
    fun hasSavedRom(context: Context): Boolean {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val filename = prefs.getString(KEY_ROM_FILENAME, null) ?: return false
        val romFile = File(File(context.filesDir, ROM_DIR), filename)
        return romFile.exists()
    }

    /**
     * Clear the saved ROM preference (does not delete the file).
     */
    fun clearSavedRom(context: Context) {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit()
            .remove(KEY_ROM_FILENAME)
            .apply()
    }

    /**
     * Save emulator state to internal storage.
     * @return true if saved successfully
     */
    fun saveState(context: Context, stateBytes: ByteArray): Boolean {
        return try {
            val stateFile = File(context.filesDir, STATE_FILENAME)
            stateFile.writeBytes(stateBytes)
            Log.i(TAG, "Saved emulator state: ${stateBytes.size} bytes")
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to save emulator state", e)
            false
        }
    }

    /**
     * Load emulator state from internal storage.
     * @return State bytes, or null if no saved state
     */
    fun loadState(context: Context): ByteArray? {
        val stateFile = File(context.filesDir, STATE_FILENAME)
        if (!stateFile.exists()) {
            Log.d(TAG, "No saved emulator state found")
            return null
        }

        return try {
            val bytes = stateFile.readBytes()
            Log.i(TAG, "Loaded emulator state: ${bytes.size} bytes")
            bytes
        } catch (e: Exception) {
            Log.e(TAG, "Failed to load emulator state", e)
            null
        }
    }

    /**
     * Check if there's a saved emulator state.
     */
    fun hasSavedState(context: Context): Boolean {
        return File(context.filesDir, STATE_FILENAME).exists()
    }

    /**
     * Delete the saved emulator state.
     */
    fun clearSavedState(context: Context) {
        val stateFile = File(context.filesDir, STATE_FILENAME)
        if (stateFile.exists()) {
            stateFile.delete()
            Log.i(TAG, "Cleared saved emulator state")
        }
    }

    private fun sanitizeFilename(name: String): String {
        // Remove path separators and other potentially problematic characters
        val sanitized = name.replace(Regex("[/\\\\:*?\"<>|]"), "_")
        // Ensure it has .rom extension if it doesn't have one
        return if (sanitized.contains('.')) sanitized else "$sanitized.rom"
    }
}
