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
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.heightIn
import com.lasco.lasco.ui.theme.LascoControlShape
import com.lasco.lasco.ui.theme.LascoBorder
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.lasco.lasco.ui.theme.LascoTheme

@Composable
private fun RoundedButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier,
    enabled: Boolean,
    background: (pressed: Boolean) -> Color,
    contentColor: Color,
    fillWidth: Boolean,
) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()

    Box(
        modifier = modifier
            .then(if (fillWidth) Modifier.fillMaxWidth() else Modifier)
            .alpha(if (enabled) 1f else 0.45f)
            .clip(LascoControlShape)
            .background(background(pressed))
            .border(1.dp, LascoBorder, LascoControlShape)
            .heightIn(min = 48.dp)
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

/** Ported from LascoPrimaryButtonStyle. Pink accent, dark label. */
@Composable
fun LascoPrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    fillWidth: Boolean = true,
) {
    val colors = LascoTheme.colors
    RoundedButton(
        text = text,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        background = { pressed -> if (pressed) colors.accentPress else colors.accent },
        contentColor = colors.bg,
        fillWidth = fillWidth,
    )
}

/** Ported from LascoSecondaryButtonStyle. Dark surface, light label. */
@Composable
fun LascoSecondaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    fillWidth: Boolean = true,
) {
    val colors = LascoTheme.colors
    RoundedButton(
        text = text,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        background = { pressed -> if (pressed) colors.surfaceAlt else colors.surface },
        contentColor = colors.ink,
        fillWidth = fillWidth,
    )
}
