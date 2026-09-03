package com.lasco.lasco.ui.album

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.result.PickVisualMediaRequest
import android.net.Uri
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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
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
import com.lasco.lasco.ui.components.AlbumCell
import com.lasco.lasco.ui.components.LascoConfirmDialog
import com.lasco.lasco.ui.components.LascoTextInputDialog
import com.lasco.lasco.ui.components.MediaThumbnail
import com.lasco.lasco.ui.components.ThumbnailPickerDialog
import com.lasco.lasco.ui.media.DetailTarget
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiAlbumUuid
import uniffi.lasco_ffi.FfiMediaUuid
import uniffi.lasco_ffi.FfiGroupUuid

/**
 * Hoisted selection state for AlbumListScreen's picker mode, used by
 * AlbumMediaPickerScreen so a single selection survives navigating in and
 * out of nested albums across separate AlbumListScreen instances.
 */
data class AlbumPickerState(
    val disabledIds: Set<FfiMediaUuid>,
    val selectedIds: Set<FfiMediaUuid>,
    val onToggle: (mediaId: FfiMediaUuid) -> Unit,
)

/**
 * One album level's content: child albums, media/group items, and (root
 * only) the disconnected-albums section. The Android equivalent of Swift's
 * AlbumsView body for a given `album: FfiAlbum?`. Navigation between levels
 * and the breadcrumb title are owned by AlbumsScreen, the caller.
 */
