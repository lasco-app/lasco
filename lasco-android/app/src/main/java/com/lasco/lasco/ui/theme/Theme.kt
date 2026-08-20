package com.lasco.lasco.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
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
fun LascoTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = if (darkTheme) DarkColors else PlasterColors

    val materialColors = if (darkTheme) {
        darkColorScheme(
            primary = colors.accent,
            background = colors.bg,
            surface = colors.surface,
            error = colors.error,
        )
    } else {
        lightColorScheme(
            primary = colors.accent,
            background = colors.bg,
            surface = colors.surface,
            error = colors.error,
        )
    }

    CompositionLocalProvider(LocalLascoColors provides colors) {
        MaterialTheme(colorScheme = materialColors, typography = Typography(), content = content)
    }
}
