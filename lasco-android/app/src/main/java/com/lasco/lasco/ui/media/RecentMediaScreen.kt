package com.lasco.lasco.ui.media

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.components.AlbumPickerDialog
import com.lasco.lasco.ui.components.MediaThumbnail
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiMediaItem

/**
 * Recent media grid, the Android equivalent of the Swift ContentView home
 * screen. Thumbnails load lazily per cell, see MediaThumbnail.
 */
@Composable
fun RecentMediaScreen(
    modifier: Modifier = Modifier,
    onOpenMedia: (mediaId: String) -> Unit = {},
    onOpenAlbum: (albumId: String) -> Unit = {},
    viewModel: RecentMediaViewModel = viewModel(factory = RecentMediaViewModel.Factory),
) {
    val colors = LascoTheme.colors
    val media by viewModel.media.collectAsStateWithLifecycle()
    val repo = LibraryRepository.from(LocalContext.current)
    val sessionState by repo.sessionState.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()

    var isSelecting by remember { mutableStateOf(false) }
    var selection by remember { mutableStateOf(setOf<String>()) }
    var albumPicker by remember { mutableStateOf<List<uniffi.lasco_ffi.FfiAlbum>?>(null) }

    fun clearSelection() {
        isSelecting = false
        selection = emptySet()
    }

    fun openContainingAlbums(mediaId: String) {
        scope.launch {
            val containing = repo.albumsContainingMedia(mediaId)
            if (containing.isEmpty()) return@launch
            clearSelection()
            if (containing.size == 1) {
                onOpenAlbum(containing.first().albumId)
            } else {
                albumPicker = containing
            }
        }
    }

    albumPicker?.let { albums ->
        AlbumPickerDialog(
            title = "Open album",
            albums = albums,
            onSelect = {
                albumPicker = null
                onOpenAlbum(it.albumId)
            },
            onCancel = { albumPicker = null },
        )
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg)
            .padding(horizontal = 20.dp),
    ) {
        if (isSelecting) {
            SelectionBar(
                count = selection.size,
                onClose = { clearSelection() },
                onOpenAlbum = { selection.firstOrNull()?.let { openContainingAlbums(it) } },
            )
        } else {
            Text(
                text = "LIBRARY",
                style = LascoTheme.type.categoryLarge(),
                color = colors.ink,
                modifier = Modifier.padding(top = 24.dp, bottom = 16.dp),
            )

            if (sessionState.defaultUploadAlbumId == null) {
                Text(
                    text = "Auto-import paused. Set a default album to enable it.",
                    style = LascoTheme.type.body(14),
                    color = colors.inkMuted,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(bottom = 16.dp),
                )
            }
        }

        if (media.isEmpty()) {
            Text(text = "No media yet.", style = LascoTheme.type.body(), color = colors.inkMuted)
        } else {
            BoxWithConstraints {
                val columns = if (maxWidth > 500.dp) 3 else 2
                LazyVerticalGrid(
                    columns = GridCells.Fixed(columns),
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    items(media, key = { it.mediaId }) { item ->
                        MediaGridCell(
                            item = item,
                            repo = repo,
                            isSelected = selection.contains(item.mediaId),
                            onTap = {
                                if (isSelecting) {
                                    selection = if (selection.contains(item.mediaId)) {
                                        selection - item.mediaId
                                    } else {
                                        selection + item.mediaId
                                    }
                                    if (selection.isEmpty()) isSelecting = false
                                } else {
                                    onOpenMedia(item.mediaId)
                                }
                            },
                            onLongPress = {
                                if (!isSelecting) {
                                    isSelecting = true
                                    selection = setOf(item.mediaId)
                                }
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun SelectionBar(count: Int, onClose: () -> Unit, onOpenAlbum: () -> Unit) {
    val colors = LascoTheme.colors
    var showActionMenu by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 16.dp, bottom = 8.dp)
            .background(colors.pink)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "✕",
            style = LascoTheme.type.body(18),
            color = Color.White,
            modifier = Modifier.clickable { onClose() },
        )
        if (count > 1) {
            Text(
                text = "$count selected",
                style = LascoTheme.type.categorySmall(),
                color = Color.White,
                modifier = Modifier.padding(start = 8.dp),
            )
        }
        Box(modifier = Modifier.weight(1f))
        if (count == 1) {
            Box {
                Text(
                    text = "...",
                    style = LascoTheme.type.body(18),
                    color = Color.White,
                    modifier = Modifier.clickable { showActionMenu = true },
                )
                DropdownMenu(expanded = showActionMenu, onDismissRequest = { showActionMenu = false }) {
                    DropdownMenuItem(
                        text = { Text("Open containing album") },
                        onClick = {
                            showActionMenu = false
                            onOpenAlbum()
                        },
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun MediaGridCell(
    item: FfiMediaItem,
    repo: LibraryRepository,
    isSelected: Boolean,
    onTap: () -> Unit,
    onLongPress: () -> Unit,
) {
    val colors = LascoTheme.colors
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .background(colors.surfaceAlt)
            .combinedClickable(onClick = onTap, onLongClick = onLongPress),
    ) {
        MediaThumbnail(mediaId = item.mediaId, repo = repo, modifier = Modifier.fillMaxWidth())
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