@Composable
fun AlbumListScreen(
    albumId: FfiAlbumUuid?,
    albumName: String? = null,
    modifier: Modifier = Modifier,
    title: String = "ALBUMS",
    backLabel: String? = null,
    onBack: (() -> Unit)? = null,
    onOpenChild: (FfiAlbum) -> Unit = {},
    onOpenMedia: (position: Int, ascending: Boolean, target: DetailTarget) -> Unit = { _, _, _ -> },
    pickerState: AlbumPickerState? = null,
    onPickerVisibleChange: (Boolean) -> Unit = {},
    viewModel: AlbumViewModel = viewModel(
        key = albumId?.value ?: "root",
        factory = AlbumViewModel.factory(albumId),
    ),
) {
    val colors = LascoTheme.colors
    val repo = LibraryRepository.from(LocalContext.current)
    val scope = rememberCoroutineScope()

    val entries = viewModel.entries.collectAsLazyPagingItems()
    val sortAscending by viewModel.sortAscending.collectAsStateWithLifecycle()

    var isGridLayout by remember { mutableStateOf(true) }

    var selectedMediaIds by remember { mutableStateOf(setOf<FfiMediaUuid>()) }
    var selectedGroupIds by remember { mutableStateOf(setOf<FfiGroupUuid>()) }
    var selectedAlbumIds by remember { mutableStateOf(setOf<FfiAlbumUuid>()) }
    var selectedAlbumNames by remember { mutableStateOf<Map<FfiAlbumUuid, String>>(emptyMap()) }
    val isSelecting = selectedMediaIds.isNotEmpty() || selectedGroupIds.isNotEmpty() || selectedAlbumIds.isNotEmpty()

    fun clearSelection() {
        selectedMediaIds = emptySet()
        selectedGroupIds = emptySet()
        selectedAlbumIds = emptySet()
        selectedAlbumNames = emptyMap()
    }
    fun toggleMedia(id: FfiMediaUuid) {
        if (selectedAlbumIds.isNotEmpty()) return
        selectedMediaIds = if (id in selectedMediaIds) selectedMediaIds - id else selectedMediaIds + id
    }
    fun toggleGroup(id: FfiGroupUuid) {
        if (selectedAlbumIds.isNotEmpty()) return
        selectedGroupIds = if (id in selectedGroupIds) selectedGroupIds - id else selectedGroupIds + id
    }
    fun toggleAlbum(id: FfiAlbumUuid) {
        if (selectedMediaIds.isNotEmpty() || selectedGroupIds.isNotEmpty()) return
        selectedAlbumIds = if (id in selectedAlbumIds) selectedAlbumIds - id else selectedAlbumIds + id
    }

    var showNewAlbumDialog by remember { mutableStateOf(false) }
    var showRenameDialog by remember { mutableStateOf(false) }
    var showMovePicker by remember { mutableStateOf(false) }
    var showAddToAlbumPicker by remember { mutableStateOf(false) }
    var showDeleteConfirm by remember { mutableStateOf(false) }
    var showMediaPicker by remember { mutableStateOf(false) }
    var pickerDisabledIds by remember { mutableStateOf(setOf<FfiMediaUuid>()) }
    DisposableEffect(showMediaPicker) {
        onPickerVisibleChange(showMediaPicker)
        onDispose { onPickerVisibleChange(false) }
    }
    var showThumbnailPicker by remember { mutableStateOf(false) }
    var thumbnailPickerMedia by remember { mutableStateOf<List<FfiMediaItem>>(emptyList()) }
    var isImporting by remember { mutableStateOf(false) }

    val context = LocalContext.current
    LaunchedEffect(showMediaPicker, albumId) {
        if (showMediaPicker && albumId != null) {
            pickerDisabledIds = repo.mediaInAlbum(albumId).map { it.mediaId }.toSet()
        }
    }
    fun importUris(uris: List<Uri>) {
        if (uris.isNotEmpty() && albumId != null) {
            isImporting = true
            scope.launch {
                MediaImporter.importUris(context, repo, uris, albumId)
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

    if (showNewAlbumDialog) {
        LascoTextInputDialog(
            title = "New album",
            fieldLabel = "Album name",
            onConfirm = { name ->
                showNewAlbumDialog = false
                scope.launch {
                    val id = repo.createAlbum(name, albumId)
                    onOpenChild(FfiAlbum(id, name, albumId, 0u, false, false, null))
                }
            },
            onCancel = { showNewAlbumDialog = false },
        )
    }

    if (showRenameDialog && selectedAlbumIds.size == 1) {
        val targetId = selectedAlbumIds.first()
        val targetName = selectedAlbumNames[targetId]
        if (targetName != null) {
            LascoTextInputDialog(
                title = "Rename album",
                fieldLabel = "Album name",
                initialValue = targetName,
                onConfirm = { name ->
                    showRenameDialog = false
                    scope.launch { repo.renameAlbum(targetId, name) }
                    clearSelection()
                },
                onCancel = { showRenameDialog = false },
            )
        }
    }

    // The move destination is a navigable paged picker; it never needs a full album tree.
    if (showMovePicker) MoveDestinationPicker(
        excludedIds = selectedAlbumIds,
        onSelect = { target ->
            showMovePicker = false
            scope.launch {
                if (albumId != null) for (id in selectedMediaIds) repo.moveMediaToAlbum(id, albumId, target.albumId)
                for (id in selectedAlbumIds) repo.reparentAlbum(id, target.albumId)
                clearSelection()
            }
        },
        onCancel = { showMovePicker = false },
        modifier = Modifier.fillMaxSize(),
    )

    if (showAddToAlbumPicker) MoveDestinationPicker(
        excludedIds = emptySet(),
        onSelect = { target ->
            showAddToAlbumPicker = false
            val mediaIds = selectedMediaIds
            scope.launch {
                for (id in mediaIds) repo.addMediaToAlbum(target.albumId, id)
                clearSelection()
            }
        },
        onCancel = { showAddToAlbumPicker = false },
        modifier = Modifier.fillMaxSize(),
    )

    if (showDeleteConfirm) {
        LascoConfirmDialog(
            title = "Delete",
            message = "This can't be undone.",
            onConfirm = {
                showDeleteConfirm = false
                scope.launch {
                    for (id in selectedAlbumIds) repo.deleteAlbum(id)
                    if (albumId != null) {
                        for (id in selectedGroupIds) repo.deleteGroup(id, albumId)
                    }
                    clearSelection()
                }
            },
            onCancel = { showDeleteConfirm = false },
        )
    }

    if (showThumbnailPicker && albumId != null) {
        LaunchedEffect(showThumbnailPicker) {
            thumbnailPickerMedia = repo.mediaInAlbum(albumId)
        }
        ThumbnailPickerDialog(
            media = thumbnailPickerMedia,
            repo = repo,
            onPick = { mediaId ->
                showThumbnailPicker = false
                scope.launch { repo.setAlbumThumbnail(albumId, mediaId) }
            },
            onCancel = { showThumbnailPicker = false },
        )
    }

    Box(modifier = modifier.fillMaxSize()) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(colors.bg)
            .padding(horizontal = 20.dp),
    ) {
        if (isSelecting) {
            AlbumSelectionBar(
                count = selectedMediaIds.size + selectedGroupIds.size + selectedAlbumIds.size,
                canRename = selectedAlbumIds.size == 1,
                canGroup = albumId != null && selectedMediaIds.size >= 2 && selectedGroupIds.isEmpty() && selectedAlbumIds.isEmpty(),
                canAddToAlbum = selectedMediaIds.isNotEmpty(),
                canMove = selectedGroupIds.isEmpty() && (selectedMediaIds.isNotEmpty() || selectedAlbumIds.isNotEmpty()),
                canRemove = albumId != null && selectedMediaIds.isNotEmpty() && selectedGroupIds.isEmpty() && selectedAlbumIds.isEmpty(),
                canDelete = selectedAlbumIds.isNotEmpty() || selectedGroupIds.isNotEmpty(),
                onClose = { clearSelection() },
                onRename = { showRenameDialog = true },
                onGroup = {
                    if (albumId != null) {
                        val mediaIds = selectedMediaIds.toList()
                        scope.launch { repo.createGroupFromSelectedMedia(mediaIds, albumId) }
                        clearSelection()
                    }
                },
                onAddToAlbum = { showAddToAlbumPicker = true },
                onMove = { showMovePicker = true },
                onRemove = {
                    if (albumId != null) {
                        val mediaIds = selectedMediaIds.toList()
                        scope.launch { for (id in mediaIds) repo.removeMediaFromAlbum(albumId, id) }
                        clearSelection()
                    }
                },
                onDelete = { showDeleteConfirm = true },
            )
        } else {
            AlbumHeader(
                title = title,
                backLabel = backLabel,
                onBack = onBack,
                isAlbumView = albumId != null,
                isGridLayout = isGridLayout,
                onToggleLayout = { isGridLayout = !isGridLayout },
                sortAscending = sortAscending,
                onToggleSort = { viewModel.setSortAscending(!sortAscending) },
                onNewAlbum = { showNewAlbumDialog = true },
                onImportPhotos = if (albumId != null) {
                    { photoPickerLauncher.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }
                } else {
                    null
                },
                onImportFiles = if (albumId != null) {
                    { filePickerLauncher.launch(arrayOf("image/*", "video/*")) }
                } else {
                    null
                },
                onAddFromLibrary = if (albumId != null) {
                    { showMediaPicker = true }
                } else {
                    null
                },
                onSetThumbnail = if (albumId != null) {
                    { showThumbnailPicker = true }
                } else {
                    null
                },
                pickerMode = pickerState != null,
            )
        }

        if (entries.loadState.refresh is LoadState.Loading && entries.itemCount == 0) {
            Box(modifier = Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
                CircularProgressIndicator(color = colors.ink)
            }
        } else if (entries.loadState.refresh is LoadState.Error && entries.itemCount == 0) {
            Box(modifier = Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
                Text("Could not load albums. Tap to retry.", color = colors.inkMuted, modifier = Modifier.clickable { entries.retry() })
            }
        } else if (entries.itemCount == 0) {
            Box(modifier = Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
                Text(
                    text = if (albumId == null) "No albums yet" else "Empty album.",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                )
            }
        } else {
            BoxWithConstraints(modifier = Modifier.fillMaxWidth().weight(1f)) {
                val columns = if (maxWidth > 500.dp) 3 else 2
                AlbumEntriesGrid(
                    modifier = Modifier.fillMaxSize(),
                    repo = repo,
                    columns = columns,
                    entries = entries,
                    isGridLayout = isGridLayout,
                    selectedAlbumIds = selectedAlbumIds,
                    selectedMediaIds = pickerState?.selectedIds ?: selectedMediaIds,
                    selectedGroupIds = selectedGroupIds,
                    isSelecting = isSelecting,
                    pickerState = pickerState,
                    onAlbumTap = { child ->
                        when {
                            pickerState != null -> onOpenChild(child)
                            isSelecting -> {
                                selectedAlbumNames += child.albumId to child.name
                                toggleAlbum(child.albumId)
                            }
                            else -> onOpenChild(child)
                        }
                    },
                    onAlbumLongPress = { child -> if (pickerState == null) { selectedAlbumNames += child.albumId to child.name; toggleAlbum(child.albumId) } },
                    onItemTap = { indexed ->
                        val item = indexed.item
                        val mediaId = item.media?.mediaId
                        val groupId = item.group?.groupId
                        when {
                            pickerState != null && mediaId != null && mediaId !in pickerState.disabledIds ->
                                pickerState.onToggle(mediaId)
                            pickerState != null -> Unit
                            isSelecting && mediaId != null -> toggleMedia(mediaId)
                            isSelecting && groupId != null -> toggleGroup(groupId)
                            else -> onOpenMedia(indexed.position, sortAscending, indexed.item.toDetailTarget())
                        }
                    },
                    onItemLongPress = { item ->
                        if (pickerState == null) {
                            item.media?.let { toggleMedia(it.mediaId) }
                            item.group?.let { toggleGroup(it.groupId) }
                        }
                    },
                )
            }
        }
    }

    if (isImporting) {
        Box(
            modifier = Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f)),
            contentAlignment = Alignment.Center,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                CircularProgressIndicator(color = colors.ink)
                Text(text = "Importing…", color = colors.ink, modifier = Modifier.padding(top = 12.dp))
            }
        }
    }

    if (showMediaPicker && albumId != null) {
        AlbumMediaPickerScreen(
            destAlbumName = albumName ?: "Album",
            disabledIds = pickerDisabledIds,
            onConfirm = { ids ->
                showMediaPicker = false
                scope.launch { for (id in ids) repo.addMediaToAlbum(albumId, id) }
            },
            onCancel = { showMediaPicker = false },
            modifier = Modifier.fillMaxSize(),
        )
    }
    }
}

