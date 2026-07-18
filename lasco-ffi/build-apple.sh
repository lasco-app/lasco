#!/usr/bin/env bash
# Builds lasco-ffi for Apple targets and packages the result as an XCFramework.
# Run from the workspace root: ./lasco-ffi/build-apple.sh
set -euo pipefail

CRATE="lasco-ffi"
LIB_NAME="liblasco_ffi"
NAMESPACE="lasco_ffi"
WORKSPACE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FFI_DIR="$WORKSPACE_ROOT/lasco-ffi"
ARTIFACTS="$FFI_DIR/artifacts"
XCODE_PROJECT="$WORKSPACE_ROOT/lasco-swift"
XCODE_SOURCES="$XCODE_PROJECT/Lasco"

# ── 1. Build ──────────────────────────────────────────────────────────────────

echo "==> Building for aarch64-apple-ios (device)"
cargo build -p "$CRATE" --target aarch64-apple-ios --release

echo "==> Building for aarch64-apple-ios-sim (Apple Silicon simulator)"
cargo build -p "$CRATE" --target aarch64-apple-ios-sim --release

echo "==> Building for x86_64-apple-ios (Intel simulator)"
cargo build -p "$CRATE" --target x86_64-apple-ios --release

echo "==> Building for aarch64-apple-darwin"
cargo build -p "$CRATE" --target aarch64-apple-darwin --release

echo "==> Building for x86_64-apple-darwin"
cargo build -p "$CRATE" --target x86_64-apple-darwin --release

# ── 2. Lipo fat binaries ──────────────────────────────────────────────────────

echo "==> Creating macOS fat binary"
mkdir -p "$ARTIFACTS/macos"
lipo -create \
    "$WORKSPACE_ROOT/target/aarch64-apple-darwin/release/${LIB_NAME}.a" \
    "$WORKSPACE_ROOT/target/x86_64-apple-darwin/release/${LIB_NAME}.a" \
    -output "$ARTIFACTS/macos/${LIB_NAME}.a"

echo "==> Creating iOS Simulator fat binary"
mkdir -p "$ARTIFACTS/ios-sim"
lipo -create \
    "$WORKSPACE_ROOT/target/aarch64-apple-ios-sim/release/${LIB_NAME}.a" \
    "$WORKSPACE_ROOT/target/x86_64-apple-ios/release/${LIB_NAME}.a" \
    -output "$ARTIFACTS/ios-sim/${LIB_NAME}.a"

# ── 3. Generate Swift bindings ────────────────────────────────────────────────

echo "==> Generating Swift bindings via uniffi-bindgen"
DYLIB="$WORKSPACE_ROOT/target/aarch64-apple-darwin/release/${LIB_NAME}.dylib"
BINDGEN_OUT="$ARTIFACTS/swift"
mkdir -p "$BINDGEN_OUT"

cargo run --bin uniffi-bindgen -- generate \
    --library "$DYLIB" \
    --language swift \
    --out-dir "$BINDGEN_OUT"

# uniffi-bindgen emits <Namespace>.swift and <Namespace>FFI.h + <Namespace>FFI.modulemap
SWIFT_FILE="$BINDGEN_OUT/${NAMESPACE}.swift"
HEADER_FILE="$BINDGEN_OUT/${NAMESPACE}FFI.h"
MODULEMAP_FILE="$BINDGEN_OUT/${NAMESPACE}FFI.modulemap"

# uniffi-bindgen's generated async plumbing (C function pointer callbacks, deinit)
# cannot be actor-isolated, but this Xcode target sets
# SWIFT_DEFAULT_ACTOR_ISOLATION = MainActor, which implicitly isolates every
# top-level declaration in every compiled file, including this generated one.
# Force every top-level declaration back to nonisolated until upstream fixes this:
# https://github.com/mozilla/uniffi-rs/issues/2818
echo "==> Marking generated Swift declarations nonisolated (uniffi-rs#2818 workaround)"
sed -i '' \
    -e 's/^fileprivate /nonisolated fileprivate /' \
    -e 's/^private /nonisolated private /' \
    -e 's/^public /nonisolated public /' \
    -e 's/^open /nonisolated open /' \
    -e 's/^extension /nonisolated extension /' \
    "$SWIFT_FILE"

# ── 4. Assemble XCFramework ───────────────────────────────────────────────────

echo "==> Assembling XCFramework"
XCFW="$ARTIFACTS/LascoFFI.xcframework"
rm -rf "$XCFW"

# Copy headers into per-slice directories
IOS_HEADERS="$ARTIFACTS/ios-headers"
IOS_SIM_HEADERS="$ARTIFACTS/ios-sim-headers"
MACOS_HEADERS="$ARTIFACTS/macos-headers"
mkdir -p "$IOS_HEADERS" "$IOS_SIM_HEADERS" "$MACOS_HEADERS"
cp "$HEADER_FILE" "$IOS_HEADERS/"
cp "$MODULEMAP_FILE" "$IOS_HEADERS/"
cp "$HEADER_FILE" "$IOS_SIM_HEADERS/"
cp "$MODULEMAP_FILE" "$IOS_SIM_HEADERS/"
cp "$HEADER_FILE" "$MACOS_HEADERS/"
cp "$MODULEMAP_FILE" "$MACOS_HEADERS/"

xcodebuild -create-xcframework \
    -library "$WORKSPACE_ROOT/target/aarch64-apple-ios/release/${LIB_NAME}.a" \
    -headers "$IOS_HEADERS" \
    -library "$ARTIFACTS/ios-sim/${LIB_NAME}.a" \
    -headers "$IOS_SIM_HEADERS" \
    -library "$ARTIFACTS/macos/${LIB_NAME}.a" \
    -headers "$MACOS_HEADERS" \
    -output "$XCFW"

# ── 5. Copy outputs into lasco-swift ─────────────────────────────────────────

echo "==> Copying artifacts to lasco-swift/"
rm -rf "$XCODE_PROJECT/LascoFFI.xcframework"
cp -R "$XCFW" "$XCODE_PROJECT/"
cp "$SWIFT_FILE" "$XCODE_SOURCES/"

echo ""
echo "Done. Artifacts written to:"
echo "  $XCODE_PROJECT/LascoFFI.xcframework"
echo "  $XCODE_SOURCES/lasco_ffi.swift"
