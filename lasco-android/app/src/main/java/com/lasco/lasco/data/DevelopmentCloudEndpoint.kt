package com.lasco.lasco.data

import android.content.Context
import com.lasco.lasco.BuildConfig

/** Debug-only Lasco Cloud endpoint selected when the app launches. */
internal object DevelopmentCloudEndpoint {
    private const val preferencesName = "lasco_development"
    private const val endpointKey = "cloud_endpoint"
    const val defaultUrl = "http://localhost:3000"

    fun url(context: Context): String =
        context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)
            .getString(endpointKey, defaultUrl)
            ?: defaultUrl

    fun setUrl(context: Context, value: String) {
        val normalized = value.trim().let { if ("://" in it) it else "http://$it" }.trimEnd('/')
        context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)
            .edit()
            .putString(endpointKey, normalized)
            .apply()
    }

    fun activeUrl(context: Context): String =
        if (BuildConfig.DEBUG) url(context) else BuildConfig.LASCO_CLOUD_URL
}
