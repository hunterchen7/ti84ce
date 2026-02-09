package com.calc.emulator

import android.content.Context
import android.content.SharedPreferences

/**
 * Preferences helper for emulator settings.
 */
object EmulatorPreferences {
    private const val PREFS_NAME = "emulator_prefs"
    private const val KEY_BACKEND = "backend"
    private const val KEY_LAST_ROM_HASH = "last_rom_hash"
    private const val KEY_CALCULATOR_SCALE = "calculator_scale"
    private const val KEY_CALCULATOR_Y_OFFSET = "calculator_y_offset"

    private fun getPrefs(context: Context): SharedPreferences {
        return context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    /**
     * Get the preferred backend name.
     * @return Backend name or null to use default
     */
    fun getPreferredBackend(context: Context): String? {
        return getPrefs(context).getString(KEY_BACKEND, null)
    }

    /**
     * Set the preferred backend name.
     * @param backendName Backend name to save
     */
    fun setPreferredBackend(context: Context, backendName: String) {
        getPrefs(context).edit().putString(KEY_BACKEND, backendName).apply()
    }

    /**
     * Get the effective backend to use.
     * Returns the preferred backend if available, otherwise the first available backend.
     */
    fun getEffectiveBackend(context: Context): String? {
        val preferred = getPreferredBackend(context)
        val available = EmulatorBridge.getAvailableBackends()

        // If preferred backend is available, use it
        if (preferred != null && available.contains(preferred)) {
            return preferred
        }

        // Otherwise use the first available
        return available.firstOrNull()
    }

    /**
     * Get the last used ROM hash.
     * @return ROM hash or null if no ROM was previously loaded
     */
    fun getLastRomHash(context: Context): String? {
        return getPrefs(context).getString(KEY_LAST_ROM_HASH, null)
    }

    /**
     * Set the last used ROM hash.
     * @param hash ROM hash to save
     */
    fun setLastRomHash(context: Context, hash: String) {
        getPrefs(context).edit().putString(KEY_LAST_ROM_HASH, hash).apply()
    }

    fun getCalculatorScale(context: Context): Float {
        val value = getPrefs(context).getFloat(KEY_CALCULATOR_SCALE, 1f)
        return if (value > 0) value else 1f
    }

    fun setCalculatorScale(context: Context, scale: Float) {
        getPrefs(context).edit().putFloat(KEY_CALCULATOR_SCALE, scale).apply()
    }

    fun getCalculatorYOffset(context: Context): Float {
        return getPrefs(context).getFloat(KEY_CALCULATOR_Y_OFFSET, 0f)
    }

    fun setCalculatorYOffset(context: Context, offset: Float) {
        getPrefs(context).edit().putFloat(KEY_CALCULATOR_Y_OFFSET, offset).apply()
    }

    /**
     * Clear the last ROM hash (e.g., when ROM fails to load).
     */
    fun clearLastRomHash(context: Context) {
        getPrefs(context).edit().remove(KEY_LAST_ROM_HASH).apply()
    }
}
