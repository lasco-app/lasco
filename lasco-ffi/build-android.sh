#!/usr/bin/env bash
# Builds lasco-ffi for Android targets and drops the shared objects plus the
# generated Kotlin bindings into the lasco-android Gradle project.
# Run from the workspace root: ./lasco-ffi/build-android.sh
#
# Requirements:
#   - Android NDK (set ANDROID_NDK_HOME, or ANDROID_HOME/ANDROID_SDK_ROOT with an
#     ndk/<version> installed, which this script will autodetect).
#   - cargo-ndk (cargo install cargo-ndk). Installed automatically if missing.
#   - The android rustup targets. Added automatically if missing.
set -euo pipefail

CRATE="lasco-ffi"
LIB_NAME="liblasco_ffi"
WORKSPACE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_PROJECT="$WORKSPACE_ROOT/lasco-android"
JNI_LIBS="$ANDROID_PROJECT/app/src/main/jniLibs"
KOTLIN_OUT="$ANDROID_PROJECT/app/src/main/java"

# Matches the app module minSdk.
MIN_SDK=24

# cargo-ndk maps each ABI to its rustup target and jniLibs/<abi> dir.
ABIS=(arm64-v8a armeabi-v7a x86_64)
RUST_TARGETS=(aarch64-linux-android armv7-linux-androideabi x86_64-linux-android)

# ── 1. Toolchain checks ───────────────────────────────────────────────────────

echo "==> Locating Android NDK"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
    SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"
    if [[ -d "$SDK_ROOT/ndk" ]]; then
        ANDROID_NDK_HOME="$SDK_ROOT/ndk/$(ls "$SDK_ROOT/ndk" | sort -V | tail -1)"
    fi
fi
if [[ -z "${ANDROID_NDK_HOME:-}" || ! -d "$ANDROID_NDK_HOME" ]]; then
    echo "error: Android NDK not found. Set ANDROID_NDK_HOME to your NDK install," >&2
    echo "       or install one via Android Studio / sdkmanager." >&2
    exit 1
fi
export ANDROID_NDK_HOME
echo "    using NDK at $ANDROID_NDK_HOME"

echo "==> Ensuring cargo-ndk is installed"
if ! cargo ndk --version >/dev/null 2>&1; then
    cargo install cargo-ndk
fi

echo "==> Ensuring android rustup targets are installed"
for t in "${RUST_TARGETS[@]}"; do
    rustup target add "$t"
done

# ── 2. Build the shared objects ───────────────────────────────────────────────

echo "==> Building $CRATE for ${ABIS[*]}"
mkdir -p "$JNI_LIBS"

ndk_args=()
for abi in "${ABIS[@]}"; do
    ndk_args+=(-t "$abi")
done

cargo ndk "${ndk_args[@]}" --platform "$MIN_SDK" -o "$JNI_LIBS" \
    build -p "$CRATE" --release

# ── 3. Generate Kotlin bindings ───────────────────────────────────────────────

echo "==> Generating Kotlin bindings via uniffi-bindgen"
# uniffi metadata is platform independent, so we use a host build rather than
# an android .so. The release profile sets strip = true, which on ELF removes
# the static symbol table uniffi library mode reads, so a stripped android
# .so yields zero bindings with no error. The host Mach-O survives stripping.
echo "==> Building $CRATE for the host to source uniffi metadata"
cargo build -p "$CRATE" --release

HOST_DIR="$WORKSPACE_ROOT/target/release"
if [[ -f "$HOST_DIR/${LIB_NAME}.dylib" ]]; then
    HOST_LIB="$HOST_DIR/${LIB_NAME}.dylib"   # macOS
else
    HOST_LIB="$HOST_DIR/${LIB_NAME}.so"      # Linux
fi
mkdir -p "$KOTLIN_OUT"

cargo run --bin uniffi-bindgen -- generate \
    --library "$HOST_LIB" \
    --language kotlin \
    --out-dir "$KOTLIN_OUT"

echo ""
echo "Done. Artifacts written to:"
echo "  $JNI_LIBS/<abi>/${LIB_NAME}.so"
echo "  $KOTLIN_OUT/uniffi/${LIB_NAME#lib}/  (generated Kotlin)"
