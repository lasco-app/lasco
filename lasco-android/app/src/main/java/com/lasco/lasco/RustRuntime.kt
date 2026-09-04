package com.lasco.lasco

import android.content.Context

/** Initializes Rust components that need Android's application context. */
internal object RustRuntime {
    init {
        System.loadLibrary("lasco_ffi")
    }

    external fun nativeInitialize(context: Context)
}
