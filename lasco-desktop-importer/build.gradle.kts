import org.jetbrains.compose.desktop.application.dsl.TargetFormat

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
    implementation(compose.material3)
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
    implementation("net.java.dev.jna:jna-jpms:5.18.1")
    implementation("org.xerial:sqlite-jdbc:3.50.3.0")
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
        }
    }
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
    val operatingSystem = System.getProperty("os.name").lowercase()
    val architecture = when (System.getProperty("os.arch").lowercase()) {
        "aarch64", "arm64" -> "aarch64"
        "x86_64", "amd64" -> "x86-64"
        else -> error("Unsupported desktop architecture: ${System.getProperty("os.arch")}")
    }
    val (resourceDirectory, libraryName) = when {
        operatingSystem.contains("mac") -> "darwin-$architecture" to "liblasco_ffi.dylib"
        operatingSystem.contains("win") -> "win32-$architecture" to "lasco_ffi.dll"
        else -> "linux-$architecture" to "liblasco_ffi.so"
    }
    val ffiLibrary = rootProject.projectDir.parentFile.resolve("target/release/$libraryName")
    inputs.file(ffiLibrary)
    from(ffiLibrary) { into(resourceDirectory) }
    doFirst {
        check(ffiLibrary.isFile) {
            "Missing $libraryName. Build the host FFI first: cargo build -p lasco-ffi --release"
        }
    }
}
