package com.lasco.lasco.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiMediaUuid

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

    // Mirrors Swift's albumThumbnailMediaId: explicit thumbnail if set,
    // otherwise the most recently dated media item in the album.
    var thumbnailMediaId by remember(album.albumId, album.thumbnailMediaId) {
        mutableStateOf(album.thumbnailMediaId)
    }
    LaunchedEffect(album.albumId, album.thumbnailMediaId) {
        if (album.thumbnailMediaId == null) {
            thumbnailMediaId = repo.mediaInAlbum(album.albumId).maxByOrNull { it.date }?.mediaId
        }
    }

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
            MediaThumbnail(mediaId = thumbnailMediaId, repo = repo, modifier = Modifier.fillMaxWidth())
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

/** A root-level Trash entry, styled to match an album card but with a pink outline. */
@Composable
fun TrashAlbumCard(
    repo: LibraryRepository,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    var thumbnailMediaIds by remember { mutableStateOf<List<FfiMediaUuid>>(emptyList()) }

    // This collage is intentionally a lightweight snapshot. The full Trash
    // screen remains the live, change-observing source of truth.
    LaunchedEffect(repo) {
        thumbnailMediaIds = repo.trashedMediaByDate(offset = 0, limit = 9).map { it.mediaId }
    }

    Column(
        modifier = modifier
            .background(colors.surfaceAlt)
            .border(2.dp, colors.pink)
            .combinedClickable(onClick = onClick),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(1f)
                .background(colors.bgDeep),
        ) {
            Column(modifier = Modifier.fillMaxSize()) {
                val cells = thumbnailMediaIds.take(9) + List((9 - thumbnailMediaIds.size).coerceAtLeast(0)) { null }
                repeat(3) { row ->
                    Row(modifier = Modifier.fillMaxWidth().weight(1f)) {
                        repeat(3) { column ->
                            val mediaId = cells[row * 3 + column]
                            MediaThumbnail(
                                mediaId = mediaId,
                                repo = repo,
                                modifier = Modifier.weight(1f).fillMaxHeight(),
                            )
                            if (column < 2) Spacer(modifier = Modifier.padding(1.dp))
                        }
                    }
                    if (row < 2) Spacer(modifier = Modifier.padding(1.dp))
                }
            }
        }
        Text(
            text = "Trash",
            style = LascoTheme.type.body(14),
            color = colors.ink,
            maxLines = 1,
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 8.dp),
        )
    }
}
