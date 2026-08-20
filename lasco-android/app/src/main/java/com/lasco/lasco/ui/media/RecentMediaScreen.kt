package com.lasco.lasco.ui.media

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.paging.LoadState
import androidx.paging.compose.collectAsLazyPagingItems
import androidx.paging.compose.itemKey
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.media.MediaImporter
import com.lasco.lasco.ui.components.AlbumPickerDialog
import com.lasco.lasco.ui.components.MediaThumbnail
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiMediaUuid

/**
 * Recent media grid, the Android equivalent of the Swift ContentView home
 * screen. Thumbnails load lazily per cell, see MediaThumbnail.
 */
@Composable
fun RecentMediaScreen(
    modifier: Modifier = Modifier,
    onOpenMedia: (position: Int) -> Unit = {},
    onOpenAlbum: (albumId: String) -> Unit = {},
    viewModel: RecentMediaViewModel = viewModel(factory = RecentMediaViewModel.Factory),
) {
    val colors = LascoTheme.colors
    val media = viewModel.media.collectAsLazyPagingItems()
    val showingOrphans by viewModel.showingOrphans.collectAsStateWithLifecycle()
    val repo = LibraryRepository.from(LocalContext.current)
    val scope = rememberCoroutineScope()
    val context = LocalContext.current

    var isSelecting by remember { mutableStateOf(false) }
    var selection by remember { mutableStateOf(setOf<FfiMediaUuid>()) }
    var albumPicker by remember { mutableStateOf<List<uniffi.lasco_ffi.FfiAlbum>?>(null) }
    var isImporting by remember { mutableStateOf(false) }
    var showImportMenu by remember { mutableStateOf(false) }

    fun clearSelection() {
        isSelecting = false
        selection = emptySet()
    }

    fun showAddToAlbumPicker() {
        scope.launch {
            albumPicker = repo.allAlbums()
        }
    }

    fun importUris(uris: List<Uri>) {
        if (uris.isEmpty()) return
        isImporting = true
        scope.launch {
            try {
                MediaImporter.importUris(context, repo, uris, albumId = null)
            } finally {
                isImporting = false
            }
        }
    }

    val photoPickerLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.PickMultipleVisualMedia(),
    ) { uris -> importUris(uris) }
    val filePickerLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris -> importUris(uris) }

    albumPicker?.let { albums ->
        AlbumPickerDialog(
            title = "Add to album",
            albums = albums,
            onSelect = { album ->
                albumPicker = null
                val mediaIds = selection
                scope.launch {
                    mediaIds.forEach { mediaId -> repo.addMediaToAlbum(album.albumId, mediaId) }
                    clearSelection()
                }
            },
            onCancel = { albumPicker = null },
        )
    }

    Box(modifier = modifier.fillMaxSize().background(colors.bg)) {
    Column(modifier = Modifier.fillMaxSize().padding(horizontal = 20.dp)) {
        if (isSelecting) {
            SelectionBar(
                count = selection.size,
                onClose = { clearSelection() },
                onAddToAlbum = ::showAddToAlbumPicker,
            )
        } else {
            Row(
                modifier = Modifier.fillMaxWidth().padding(top = 24.dp, bottom = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = "LIBRARY",
                    style = LascoTheme.type.categoryLarge(),
                    color = colors.ink,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    text = if (showingOrphans) "Orphan" else "All",
                    style = LascoTheme.type.body(14),
                    color = colors.inkSub,
                )
                Switch(
                    checked = showingOrphans,
                    onCheckedChange = {
                        clearSelection()
                        viewModel.setShowingOrphans(it)
                    },
                    modifier = Modifier.padding(start = 4.dp),
                )
                Box {
                    Text(
                        text = "＋",
                        style = LascoTheme.type.body(20),
                        color = colors.ink,
                        modifier = Modifier.padding(start = 12.dp).clickable { showImportMenu = true },
                    )
                    DropdownMenu(
                        expanded = showImportMenu,
                        onDismissRequest = { showImportMenu = false },
                    ) {
                        DropdownMenuItem(
                            text = { Text("Import from device Photos") },
                            onClick = {
                                showImportMenu = false
                                photoPickerLauncher.launch(
                                    PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo),
                                )
                            },
                        )
                        DropdownMenuItem(
                            text = { Text("Import from Files") },
                            onClick = {
                                showImportMenu = false
                                filePickerLauncher.launch(arrayOf("image/*", "video/*"))
                            },
                        )
                    }
                }
            }

        }

        if (media.loadState.refresh is LoadState.Loading && media.itemCount == 0) {
            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                androidx.compose.material3.CircularProgressIndicator(color = colors.ink)
            }
        } else if (media.loadState.refresh is LoadState.Error && media.itemCount == 0) {
            val error = media.loadState.refresh as LoadState.Error
            Text(
                text = "Could not load media: ${error.error.message ?: "Unknown error"}. Tap to retry.",
                style = LascoTheme.type.body(),
                color = colors.inkMuted,
                modifier = Modifier.clickable { media.retry() },
            )
        } else if (media.itemCount == 0) {
            Text(
                text = if (showingOrphans) "No orphan media." else "No media yet.",
                style = LascoTheme.type.body(),
                color = colors.inkMuted,
            )
        } else {
            BoxWithConstraints {
                val columns = if (maxWidth > 500.dp) 3 else 2
                LazyVerticalGrid(
                    columns = GridCells.Fixed(columns),
                    state = rememberLazyGridState(),
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    // Lazy item keys are saved in Android's Bundle, so UniFFI
                    // wrapper types must be reduced to their String values.
                    items(count = media.itemCount, key = media.itemKey { it.mediaId.value }) { index ->
                        media[index]?.let { item ->
                            MediaGridCell(
                                item = item,
                                repo = repo,
                                isSelected = selection.contains(item.mediaId),
                                onTap = {
                                    if (isSelecting) {
                                        selection = if (selection.contains(item.mediaId)) selection - item.mediaId else selection + item.mediaId
                                        if (selection.isEmpty()) isSelecting = false
                                    } else onOpenMedia(index)
                                },
                                onLongPress = {
                                    if (!isSelecting) {
                                        isSelecting = true
                                        selection = setOf(item.mediaId)
                                    }
                                },
                            )
                        } ?: Box(modifier = Modifier.fillMaxWidth().background(colors.surfaceAlt))
                    }
                    when (val append = media.loadState.append) {
                        is LoadState.Loading -> item { androidx.compose.material3.CircularProgressIndicator(color = colors.ink) }
                        is LoadState.Error -> item {
                            Text(
                                text = "Could not load more. Tap to retry.",
                                color = colors.inkMuted,
                                modifier = Modifier.clickable { media.retry() },
                            )
                        }
                        else -> Unit
                    }
                }
            }
        }
    }
    if (isImporting) {
        Box(
            modifier = Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f)),
            contentAlignment = Alignment.Center,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                androidx.compose.material3.CircularProgressIndicator(color = colors.ink)
                Text(text = "Importing…", color = colors.ink, modifier = Modifier.padding(top = 12.dp))
            }
        }
    }
    }
}

@Composable
private fun SelectionBar(count: Int, onClose: () -> Unit, onAddToAlbum: () -> Unit) {
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
        Box {
            Text(
                text = "...",
                style = LascoTheme.type.body(18),
                color = Color.White,
                modifier = Modifier.clickable { showActionMenu = true },
            )
            DropdownMenu(expanded = showActionMenu, onDismissRequest = { showActionMenu = false }) {
                DropdownMenuItem(
                    text = { Text("Add to album") },
                    onClick = {
                        showActionMenu = false
                        onAddToAlbum()
                    },
                )
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
