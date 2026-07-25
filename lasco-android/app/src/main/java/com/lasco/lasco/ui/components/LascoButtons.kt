package com.lasco.lasco.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * Win98 style bevel, ported from the Swift bevel overlay. A 2dp ink frame with
 * a 1dp highlight on the top and leading edges and a 1dp shadow on the bottom
 * and trailing edges. The highlight and shadow swap when pressed.
 */
private fun Modifier.lascoBevel(pressed: Boolean, hi: Color, lo: Color, ink: Color): Modifier =
    drawWithContent {
        drawContent()
        val two = 2.dp.toPx()
        val one = 1.dp.toPx()
        val w = size.width
        val h = size.height

        // Ink frame, inset so the 2dp stroke stays inside the bounds.
        drawRect(
            color = ink,
            topLeft = androidx.compose.ui.geometry.Offset(one, one),
            size = androidx.compose.ui.geometry.Size(w - two, h - two),
            style = androidx.compose.ui.graphics.drawscope.Stroke(width = two),
        )

        val topLeftEdge = if (pressed) lo else hi
        val bottomRightEdge = if (pressed) hi else lo
        val inset = two

        // Top and leading highlight.
        drawLine(topLeftEdge, androidx.compose.ui.geometry.Offset(inset, inset), androidx.compose.ui.geometry.Offset(w - inset, inset), one)
        drawLine(topLeftEdge, androidx.compose.ui.geometry.Offset(inset, inset), androidx.compose.ui.geometry.Offset(inset, h - inset), one)
        // Bottom and trailing shadow.
        drawLine(bottomRightEdge, androidx.compose.ui.geometry.Offset(inset, h - inset), androidx.compose.ui.geometry.Offset(w - inset, h - inset), one)
        drawLine(bottomRightEdge, androidx.compose.ui.geometry.Offset(w - inset, inset), androidx.compose.ui.geometry.Offset(w - inset, h - inset), one)
    }

@Composable
private fun BeveledButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier,
    enabled: Boolean,
    background: (pressed: Boolean) -> Color,
    hi: Color,
    lo: Color,
    contentColor: Color,
    fillWidth: Boolean,
) {
    val colors = LascoTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()

    Box(
        modifier = modifier
            .then(if (fillWidth) Modifier.fillMaxWidth() else Modifier)
            .alpha(if (enabled) 1f else 0.45f)
            .background(background(pressed))
            .lascoBevel(pressed, hi = hi, lo = lo, ink = colors.ink)
            .clickable(
                enabled = enabled,
                interactionSource = interaction,
                indication = null,
            ) { onClick() }
            .padding(horizontal = 20.dp, vertical = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = text, style = LascoTheme.type.button, color = contentColor)
    }
}

/** Ported from LascoPrimaryButtonStyle. Dark accent, white label. */
@Composable
fun LascoPrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    fillWidth: Boolean = true,
) {
    val colors = LascoTheme.colors
    BeveledButton(
        text = text,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        background = { pressed -> if (pressed) colors.accentPress else colors.accent },
        hi = Color(0xFF2A3060),
        lo = Color(0xFF000000),
        contentColor = Color.White,
        fillWidth = fillWidth,
    )
}

/** Ported from LascoSecondaryButtonStyle. Plaster background, ink label. */
@Composable
fun LascoSecondaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    fillWidth: Boolean = true,
) {
    val colors = LascoTheme.colors
    BeveledButton(
        text = text,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        background = { pressed -> if (pressed) colors.bgDeep else colors.bg },
        hi = colors.surfaceAlt,
        lo = colors.inkSub,
        contentColor = colors.ink,
        fillWidth = fillWidth,
    )
}