@Composable
private fun AlbumHeader(
    title: String,
    backLabel: String?,
    onBack: (() -> Unit)?,
    isAlbumView: Boolean,
    isGridLayout: Boolean,
    onToggleLayout: () -> Unit,
    sortAscending: Boolean,
    onToggleSort: () -> Unit,
    onNewAlbum: () -> Unit,
    onImportPhotos: (() -> Unit)? = null,
    onImportFiles: (() -> Unit)? = null,
    onAddFromLibrary: (() -> Unit)? = null,
    onSetThumbnail: (() -> Unit)? = null,
    pickerMode: Boolean = false,
) {
    val colors = LascoTheme.colors
    var showAddMenu by remember { mutableStateOf(false) }
    var showImportAndAddMenu by remember { mutableStateOf(false) }
    var showMoreMenu by remember { mutableStateOf(false) }
    Column {
        if (onBack != null) {
            Text(
                text = "← ${backLabel ?: "Albums"}",
                style = LascoTheme.type.body(),
                color = colors.inkMuted,
                modifier = Modifier.padding(top = 20.dp).clickable { onBack() },
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(top = if (onBack != null) 8.dp else 24.dp, bottom = 16.dp),
        ) {
            Text(
                text = title,
                style = LascoTheme.type.categoryLarge(),
                color = colors.ink,
                maxLines = 1,
                modifier = Modifier.weight(1f),
            )
            if (!pickerMode) {
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                if (isAlbumView) {
                    Text(
                        text = if (isGridLayout) "☰" else "▦",
                        style = LascoTheme.type.body(18),
                        color = colors.ink,
                        modifier = Modifier.clickable { onToggleLayout() },
                    )
                }
                Text(
                    text = if (sortAscending) "↑" else "↓",
                    style = LascoTheme.type.body(18),
                    color = colors.ink,
                    modifier = Modifier.clickable { onToggleSort() },
                )
                if (onImportPhotos != null) {
                    Box {
                        Text(
                            text = "＋",
                            style = LascoTheme.type.body(20),
                            color = colors.ink,
                            modifier = Modifier.clickable { showAddMenu = true },
                        )
                        DropdownMenu(expanded = showAddMenu, onDismissRequest = { showAddMenu = false }) {
                            DropdownMenuItem(
                                text = { Text("Import and add…") },
                                onClick = {
                                    showAddMenu = false
                                    showImportAndAddMenu = true
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("Add file from library") },
                                onClick = {
                                    showAddMenu = false
                                    onAddFromLibrary?.invoke()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("Create album") },
                                onClick = {
                                    showAddMenu = false
                                    onNewAlbum()
                                },
                            )
                        }
                        DropdownMenu(expanded = showImportAndAddMenu, onDismissRequest = { showImportAndAddMenu = false }) {
                            DropdownMenuItem(
                                text = { Text("Import from Photos") },
                                onClick = {
                                    showImportAndAddMenu = false
                                    onImportPhotos()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("Import from Files") },
                                onClick = {
                                    showImportAndAddMenu = false
                                    onImportFiles?.invoke()
                                },
                            )
                        }
                    }
                } else {
                    Text(
                        text = "＋",
                        style = LascoTheme.type.body(20),
                        color = colors.ink,
                        modifier = Modifier.clickable { onNewAlbum() },
                    )
                }
                if (onSetThumbnail != null) {
                    Box {
                        Text(
                            text = "...",
                            style = LascoTheme.type.body(18),
                            color = colors.ink,
                            modifier = Modifier.clickable { showMoreMenu = true },
                        )
                        DropdownMenu(expanded = showMoreMenu, onDismissRequest = { showMoreMenu = false }) {
                            DropdownMenuItem(
                                text = { Text("Set thumbnail...") },
                                onClick = {
                                    showMoreMenu = false
                                    onSetThumbnail()
                                },
                            )
                        }
                    }
                }
            }
            }
        }
    }
}

