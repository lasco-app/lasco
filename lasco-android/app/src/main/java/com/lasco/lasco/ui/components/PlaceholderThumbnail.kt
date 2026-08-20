package com.lasco.lasco.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.ui.Modifier
import androidx.compose.runtime.Composable
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * Shared placeholder used everywhere a thumbnail slot exists but no image has
 * been loaded yet. Matches the bgDeep square Swift shows in MediaGridCell and
 * AlbumCell before thumbnailAsync resolves. Used directly by MediaThumbnail
 * while a fetch is in flight.
 */
@Composable
fun Modifier.placeholderThumbnail(): Modifier {
    val colors = LascoTheme.colors
    return this
        .aspectRatio(1f)
        .background(colors.bgDeep)
}
