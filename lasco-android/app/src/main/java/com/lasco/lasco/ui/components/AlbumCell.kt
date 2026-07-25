package com.lasco.lasco.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import uniffi.lasco_ffi.FfiAlbum

/**
 * Shared album cell: thumbnail square, name, optional parent info line,
 * and a selection checkmark overlay. Used by the Albums screen grid and by
 * Media Detail's "also in these albums" grid. Mirrors Swift's AlbumCell,
 * a single lascoPanel (surfaceAlt background, 2dp ink border, no rounding).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun AlbumCell(
    album: FfiAlbum,
    repo: LibraryRepository,
    modifier: Modifier = Modifier,
    parentInfo: String? = null,
    isSelected: Boolean = false,
    onClick: (() -> Unit)? = null,
    onLongClick: (() -> Unit)? = null,
) {
    val colors = LascoTheme.colors
    Box(
        modifier = modifier
            .lascoPanel()
            .then(
                if (onClick != null || onLongClick != null) {
                    Modifier.combinedClickable(onClick = { onClick?.invoke() }, onLongClick = onLongClick)
                } else {
                    Modifier
                },
            ),
    ) {
        Column {
            MediaThumbnail(mediaId = album.thumbnailMediaId, repo = repo, modifier = Modifier.fillMaxWidth())
            Column(modifier = Modifier.padding(horizontal = 8.dp, vertical = 8.dp)) {
                Text(text = album.name, style = LascoTheme.type.body(14), color = colors.ink, maxLines = 1)
                if (parentInfo != null) {
                    Text(text = parentInfo, style = LascoTheme.type.pixel(12), color = colors.inkMuted, maxLines = 1)
                }
            }
        }
        if (isSelected) {
            Text(
                text = "✓",
                style = LascoTheme.type.body(16),
                color = colors.pink,
                modifier = Modifier.align(Alignment.TopEnd).padding(6.dp),
            )
        }
    }
}