@Composable
private fun AlbumSelectionBar(
    count: Int,
    canRename: Boolean,
    canGroup: Boolean,
    canAddToAlbum: Boolean,
    canMove: Boolean,
    canRemove: Boolean,
    canDelete: Boolean,
    onClose: () -> Unit,
    onRename: () -> Unit,
    onGroup: () -> Unit,
    onAddToAlbum: () -> Unit,
    onMove: () -> Unit,
    onRemove: () -> Unit,
    onDelete: () -> Unit,
) {
    val colors = LascoTheme.colors
    var showActionMenu by remember { mutableStateOf(false) }
    val hasActions = canRename || canGroup || canAddToAlbum || canMove || canRemove || canDelete
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 16.dp, bottom = 8.dp)
            .background(colors.pink)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
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
        if (hasActions) {
            Box {
                Text(
                    text = "...",
                    style = LascoTheme.type.body(18),
                    color = Color.White,
                    modifier = Modifier.clickable { showActionMenu = true },
                )
                DropdownMenu(expanded = showActionMenu, onDismissRequest = { showActionMenu = false }) {
                    if (canRename) {
                        DropdownMenuItem(
                            text = { Text("Rename album") },
                            onClick = {
                                showActionMenu = false
                                onRename()
                            },
                        )
                    }
                    if (canGroup) {
                        DropdownMenuItem(
                            text = { Text("Group together") },
                            onClick = {
                                showActionMenu = false
                                onGroup()
                            },
                        )
                    }
                    if (canAddToAlbum) {
                        DropdownMenuItem(
                            text = { Text("Add to album...") },
                            onClick = {
                                showActionMenu = false
                                onAddToAlbum()
                            },
                        )
                    }
                    if (canMove) {
                        DropdownMenuItem(
                            text = { Text("Move to album...") },
                            onClick = {
                                showActionMenu = false
                                onMove()
                            },
                        )
                    }
                    if (canRemove) {
                        DropdownMenuItem(
                            text = { Text("Remove from album") },
                            onClick = {
                                showActionMenu = false
                                onRemove()
                            },
                        )
                    }
                    if (canDelete) {
                        DropdownMenuItem(
                            text = { Text("Delete") },
                            onClick = {
                                showActionMenu = false
                                onDelete()
                            },
                        )
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun AlbumEntriesGrid(
    modifier: Modifier = Modifier,
    repo: LibraryRepository,
    columns: Int,
    entries: androidx.paging.compose.LazyPagingItems<AlbumEntry>,
    isGridLayout: Boolean,
    selectedAlbumIds: Set<FfiAlbumUuid>,
    selectedMediaIds: Set<FfiMediaUuid>,
    selectedGroupIds: Set<FfiGroupUuid>,
    isSelecting: Boolean,
    pickerState: AlbumPickerState? = null,
    onAlbumTap: (FfiAlbum) -> Unit,
    onAlbumLongPress: (FfiAlbum) -> Unit,
    onItemTap: (AlbumEntry.Item) -> Unit,
    onItemLongPress: (uniffi.lasco_ffi.FfiAlbumItem) -> Unit,
) {
    val colors = LascoTheme.colors
    LazyVerticalGrid(
        columns = GridCells.Fixed(columns),
        state = rememberLazyGridState(),
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(if (isGridLayout) 3.dp else 12.dp),
        verticalArrangement = Arrangement.spacedBy(if (isGridLayout) 3.dp else 12.dp),
    ) {
        items(
            count = entries.itemCount,
            key = entries.itemKey { it.key.saveableValue() },
            span = { index -> if (entries.peek(index) is AlbumEntry.DisconnectedHeader || !isGridLayout && entries.peek(index) is AlbumEntry.Item) GridItemSpan(maxLineSpan) else GridItemSpan(1) },
        ) { index ->
            when (val entry = entries[index]) {
                is AlbumEntry.ChildAlbum -> AlbumCell(
                    album = entry.album,
                    repo = repo,
                    isSelected = entry.album.albumId in selectedAlbumIds,
                    onClick = { onAlbumTap(entry.album) },
                    onLongClick = { onAlbumLongPress(entry.album) },
                )
                is AlbumEntry.Item -> {
                    val item = entry.item
                    val dimmed = pickerState != null && (item.group != null || item.media?.mediaId in pickerState.disabledIds)
                    if (isGridLayout) AlbumItemCell(
                        item, repo, isSelected = item.media?.mediaId in selectedMediaIds || item.group?.groupId in selectedGroupIds,
                        dimmed = dimmed, onTap = { onItemTap(entry) }, onLongPress = { onItemLongPress(item) },
                    ) else AlbumItemRow(
                        item, repo, isSelected = item.media?.mediaId in selectedMediaIds || item.group?.groupId in selectedGroupIds,
                        dimmed = dimmed, onTap = { onItemTap(entry) }, onLongPress = { onItemLongPress(item) },
                    )
                }
                AlbumEntry.DisconnectedHeader -> Text(
                    "DISCONNECTED", style = LascoTheme.type.categoryLarge(22), color = colors.ink,
                    modifier = Modifier.padding(top = 12.dp),
                )
                null -> Box(modifier = Modifier.fillMaxWidth().background(colors.surfaceAlt))
            }
        }
        when (entries.loadState.append) {
            is LoadState.Loading -> item(span = { GridItemSpan(maxLineSpan) }) { CircularProgressIndicator(color = colors.ink) }
            is LoadState.Error -> item(span = { GridItemSpan(maxLineSpan) }) {
                Text("Could not load more. Tap to retry.", color = colors.inkMuted, modifier = Modifier.clickable { entries.retry() })
            }
            else -> Unit
        }
    }
}

/** A paged, navigable destination picker used for moving albums and media. */
@Composable
private fun MoveDestinationPicker(
    excludedIds: Set<FfiAlbumUuid>,
    onSelect: (FfiAlbum) -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var current by remember { mutableStateOf<FfiAlbum?>(null) }
    val viewModel: AlbumViewModel = viewModel(
        key = "move:${current?.albumId ?: "root"}",
        factory = AlbumViewModel.factory(current?.albumId),
    )
    val entries = viewModel.entries.collectAsLazyPagingItems()
    val colors = LascoTheme.colors
    Column(modifier = modifier.background(colors.bg).padding(20.dp)) {
        Text("Move to", style = LascoTheme.type.categoryLarge(), color = colors.ink)
        if (current != null) {
            Row(modifier = Modifier.fillMaxWidth().padding(vertical = 12.dp)) {
                Text("← ${current!!.name}", color = colors.inkMuted, modifier = Modifier.clickable { current = null })
                Text("Move here", color = colors.ink, modifier = Modifier.padding(start = 20.dp).clickable { onSelect(current!!) })
            }
        } else Text("Open a destination album, then choose Move here.", color = colors.inkMuted, modifier = Modifier.padding(vertical = 12.dp))
        LazyVerticalGrid(
            columns = GridCells.Fixed(2),
            modifier = Modifier.weight(1f),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            items(count = entries.itemCount, key = entries.itemKey { it.key.saveableValue() }) { index ->
                (entries[index] as? AlbumEntry.ChildAlbum)?.album?.let { child ->
                    if (child.albumId !in excludedIds) AlbumCell(child, LibraryRepository.from(LocalContext.current), onClick = { current = child })
                }
            }
        }
        Text("Cancel", color = colors.inkMuted, modifier = Modifier.padding(top = 12.dp).clickable { onCancel() })
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun AlbumItemCell(
    item: FfiAlbumItem,
    repo: LibraryRepository,
    modifier: Modifier = Modifier,
    isSelected: Boolean,
    dimmed: Boolean = false,
    onTap: () -> Unit,
    onLongPress: () -> Unit,
) {
    val colors = LascoTheme.colors
    Box(
        modifier = modifier
            .background(colors.surfaceAlt)
            .then(if (dimmed) Modifier.alpha(0.4f) else Modifier)
            .combinedClickable(enabled = !dimmed, onClick = onTap, onLongClick = onLongPress),
    ) {
        MediaThumbnail(
            mediaId = item.media?.mediaId ?: item.group?.mediaIds?.firstOrNull(),
            repo = repo,
            modifier = Modifier.fillMaxWidth(),
        )
        item.group?.let { g ->
            Text(
                text = "GROUP (${g.mediaIds.size})",
                style = LascoTheme.type.mono(10),
                color = colors.inkMuted,
                modifier = Modifier.padding(4.dp),
            )
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

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun AlbumItemRow(
    item: FfiAlbumItem,
    repo: LibraryRepository,
    isSelected: Boolean,
    dimmed: Boolean = false,
    onTap: () -> Unit,
    onLongPress: () -> Unit,
) {
    val colors = LascoTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (dimmed) Modifier.alpha(0.4f) else Modifier)
            .combinedClickable(enabled = !dimmed, onClick = onTap, onLongClick = onLongPress)
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        MediaThumbnail(
            mediaId = item.media?.mediaId ?: item.group?.mediaIds?.firstOrNull(),
            repo = repo,
            modifier = Modifier.size(40.dp),
        )
        Text(
            text = item.media?.name ?: item.media?.filenameOriginal ?: item.group?.let { "Group (${it.mediaIds.size})" } ?: "",
            style = LascoTheme.type.body(14),
            color = colors.ink,
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        if (isSelected) {
            Text(text = "✓", style = LascoTheme.type.body(16), color = colors.pink)
        }
    }
}
