plugins {
    kotlin("multiplatform") version "2.4.10"
    kotlin("plugin.serialization") version "2.4.10"
}

kotlin {
    macosArm64 { binaries { sharedLib { baseName = "lasco_photos_bridge" } } }
    macosX64 { binaries { sharedLib { baseName = "lasco_photos_bridge" } } }
    sourceSets { macosMain.dependencies { implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0") } }
}
