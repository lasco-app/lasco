//! One-time Android runtime bootstrap for the Rust-owned SAF backend.

#![allow(unsafe_code)]

use std::{ffi::c_void, sync::OnceLock};

use jni::JNIEnv;
use jni::objects::{GlobalRef, JClass, JObject};

static ANDROID_CONTEXT: OnceLock<GlobalRef> = OnceLock::new();

/// Called once at application startup. It supplies Android's application
/// context to both the native USB backend and the Android Keyring provider.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_lasco_lasco_RustRuntime_nativeInitialize(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    context: JObject<'_>,
) {
    let Ok(vm) = env.get_java_vm() else { return };
    let Ok(context) = env.new_global_ref(context) else {
        return;
    };
    let context = ANDROID_CONTEXT.get_or_init(|| context);
    unsafe {
        ndk_context::initialize_android_context(
            vm.get_java_vm_pointer() as *mut c_void,
            context.as_obj().as_raw() as *mut c_void,
        );
    }
    let _ = lasco_core::storage::initialize_android_runtime(vm, context.clone());
}
