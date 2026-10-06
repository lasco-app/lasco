package com.lasco.lasco.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp

val LascoPanelShape = RoundedCornerShape(16.dp)
val LascoControlShape = RoundedCornerShape(9.dp)

/** Rounded panels clip album artwork; photo-grid cells never use this modifier. */
@Composable
fun Modifier.lascoPanel(): Modifier = this
    .clip(LascoPanelShape)
    .background(LascoTheme.colors.surface)
    .border(1.dp, LascoBorder, LascoPanelShape)

@Composable
fun Modifier.lascoPanelHard(): Modifier = lascoPanel()
