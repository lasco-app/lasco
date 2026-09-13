import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    kotlin("jvm") version "2.4.10"
    kotlin("plugin.serialization") version "2.4.10"
    id("org.jetbrains.compose") version "1.9.0"
}

group = "app.lasco"
version = "0.1.0"

repositories { mavenCentral() }

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
            packageVersion = project.version.toString()
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
