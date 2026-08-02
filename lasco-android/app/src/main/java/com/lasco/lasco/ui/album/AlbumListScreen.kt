package com.lasco.lasco.ui.album

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.result.PickVisualMediaRequest
import android.net.Uri
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.media.MediaImporter
import com.lasco.lasco.ui.components.AlbumCell
import com.lasco.lasco.ui.components.AlbumPickerDialog
import com.lasco.lasco.ui.components.LascoConfirmDialog
import com.lasco.lasco.ui.components.LascoTextInputDialog
import com.lasco.lasco.ui.components.MediaThumbnail
import com.lasco.lasco.ui.components.ThumbnailPickerDialog
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem
import uniffi.lasco_ffi.FfiMediaItem

/**
 * Hoisted selection state for AlbumListScreen's picker mode, used by
 * AlbumMediaPickerScreen so a single selection survives navigating in and
 * out of nested albums across separate AlbumListScreen instances.
 */
data class AlbumPickerState(
    val disabledIds: Set<String>,
    val selectedIds: Set<String>,
    val onToggle: (mediaId: String) -> Unit,
)

/**
 * One album level's content: child albums, media/group items, and (root
 * only) the disconnected-albums section. The Android equivalent of Swift's
 * AlbumsView body for a given `album: FfiAlbum?`. Navigation between levels
 * and the breadcrumb title are owned by AlbumsScreen, the caller.
 */
