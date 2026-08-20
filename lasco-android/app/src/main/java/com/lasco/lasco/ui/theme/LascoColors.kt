package com.lasco.lasco.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/**
 * The Plaster palette, ported from the Swift LascoTheme. Same hex values so
 * the two apps look identical. Exposed through a CompositionLocal the same way
 * Material exposes its color scheme, so screens read LascoTheme.colors.
 */
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

val PlasterColors = LascoColors(
    bg = Color(0xFFE6E2D4),
    bgDeep = Color(0xFFD2CDBA),
    surface = Color(0xFFF3EFE2),
    surfaceAlt = Color(0xFFFFFFFF),
    ink = Color(0xFF1A1A1A),
    inkSub = Color(0xFF4A4A48),
    inkMuted = Color(0xFF8A8682),
    accent = Color(0xFF0A0F2E),
    accentPress = Color(0xFF04071A),
    ok = Color(0xFF5B8B3E),
    warn = Color(0xFFD9A23A),
    error = Color(0xFFC44A3E),
    pink = Color(0xFFE84A8A),
)

val DarkColors = LascoColors(
    bg = Color(0xFF000000),
    bgDeep = Color(0xFF111111),
    surface = Color(0xFF1A1A1A),
    surfaceAlt = Color(0xFF222222),
    ink = Color(0xFFFFFFFF),
    inkSub = Color(0xFFCCCCCC),
    inkMuted = Color(0xFF888888),
    accent = Color(0xFF0A0F2E),
    accentPress = Color(0xFF04071A),
    ok = Color(0xFF5B8B3E),
    warn = Color(0xFFD9A23A),
    error = Color(0xFFC44A3E),
    pink = Color(0xFFFFB8D9),
)

val LocalLascoColors = staticCompositionLocalOf { PlasterColors }
