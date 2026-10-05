package com.lasco.lasco.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.LascoBorder
import com.lasco.lasco.ui.theme.LascoControlShape
import com.lasco.lasco.ui.theme.lascoPanel

/** Ported from the Swift FieldLabel. Uppercased caption above a field. */
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
 * subtle border and rounded corners. Used for both plain and secure entry.
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
    autoFocus: Boolean = false,
    testTag: String? = null,
) {
    val colors = LascoTheme.colors
    val focusRequester = remember { FocusRequester() }

    LaunchedEffect(autoFocus, enabled) {
        if (autoFocus && enabled) focusRequester.requestFocus()
    }

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
            modifier = Modifier
                .fillMaxWidth()
                .then(if (autoFocus) Modifier.focusRequester(focusRequester) else Modifier)
                .then(if (testTag != null) Modifier.maestroTag(testTag) else Modifier),
            decorationBox = { inner ->
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(LascoControlShape)
                        .background(colors.surfaceAlt)
                        .border(1.dp, LascoBorder, LascoControlShape)
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
 * Ported from the Swift LascoCheckbox. A rounded box that fills with pink when
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
                .background(if (checked) colors.pink else colors.surfaceAlt, RoundedCornerShape(4.dp))
                .border(1.dp, if (checked) colors.pink else LascoBorder, RoundedCornerShape(4.dp)),
            contentAlignment = Alignment.Center,
        ) {
            if (checked) {
                Text(text = "✓", style = LascoTheme.type.body(12), color = colors.bg)
            }
        }
        Text(text = label, style = LascoTheme.type.body(13), color = colors.inkSub)
    }
}

/** Rounded native switch with the shared pink accent. */
@Composable
fun LascoToggle(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    androidx.compose.material3.Switch(
        checked = checked,
        onCheckedChange = onCheckedChange,
        modifier = modifier,
        colors = androidx.compose.material3.SwitchDefaults.colors(
            checkedTrackColor = colors.pink,
            checkedThumbColor = colors.bg,
            checkedBorderColor = colors.pink,
            uncheckedTrackColor = colors.surfaceAlt,
            uncheckedThumbColor = colors.inkSub,
            uncheckedBorderColor = LascoBorder,
        ),
    )
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
            .clip(LascoControlShape)
            .background(colors.error.copy(alpha = 0.08f))
            .border(1.dp, colors.error, LascoControlShape)
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
