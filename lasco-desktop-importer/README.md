# Lasco Desktop Importer

A Compose Desktop/JVM importer for an existing Lasco library. It supports Google Takeout on macOS,
Windows, and Linux, and the local Apple Photos/iCloud Photos library on macOS.

The importer has its own app-support directory and SQLite manifest. It never shares a local Lasco
cache with another client process. The manifest stores discovered source identifiers, staged paths,
dependency order, imported media IDs, per-remote push completion, and the current chunk. Closing
the app is therefore safe: resume continues from the manifest without re-enumerating Photos.

## Build prerequisites

This project intentionally does not bootstrap Kotlin/Gradle tooling. Build it with an existing
Gradle installation:

```sh
gradle run
gradle packageDistributionForCurrentOS
```

Before packaging, build `lasco-ffi` for the target and arrange for the generated Kotlin binding
and native library to be bundled. `src/main/kotlin/app/lasco/importer/ffi/UniffiLascoGateway.kt`
is the only production boundary that imports UniFFI/JNA types. Generate bindings with:

```sh
cargo run -p lasco-ffi --bin uniffi-bindgen -- generate \
  --library target/release/liblasco_ffi.dylib --language kotlin --out-dir build/generated/uniffi
```

Add `build/generated/uniffi` to the main Kotlin source set and the target `lasco_ffi` dynamic
library to the application distribution. The macOS PhotoKit bridge is separately built as a
Kotlin/Native dynamic library in `native-photos-bridge` and loaded only on macOS.

## Import guarantees

- A source resource is staged once, then imported once through `lasco-ffi`; each selected remote
  receives the same imported media during the push fan-out.
- An edited Live Photo is imported in the required order: AAE sidecar, paired video, then still
  with both resulting media IDs supplied as metadata.
- Source filenames and timestamps are retained. Google JSON sidecars contribute captured time and
  GPS when present.
- A pause request finishes the current chunk, pushes it, records the checkpoint, and then stops.
- Exact duplicate detection remains authoritative in Rust: content hashes are known only after the
  source bytes are staged. The recap excludes already-completed manifest records and labels other
  items as candidates until the core reports its exact hash result.
