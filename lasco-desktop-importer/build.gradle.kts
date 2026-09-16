import org.jetbrains.compose.desktop.application.dsl.TargetFormat
import org.gradle.api.tasks.JavaExec
import org.gradle.api.tasks.Exec

plugins {
    kotlin("jvm") version "2.4.10"
    kotlin("plugin.serialization") version "2.4.10"
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.10"
    id("org.jetbrains.compose") version "1.9.0"
}

group = "app.lasco"
version = "0.1.0"

kotlin { jvmToolchain(21) }

// UniFFI emits pure JVM/JNA Kotlin. The Android client is the canonical checked-in generated
// binding until target packaging regenerates it into build/generated/uniffi.
kotlin.sourceSets.main {
    kotlin.srcDir("../lasco-android/app/src/main/java/uniffi")
    kotlin.srcDir(layout.buildDirectory.dir("generated/uniffi"))
}

dependencies {
    implementation(compose.desktop.currentOs)
    implementation(compose.components.resources)
    implementation(compose.material3)
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
    implementation("net.java.dev.jna:jna-jpms:5.18.1")
    testImplementation(kotlin("test"))
}

compose.desktop {
    application {
        mainClass = "app.lasco.importer.MainKt"
        nativeDistributions {
            targetFormats(TargetFormat.Dmg, TargetFormat.Msi, TargetFormat.Deb, TargetFormat.Rpm)
            packageName = "lasco-desktop-importer"
            // macOS requires the first component of its package/build version to be non-zero.
            packageVersion = "1.0.0"
            description = "Import Google Takeout and Apple Photos into a Lasco library"
            vendor = "Lasco"
            macOS {
                // Used by the Finder and Dock for the packaged macOS application.
                iconFile.set(project.file("src/main/resources/lasco.icns"))
                // PhotoKit/TCC registers a macOS app bundle, not a Gradle or JDK process.
                // This text is required for the system permission prompt and privacy list.
                bundleID = "app.lasco.desktopimporter"
                entitlementsFile.set(project.file("entitlements.plist"))
                infoPlist {
                    extraKeysRawXml = """
                        <key>NSPhotoLibraryUsageDescription</key>
                        <string>Lasco needs access to import the photos and videos you select.</string>
                    """.trimIndent()
                }
            }
        }
    }
}

tasks.withType<JavaExec>().configureEach {
    // Packaged launchers default to release behavior; only the local development `run` task
    // asks for a session-only server address for staging or local-cloud testing.
    if (name == "run") systemProperty("lasco.importer.release", "false")
}

val operatingSystem = System.getProperty("os.name").lowercase()
val architecture = when (System.getProperty("os.arch").lowercase()) {
    "aarch64", "arm64" -> "aarch64"
    "x86_64", "amd64" -> "x86-64"
    else -> error("Unsupported desktop architecture: ${System.getProperty("os.arch")}")
}

val nativePhotosBridge = if (operatingSystem.contains("mac")) {
    tasks.register<Exec>("buildNativePhotosBridge") {
        // The shared package exports the existing narrow C/JNA symbols. Kotlin remains the only
        // desktop caller of Lasco FFI; Swift owns only PhotoKit discovery and staging policy.
        workingDir = rootProject.projectDir.parentFile.resolve("LascoPhotoImportKit")
        commandLine(
            "swift", "build", "-c", "release", "--product", "LascoPhotoImportKit",
        )
    }
} else {
    null
}

tasks.register<Exec>("generateUniffiKotlin") {
    workingDir = rootProject.projectDir.parentFile
    commandLine(
        "cargo", "run", "-p", "lasco-ffi", "--bin", "uniffi-bindgen", "--", "generate",
        "--library", "target/release/${if (System.getProperty("os.name").startsWith("Mac")) "liblasco_ffi.dylib" else "liblasco_ffi.so"}",
        "--language", "kotlin", "--out-dir", layout.buildDirectory.dir("generated/uniffi").get().asFile,
    )
}

tasks.test { useJUnitPlatform() }

// Keep desktop typography in lockstep with Lasco Android without maintaining a second copy of
// the font files. They are packaged at the root of the desktop application's resources.
tasks.processResources {
    from("../lasco-android/app/src/main/res/font")

    // UniFFI loads through JNA, which looks for platform libraries at this exact resource path.
    // This makes both `run` and packaged distributions self-contained rather than depending on
    // a dylib installed beside the JDK or in a system Frameworks directory.
    val (resourceDirectory, libraryName) = when {
        operatingSystem.contains("mac") -> "darwin-$architecture" to "liblasco_ffi.dylib"
        operatingSystem.contains("win") -> "win32-$architecture" to "lasco_ffi.dll"
        else -> "linux-$architecture" to "liblasco_ffi.so"
    }
    val ffiLibrary = rootProject.projectDir.parentFile.resolve("target/release/$libraryName")
    inputs.file(ffiLibrary)
    from(ffiLibrary) { into(resourceDirectory) }

    // Apple Photos is macOS-only. The FFI-free Swift package exports the narrow JNA transport
    // and is packaged beside Lasco FFI for the desktop JVM.
    if (operatingSystem.contains("mac")) {
        val photosBridge = layout.projectDirectory.file(
            "../LascoPhotoImportKit/.build/release/libLascoPhotoImportKit.dylib",
        )
        nativePhotosBridge?.also { bridgeTask -> dependsOn(bridgeTask) }
        inputs.file(photosBridge)
        from(photosBridge) { into(resourceDirectory) }
    }
    doFirst {
        check(ffiLibrary.isFile) {
            "Missing $libraryName. Build the host FFI first: cargo build -p lasco-ffi --release"
        }
    }
}
