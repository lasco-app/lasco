package com.lasco.lasco.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * Panel styling ported from the Swift LascoFlatPanel. No border radius ever,
 * a 2dp ink border over the surfaceAlt background. In the Swift app the flat
 * and hard shadow panels render identically, so both map here.
 */
@Composable
fun Modifier.lascoPanel(): Modifier {
    val colors = LascoTheme.colors
    return this
        .background(colors.surfaceAlt)
        .border(2.dp, colors.ink)
}

/**
 * Same as lascoPanel, kept as a separate name to match the Swift call sites
 * (lascoPanelHard) so ports read one to one.
 */
@Composable
fun Modifier.lascoPanelHard(): Modifier = lascoPanel()
