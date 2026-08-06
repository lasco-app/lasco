//! One-time Android runtime bootstrap for the Rust-owned SAF backend.

#![allow(unsafe_code)]

use jni::objects::{JClass, JObject};
use jni::JNIEnv;

/// Called by `UsbRustRuntime.nativeInitialize(applicationContext)` once after
/// the app process starts. Storage operations themselves never call Kotlin.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_lasco_lasco_UsbRustRuntime_nativeInitialize(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    context: JObject<'_>,
) {
    let Ok(vm) = env.get_java_vm() else { return };
    let Ok(context) = env.new_global_ref(context) else { return };
    let _ = lasco_core::storage::initialize_android_runtime(vm, context);
}