@Composable
fun AlbumListScreen(
    album: FfiAlbum?,
    modifier: Modifier = Modifier,
    title: String = "ALBUMS",
    backLabel: String? = null,
    onBack: (() -> Unit)? = null,
    onOpenChild: (FfiAlbum) -> Unit = {},
    onOpenMedia: (itemId: String) -> Unit = {},
    pickerState: AlbumPickerState? = null,
    onPickerVisibleChange: (Boolean) -> Unit = {},
    viewModel: AlbumViewModel = viewModel(
        key = album?.albumId ?: "root",
        factory = AlbumViewModel.factory(album?.albumId),
    ),
) {
    val colors = LascoTheme.colors
    val repo = LibraryRepository.from(LocalContext.current)
    val scope = rememberCoroutineScope()

    val allAlbums by viewModel.allAlbums.collectAsStateWithLifecycle()
    val items by viewModel.items.collectAsStateWithLifecycle()
    val sortAscending by viewModel.sortAscending.collectAsStateWithLifecycle()

    val childAlbums = remember(allAlbums, album) {
        allAlbums.filter { it.parentAlbumId == album?.albumId && !it.deleted && !it.isDisconnected }
    }
    val disconnectedAlbums = remember(allAlbums, album) {
        if (album == null) allAlbums.filter { it.isDisconnected && !it.deleted } else emptyList()
    }

    var isGridLayout by remember { mutableStateOf(true) }

    var selectedMediaIds by remember { mutableStateOf(setOf<String>()) }
    var selectedGroupIds by remember { mutableStateOf(setOf<String>()) }
    var selectedAlbumIds by remember { mutableStateOf(setOf<String>()) }
    val isSelecting = selectedMediaIds.isNotEmpty() || selectedGroupIds.isNotEmpty() || selectedAlbumIds.isNotEmpty()

    fun clearSelection() {
        selectedMediaIds = emptySet()
        selectedGroupIds = emptySet()
        selectedAlbumIds = emptySet()
    }
    fun toggleMedia(id: String) {
        if (selectedAlbumIds.isNotEmpty()) return
        selectedMediaIds = if (id in selectedMediaIds) selectedMediaIds - id else selectedMediaIds + id
    }
    fun toggleGroup(id: String) {
        if (selectedAlbumIds.isNotEmpty()) return
        selectedGroupIds = if (id in selectedGroupIds) selectedGroupIds - id else selectedGroupIds + id
    }
    fun toggleAlbum(id: String) {
        if (selectedMediaIds.isNotEmpty() || selectedGroupIds.isNotEmpty()) return
        selectedAlbumIds = if (id in selectedAlbumIds) selectedAlbumIds - id else selectedAlbumIds + id
    }

    var showNewAlbumDialog by remember { mutableStateOf(false) }
    var showRenameDialog by remember { mutableStateOf(false) }
    var showMovePicker by remember { mutableStateOf(false) }
    var showDeleteConfirm by remember { mutableStateOf(false) }
    var showMediaPicker by remember { mutableStateOf(false) }
    DisposableEffect(showMediaPicker) {
        onPickerVisibleChange(showMediaPicker)
        onDispose { onPickerVisibleChange(false) }
    }
    var showThumbnailPicker by remember { mutableStateOf(false) }
    var thumbnailPickerMedia by remember { mutableStateOf<List<FfiMediaItem>>(emptyList()) }
    var isImporting by remember { mutableStateOf(false) }

    val context = LocalContext.current
    fun importUris(uris: List<Uri>) {
        val albumId = album?.albumId
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
                    val id = repo.createAlbum(name, album?.albumId)
                    repo.allAlbums().firstOrNull { it.albumId == id }?.let(onOpenChild)
                }
            },
            onCancel = { showNewAlbumDialog = false },
        )
    }

    if (showRenameDialog && selectedAlbumIds.size == 1) {
        val target = allAlbums.firstOrNull { it.albumId == selectedAlbumIds.first() }
        if (target != null) {
            LascoTextInputDialog(
                title = "Rename album",
                fieldLabel = "Album name",
                initialValue = target.name,
                onConfirm = { name ->
                    showRenameDialog = false
                    scope.launch { repo.renameAlbum(target.albumId, name) }
                    clearSelection()
                },
                onCancel = { showRenameDialog = false },
            )
        }
    }

    if (showMovePicker) {
        AlbumPickerDialog(
            title = "Move to",
            albums = allAlbums.filter { it.albumId !in selectedAlbumIds && !it.deleted },
            onSelect = { target ->
                showMovePicker = false
                scope.launch {
                    if (album != null) {
                        for (id in selectedMediaIds) repo.moveMediaToAlbum(id, album.albumId, target.albumId)
                    }
                    for (id in selectedAlbumIds) repo.reparentAlbum(id, target.albumId)
                    clearSelection()
                }
            },
            onCancel = { showMovePicker = false },
        )
    }

    if (showDeleteConfirm) {
        LascoConfirmDialog(
            title = "Delete",
            message = "This can't be undone.",
            onConfirm = {
                showDeleteConfirm = false
                scope.launch {
                    for (id in selectedAlbumIds) repo.deleteAlbum(id)
                    if (album != null) {
                        for (id in selectedGroupIds) repo.deleteGroup(id, album.albumId)
                    }
                    clearSelection()
                }
            },
            onCancel = { showDeleteConfirm = false },
        )
    }

    if (showThumbnailPicker && album != null) {
        LaunchedEffect(showThumbnailPicker) {
            thumbnailPickerMedia = repo.mediaInAlbum(album.albumId)
        }
        ThumbnailPickerDialog(
            media = thumbnailPickerMedia,
            repo = repo,
            onPick = { mediaId ->
                showThumbnailPicker = false
                scope.launch { repo.setAlbumThumbnail(album.albumId, mediaId) }
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
                canGroup = album != null && selectedMediaIds.size >= 2 && selectedGroupIds.isEmpty() && selectedAlbumIds.isEmpty(),
                canMove = selectedGroupIds.isEmpty() && (selectedMediaIds.isNotEmpty() || selectedAlbumIds.isNotEmpty()),
                canRemove = album != null && selectedMediaIds.isNotEmpty() && selectedGroupIds.isEmpty() && selectedAlbumIds.isEmpty(),
                canDelete = selectedAlbumIds.isNotEmpty() || selectedGroupIds.isNotEmpty(),
                onClose = { clearSelection() },
                onRename = { showRenameDialog = true },
                onGroup = {
                    if (album != null) {
                        val mediaIds = selectedMediaIds.toList()
                        scope.launch { repo.createGroupFromSelectedMedia(mediaIds, album.albumId) }
                        clearSelection()
                    }
                },
                onMove = { showMovePicker = true },
                onRemove = {
                    if (album != null) {
                        val mediaIds = selectedMediaIds.toList()
                        scope.launch { for (id in mediaIds) repo.removeMediaFromAlbum(album.albumId, id) }
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
                isAlbumView = album != null,
                isGridLayout = isGridLayout,
                onToggleLayout = { isGridLayout = !isGridLayout },
                sortAscending = sortAscending,
                onToggleSort = { viewModel.setSortAscending(!sortAscending) },
                onNewAlbum = { showNewAlbumDialog = true },
                onImportPhotos = if (album != null) {
                    { photoPickerLauncher.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }
                } else {
                    null
                },
                onImportFiles = if (album != null) {
                    { filePickerLauncher.launch(arrayOf("image/*", "video/*")) }
                } else {
                    null
                },
                onAddFromLibrary = if (album != null) {
                    { showMediaPicker = true }
                } else {
                    null
                },
                onSetThumbnail = if (album != null) {
                    { showThumbnailPicker = true }
                } else {
                    null
                },
                pickerMode = pickerState != null,
            )
        }

        val isEmpty = childAlbums.isEmpty() && items.isEmpty() && disconnectedAlbums.isEmpty()
        if (isEmpty) {
            Box(modifier = Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
                Text(
                    text = if (album == null) "No albums yet." else "Empty album.",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                )
            }
        } else {
            BoxWithConstraints(modifier = Modifier.fillMaxWidth().weight(1f)) {
                val columns = if (maxWidth > 500.dp) 3 else 2
                val albumCellSpacing = 12.dp
                val photoCellSpacing = 3.dp
                val albumCellWidth = (maxWidth - albumCellSpacing * (columns - 1)) / columns
                val photoCellWidth = (maxWidth - photoCellSpacing * (columns - 1)) / columns

                AlbumSectionsContent(
                    modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
                    repo = repo,
                    albumCellWidth = albumCellWidth,
                    photoCellWidth = photoCellWidth,
                    childAlbums = childAlbums,
                    disconnectedAlbums = disconnectedAlbums,
                    allAlbums = allAlbums,
                    items = items,
                    isGridLayout = isGridLayout,
                    selectedAlbumIds = selectedAlbumIds,
                    selectedMediaIds = pickerState?.selectedIds ?: selectedMediaIds,
                    selectedGroupIds = selectedGroupIds,
                    isSelecting = isSelecting,
                    pickerState = pickerState,
                    onAlbumTap = { child ->
                        when {
                            pickerState != null -> onOpenChild(child)
                            isSelecting -> toggleAlbum(child.albumId)
                            else -> onOpenChild(child)
                        }
                    },
                    onAlbumLongPress = { if (pickerState == null) toggleAlbum(it.albumId) },
                    onItemTap = { item, _ ->
                        val mediaId = item.media?.mediaId
                        val groupId = item.group?.groupId
                        when {
                            pickerState != null && mediaId != null && mediaId !in pickerState.disabledIds ->
                                pickerState.onToggle(mediaId)
                            pickerState != null -> Unit
                            isSelecting && mediaId != null -> toggleMedia(mediaId)
                            isSelecting && groupId != null -> toggleGroup(groupId)
                            else -> (mediaId ?: groupId)?.let(onOpenMedia)
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

    if (showMediaPicker && album != null) {
        AlbumMediaPickerScreen(
            destAlbumName = album.name,
            disabledIds = remember(items) { items.mapNotNull { it.media?.mediaId }.toSet() },
            onConfirm = { ids ->
                showMediaPicker = false
                scope.launch { for (id in ids) repo.addMediaToAlbum(album.albumId, id) }
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
                                text = { Text("Import photo") },
                                onClick = {
                                    showAddMenu = false
                                    onImportPhotos()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("Import file") },
                                onClick = {
                                    showAddMenu = false
                                    onImportFiles?.invoke()
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
    canMove: Boolean,
    canRemove: Boolean,
    canDelete: Boolean,
    onClose: () -> Unit,
    onRename: () -> Unit,
    onGroup: () -> Unit,
    onMove: () -> Unit,
    onRemove: () -> Unit,
    onDelete: () -> Unit,
) {
    val colors = LascoTheme.colors
    var showActionMenu by remember { mutableStateOf(false) }
    val hasActions = canRename || canGroup || canMove || canRemove || canDelete
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
private fun AlbumSectionsContent(
    modifier: Modifier = Modifier,
    repo: LibraryRepository,
    albumCellWidth: Dp,
    photoCellWidth: Dp,
    childAlbums: List<FfiAlbum>,
    disconnectedAlbums: List<FfiAlbum>,
    allAlbums: List<FfiAlbum>,
    items: List<FfiAlbumItem>,
    isGridLayout: Boolean,
    selectedAlbumIds: Set<String>,
    selectedMediaIds: Set<String>,
    selectedGroupIds: Set<String>,
    isSelecting: Boolean,
    pickerState: AlbumPickerState? = null,
    onAlbumTap: (FfiAlbum) -> Unit,
    onAlbumLongPress: (FfiAlbum) -> Unit,
    onItemTap: (FfiAlbumItem, Int) -> Unit,
    onItemLongPress: (FfiAlbumItem) -> Unit,
) {
    val colors = LascoTheme.colors

    fun parentInfoFor(disc: FfiAlbum): String {
        val parentId = disc.parentAlbumId ?: return "No parent"
        return allAlbums.firstOrNull { it.albumId == parentId }?.name ?: "Parent deleted"
    }

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(20.dp)) {
        if (childAlbums.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                childAlbums.forEach { child ->
                    AlbumCell(
                        album = child,
                        repo = repo,
                        modifier = Modifier.width(albumCellWidth),
                        isSelected = child.albumId in selectedAlbumIds,
                        onClick = { onAlbumTap(child) },
                        onLongClick = { onAlbumLongPress(child) },
                    )
                }
            }
        }

        if (items.isNotEmpty()) {
            if (isGridLayout) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(3.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    items.forEachIndexed { index, item ->
                        val dimmed = pickerState != null &&
                            (item.group != null || item.media?.mediaId in pickerState.disabledIds)
                        AlbumItemCell(
                            item = item,
                            repo = repo,
                            modifier = Modifier.width(photoCellWidth),
                            isSelected = (item.media?.mediaId != null && item.media?.mediaId in selectedMediaIds) ||
                                (item.group?.groupId != null && item.group?.groupId in selectedGroupIds),
                            dimmed = dimmed,
                            onTap = { onItemTap(item, index) },
                            onLongPress = { onItemLongPress(item) },
                        )
                    }
                }
            } else {
                Column {
                    items.forEachIndexed { index, item ->
                        val dimmed = pickerState != null &&
                            (item.group != null || item.media?.mediaId in pickerState.disabledIds)
                        AlbumItemRow(
                            item = item,
                            repo = repo,
                            isSelected = (item.media?.mediaId != null && item.media?.mediaId in selectedMediaIds) ||
                                (item.group?.groupId != null && item.group?.groupId in selectedGroupIds),
                            dimmed = dimmed,
                            onTap = { onItemTap(item, index) },
                            onLongPress = { onItemLongPress(item) },
                        )
                    }
                }
            }
        }

        if (disconnectedAlbums.isNotEmpty()) {
            Text(text = "DISCONNECTED", style = LascoTheme.type.categoryLarge(22), color = colors.ink)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                disconnectedAlbums.forEach { disc ->
                    AlbumCell(
                        album = disc,
                        repo = repo,
                        modifier = Modifier.width(albumCellWidth),
                        parentInfo = parentInfoFor(disc),
                        isSelected = disc.albumId in selectedAlbumIds,
                        onClick = { onAlbumTap(disc) },
                        onLongClick = { onAlbumLongPress(disc) },
                    )
                }
            }
        }
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
