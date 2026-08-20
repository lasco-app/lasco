package com.lasco.lasco.data

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.lasco_ffi.FfiLibraryId
import uniffi.lasco_ffi.FfiRemoteUuid

/**
 * Small local settings store, the Android equivalent of Swift's
 * UserDefaults backed AppStorage. Holds expert mode and the last push/fetch
 * timestamps per remote, none of which is FFI state so it does not belong
 * on SessionState.
 *
 * One instance lives for the process lifetime (see companion), so everything
 * here is exposed as a StateFlow that every screen observes rather than a
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

    fun onboardingCheckpoint(libraryId: FfiLibraryId): WizardCheckpoint? {
        return sp.getString("$KEY_ONBOARDING_CHECKPOINT.${libraryId.value}", null)
            ?.let { value -> WizardCheckpoint.entries.firstOrNull { it.name == value } }
    }

    fun setOnboardingCheckpoint(libraryId: FfiLibraryId, checkpoint: WizardCheckpoint) {
        sp.edit().putString("$KEY_ONBOARDING_CHECKPOINT.${libraryId.value}", checkpoint.name).apply()
    }

    fun clearOnboardingIncomplete(libraryId: FfiLibraryId) {
        sp.edit().remove("$KEY_ONBOARDING_CHECKPOINT.${libraryId.value}").apply()
    }

    // DATE_ADDED of the newest device media row imported so far, the Android
    // equivalent of PhotoLibraryImporter's lastImportDate. Used by the
    // incremental import path to scan only newer rows.
    fun importWatermark(libraryId: FfiLibraryId): Long? {
        val value = sp.getLong("$KEY_IMPORT_WATERMARK.${libraryId.value}", -1L)
        return if (value < 0) null else value
    }

    fun setImportWatermark(libraryId: FfiLibraryId, dateAdded: Long) {
        sp.edit().putLong("$KEY_IMPORT_WATERMARK.${libraryId.value}", dateAdded).apply()
    }

    // Stamps the watermark with the current time, for the paths that end
    // without importing anything, the two wizard skips and a failed import.
    // Without it the watermark stays null and the incremental import has no
    // floor to scan from, so it would treat the whole camera folder as new.
    // Seconds, to match the MediaStore DATE_ADDED it is compared against.
    // An already stored watermark is left alone, a real import knows better.
    fun baselineImportWatermark(libraryId: FfiLibraryId) {
        if (importWatermark(libraryId) != null) return
        setImportWatermark(libraryId, System.currentTimeMillis() / 1000)
    }

    private val _lastPush = MutableStateFlow(readAll(KEY_LAST_PUSH))
    val lastPush: StateFlow<Map<FfiRemoteUuid, SyncRecord>> = _lastPush.asStateFlow()

    private val _lastFetch = MutableStateFlow(readAll(KEY_LAST_FETCH))
    val lastFetch: StateFlow<Map<FfiRemoteUuid, SyncRecord>> = _lastFetch.asStateFlow()

    fun recordPush(remoteId: FfiRemoteUuid, success: Boolean) =
        record(KEY_LAST_PUSH, _lastPush, remoteId, success)

    fun recordFetch(remoteId: FfiRemoteUuid, success: Boolean) =
        record(KEY_LAST_FETCH, _lastFetch, remoteId, success)

    private fun record(
        key: String,
        flow: MutableStateFlow<Map<FfiRemoteUuid, SyncRecord>>,
        remoteId: FfiRemoteUuid,
        success: Boolean,
    ) {
        val record = SyncRecord(System.currentTimeMillis(), success)
        sp.edit()
            .putLong("$key.${remoteId.value}", record.epochMillis)
            .putBoolean("${key}Ok.${remoteId.value}", record.success)
            .apply()
        // Called from the push and fetch coroutines on the io dispatcher, so
        // this has to be an atomic update rather than a read then write.
        flow.update { it + (remoteId to record) }
    }

    // The remote ids are not known here, so the stored keys say which remotes
    // have a record.
    private fun readAll(key: String): Map<FfiRemoteUuid, SyncRecord> {
        val prefix = "$key."
        return sp.all.keys
            .filter { it.startsWith(prefix) }
            .associate { storedKey ->
                val remoteId = FfiRemoteUuid(storedKey.removePrefix(prefix))
                remoteId to SyncRecord(
                    epochMillis = sp.getLong(storedKey, 0L),
                    success = sp.getBoolean("${key}Ok.${remoteId.value}", false),
                )
            }
    }

    companion object {
        private const val KEY_EXPERT_MODE = "expertMode"
        private const val KEY_ONBOARDING_CHECKPOINT = "onboardingCheckpoint"
        private const val KEY_IMPORT_WATERMARK = "importWatermark"
        private const val KEY_LAST_PUSH = "lastPush"
        private const val KEY_LAST_FETCH = "lastFetch"

        @Volatile
        private var instance: Prefs? = null

        fun from(context: Context): Prefs =
            instance ?: synchronized(this) {
                instance ?: Prefs(context.applicationContext).also { instance = it }
            }
    }
}

enum class WizardCheckpoint {
    AddRemote,
    ChooseDeviceImport,
    GrantMediaAccess,
    GrantLocationAccess,
    ImportDeviceMedia,
    ChooseAutoImport,
}

data class SyncRecord(val epochMillis: Long, val success: Boolean)
