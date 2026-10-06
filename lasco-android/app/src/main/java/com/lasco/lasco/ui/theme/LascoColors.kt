package com.lasco.lasco.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/** Website palette, shared with the Swift and desktop clients. */
@Immutable
data class LascoColors(
    val bg: Color,
    val bgDeep: Color,
    val surface: Color,
    val surfaceAlt: Color,
    val ink: Color,
    val inkSub: Color,
    val inkMuted: Color,
    val accent: Color,
    val accentPress: Color,
    val ok: Color,
    val warn: Color,
    val error: Color,
    val pink: Color,
)

val DarkColors = LascoColors(
    bg = Color(0xFF111315),
    bgDeep = Color(0xFF191C1F),
    surface = Color(0xFF191C1F),
    surfaceAlt = Color(0xFF22262A),
    ink = Color(0xFFF1F0EB),
    inkSub = Color(0xFFB2B8B9),
    inkMuted = Color(0xFFB2B8B9),
    accent = Color(0xFFF4B8D5),
    accentPress = Color(0xFFDC8EB5),
    ok = Color(0xFFA5D7AA),
    warn = Color(0xFFEFC879),
    error = Color(0xFFF3A5A5),
    pink = Color(0xFFF4B8D5),
)

val LascoBorder = Color(0xFF343A3B)
val LocalLascoColors = staticCompositionLocalOf { DarkColors }
