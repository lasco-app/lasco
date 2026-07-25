package com.lasco.lasco.data

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Small local settings store, the Android equivalent of Swift's
 * UserDefaults backed AppStorage. Holds expert mode and the last push/fetch
 * timestamps per remote, none of which is FFI state so it does not belong
 * on SessionState.
 *
 * One instance lives for the process lifetime (see companion), so expert
 * mode is exposed as a StateFlow that every screen observes rather than a
 * plain property, keeping them in sync without a re-read on every
 * recomposition.
 */
class Prefs(context: Context) {
    private val sp: SharedPreferences =
        context.getSharedPreferences("lasco_prefs", Context.MODE_PRIVATE)

    private val _expertMode = MutableStateFlow(sp.getBoolean(KEY_EXPERT_MODE, false))
    val expertMode: StateFlow<Boolean> = _expertMode.asStateFlow()

    fun setExpertMode(value: Boolean) {
        sp.edit().putBoolean(KEY_EXPERT_MODE, value).apply()
        _expertMode.value = value
    }

    fun recordPush(remoteId: String, success: Boolean) =
        record("lasco.lastPush", remoteId, success)

    fun recordFetch(remoteId: String, success: Boolean) =
        record("lasco.lastFetch", remoteId, success)

    fun lastPush(remoteId: String): SyncRecord? = read("lasco.lastPush", remoteId)

    fun lastFetch(remoteId: String): SyncRecord? = read("lasco.lastFetch", remoteId)

    private fun record(key: String, remoteId: String, success: Boolean) {
        sp.edit()
            .putLong("$key.$remoteId", System.currentTimeMillis())
            .putBoolean("${key}Ok.$remoteId", success)
            .apply()
    }

    private fun read(key: String, remoteId: String): SyncRecord? {
        val epochMillis = sp.getLong("$key.$remoteId", -1L)
        if (epochMillis < 0) return null
        return SyncRecord(epochMillis, sp.getBoolean("${key}Ok.$remoteId", false))
    }

    companion object {
        private const val KEY_EXPERT_MODE = "expertMode"

        @Volatile
        private var instance: Prefs? = null

        fun from(context: Context): Prefs =
            instance ?: synchronized(this) {
                instance ?: Prefs(context.applicationContext).also { instance = it }
            }
    }
}

data class SyncRecord(val epochMillis: Long, val success: Boolean)
