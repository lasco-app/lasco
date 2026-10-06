package com.lasco.lasco.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable

/**
 * The Lasco design theme. It mirrors the Swift LascoTheme, exposing the palette
 * and typography through a CompositionLocal. Screens read them through the
 * LascoTheme object, the same shape as Material's MaterialTheme.
 */
object LascoTheme {
    val colors: LascoColors
        @Composable
        @ReadOnlyComposable
        get() = LocalLascoColors.current

    val type = LascoType
}

/**
 * Wraps content in the Lasco palette. A minimal MaterialTheme sits underneath
 * so ripples, text selection and other Material defaults pick up sensible
 * colors, but all app styling goes through LascoTheme.colors and LascoType.
 */
@Composable
fun LascoTheme(content: @Composable () -> Unit) {
    val colors = DarkColors
    val materialColors = darkColorScheme(
        primary = colors.accent, onPrimary = colors.bg,
        primaryContainer = colors.surfaceAlt, onPrimaryContainer = colors.pink,
        secondary = colors.pink, onSecondary = colors.bg,
        background = colors.bg, onBackground = colors.ink,
        surface = colors.surface, onSurface = colors.ink,
        surfaceVariant = colors.surfaceAlt, onSurfaceVariant = colors.inkSub,
        outline = LascoBorder, outlineVariant = LascoBorder,
        error = colors.error, onError = colors.bg,
        surfaceTint = colors.surface,
    )
    CompositionLocalProvider(LocalLascoColors provides colors) {
        MaterialTheme(
            colorScheme = materialColors,
            typography = Typography().run {
                copy(
                    displayLarge = displayLarge.copy(fontFamily = SpaceGrotesk),
                    displayMedium = displayMedium.copy(fontFamily = SpaceGrotesk),
                    displaySmall = displaySmall.copy(fontFamily = SpaceGrotesk),
                    headlineLarge = headlineLarge.copy(fontFamily = SpaceGrotesk),
                    headlineMedium = headlineMedium.copy(fontFamily = SpaceGrotesk),
                    headlineSmall = headlineSmall.copy(fontFamily = SpaceGrotesk),
                    titleLarge = titleLarge.copy(fontFamily = SpaceGrotesk),
                    titleMedium = titleMedium.copy(fontFamily = SpaceGrotesk),
                    titleSmall = titleSmall.copy(fontFamily = SpaceGrotesk),
                    bodyLarge = bodyLarge.copy(fontFamily = SpaceGrotesk),
                    bodyMedium = bodyMedium.copy(fontFamily = SpaceGrotesk),
                    bodySmall = bodySmall.copy(fontFamily = SpaceGrotesk),
                    labelLarge = labelLarge.copy(fontFamily = SpaceGrotesk),
                    labelMedium = labelMedium.copy(fontFamily = SpaceGrotesk),
                    labelSmall = labelSmall.copy(fontFamily = SpaceGrotesk),
                )
            },
            shapes = Shapes(
                extraSmall = RoundedCornerShape(4.dp), small = RoundedCornerShape(8.dp),
                medium = RoundedCornerShape(16.dp), large = RoundedCornerShape(16.dp),
                extraLarge = RoundedCornerShape(24.dp),
            ),
            content = content,
        )
    }
}
