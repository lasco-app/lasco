package com.lasco.lasco.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel

/** Ported from the Swift FieldLabel. Uppercased pixel caption above a field. */
@Composable
fun FieldLabel(text: String, size: Int = 11) {
    val colors = LascoTheme.colors
    Text(
        text = text.uppercase(),
        style = LascoTheme.type.categorySmall(size).copy(letterSpacing = 1.5.sp),
        color = colors.inkSub,
    )
}

/**
 * A labelled text field styled like the Swift lascoInput. Flat surface with a
 * 2dp ink border, no radius. Used for both plain and secure entry.
 */
@Composable
fun LascoField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    secure: Boolean = false,
    enabled: Boolean = true,
) {
    val colors = LascoTheme.colors
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        FieldLabel(text = label, size = 13)
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            enabled = enabled,
            singleLine = true,
            textStyle = LascoTheme.type.body().copy(color = colors.ink),
            cursorBrush = SolidColor(colors.pink),
            visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
            modifier = Modifier.fillMaxWidth(),
            decorationBox = { inner ->
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(colors.surfaceAlt)
                        .border(2.dp, colors.ink)
                        .padding(horizontal = 10.dp, vertical = 9.dp),
                ) {
                    if (value.isEmpty() && placeholder.isNotEmpty()) {
                        Text(
                            text = placeholder,
                            style = LascoTheme.type.body(),
                            color = colors.inkMuted,
                        )
                    }
                    inner()
                }
            },
        )
    }
}

/**
 * Ported from the Swift LascoCheckbox. A square box that fills with ink when
 * checked, next to a wrapping label.
 */
@Composable
fun LascoCheckbox(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    Row(
        modifier = modifier.clickable(interactionSource = null, indication = null) { onCheckedChange(!checked) },
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            modifier = Modifier
                .size(20.dp)
                .background(if (checked) colors.ink else colors.surfaceAlt)
                .border(2.dp, colors.ink),
            contentAlignment = Alignment.Center,
        ) {
            if (checked) {
                Text(text = "✓", style = LascoTheme.type.body(12), color = colors.bg)
            }
        }
        Text(text = label, style = LascoTheme.type.body(13), color = colors.inkSub)
    }
}

/**
 * Ported from the Swift LascoToggleStyle. A 36x22 ink bordered track with a
 * 14dp thumb that slides between leading and trailing.
 */
@Composable
fun LascoToggle(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    Box(
        modifier = modifier
            .size(width = 36.dp, height = 22.dp)
            .background(if (checked) colors.ink else colors.surfaceAlt)
            .border(2.dp, colors.ink)
            .clickable(interactionSource = null, indication = null) { onCheckedChange(!checked) },
        contentAlignment = if (checked) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Box(
            modifier = Modifier
                .padding(3.dp)
                .size(14.dp)
                .background(if (checked) colors.bg else colors.inkMuted),
        )
    }
}

/**
 * Ported from the Swift StatCard. A flat panel showing a big value over a
 * small uppercased label.
 */
@Composable
fun StatCard(
    value: String,
    label: String,
    modifier: Modifier = Modifier,
    valueColor: Color? = null,
) {
    val colors = LascoTheme.colors
    Column(
        modifier = modifier
            .fillMaxWidth()
            .lascoPanel()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(text = value, style = LascoTheme.type.title(26), color = valueColor ?: colors.ink)
        Text(
            text = label.uppercase(),
            style = LascoTheme.type.categorySmall(11).copy(letterSpacing = 1.sp),
            color = colors.inkMuted,
        )
    }
}

/** Ported from the Swift ErrorBanner. */
@Composable
fun ErrorBanner(message: String, modifier: Modifier = Modifier) {
    val colors = LascoTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(colors.error.copy(alpha = 0.08f))
            .border(1.dp, colors.error)
            .padding(10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(text = "✗", style = LascoTheme.type.mono(), color = colors.error)
        Text(
            text = message,
            style = LascoTheme.type.body(13),
            color = colors.error,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
