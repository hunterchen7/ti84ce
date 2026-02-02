package com.calc.emulator

import android.content.Context
import android.content.SharedPreferences

/**
 * Preferences helper for emulator settings.
 */
object EmulatorPreferences {
    private const val PREFS_NAME = "emulator_prefs"
    private const val KEY_BACKEND = "backend"

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
}
