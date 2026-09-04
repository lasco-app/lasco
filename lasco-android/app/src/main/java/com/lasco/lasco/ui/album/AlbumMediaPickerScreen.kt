package com.lasco.lasco.ui.album

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.ui.NavDisplay
import androidx.paging.LoadState
import androidx.paging.compose.collectAsLazyPagingItems
import androidx.paging.compose.itemKey
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.components.LascoToggle
import com.lasco.lasco.ui.components.MediaThumbnail
import com.lasco.lasco.ui.media.RecentMediaViewModel
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.serialization.Serializable
import uniffi.lasco_ffi.FfiAlbumUuid
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiMediaUuid

@Serializable
private data class PickerAlbumKey(val albumId: String?, val albumName: String? = null) : NavKey

private enum class AddMediaPickerTab { AllMedia, Albums }

/** A picker with shared selection state across the All media and Albums sources. */
@Composable
fun AlbumMediaPickerScreen(
    destAlbumName: String,
    disabledIds: Set<FfiMediaUuid>,
    onConfirm: (Set<FfiMediaUuid>) -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val backStack = rememberNavBackStack(PickerAlbumKey(null))
    var selectedIds by remember { mutableStateOf(setOf<FfiMediaUuid>()) }
    var selectedTab by remember { mutableStateOf(AddMediaPickerTab.AllMedia) }

    Column(modifier = modifier.fillMaxSize()) {
        PickerTopBar(
            destAlbumName = destAlbumName,
            selectedTab = selectedTab,
            onTabSelected = { selectedTab = it },
        )

        Box(modifier = Modifier.weight(1f)) {
            when (selectedTab) {
                AddMediaPickerTab.AllMedia -> AddMediaAllMediaPicker(
                    selectedIds = selectedIds,
                    disabledIds = disabledIds,
                    onToggle = { id -> selectedIds = if (id in selectedIds) selectedIds - id else selectedIds + id },
                )
                AddMediaPickerTab.Albums -> AlbumBrowserPicker(
                    backStack = backStack,
                    selectedIds = selectedIds,
                    disabledIds = disabledIds,
                    onToggle = { id -> selectedIds = if (id in selectedIds) selectedIds - id else selectedIds + id },
                    onCancel = onCancel,
                )
            }
        }

        PickerBottomBar(selectedIds.size, onCancel, onConfirm = { onConfirm(selectedIds) })
    }
}

@Composable
private fun PickerTopBar(
    destAlbumName: String,
    selectedTab: AddMediaPickerTab,
    onTabSelected: (AddMediaPickerTab) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    Column(modifier = modifier.fillMaxWidth().background(colors.pink).padding(horizontal = 20.dp, vertical = 16.dp)) {
        Text("Select to add to ${destAlbumName.uppercase()}", style = LascoTheme.type.categoryLarge(), color = Color.White, maxLines = 1)
        Row(modifier = Modifier.fillMaxWidth().padding(top = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PickerTab("All media", selectedTab == AddMediaPickerTab.AllMedia) { onTabSelected(AddMediaPickerTab.AllMedia) }
            PickerTab("Albums", selectedTab == AddMediaPickerTab.Albums) { onTabSelected(AddMediaPickerTab.Albums) }
        }
    }
}

@Composable
private fun PickerTab(label: String, selected: Boolean, onClick: () -> Unit) {
    val colors = LascoTheme.colors
    Text(
        text = label,
        style = LascoTheme.type.body(),
        // The picker header is pink in both themes. `colors.ink` is white in
        // dark mode, so the active tab needs a fixed dark foreground on white.
        color = if (selected) Color(0xFF1A1A1A) else Color.White,
        modifier = Modifier
            .background(if (selected) Color.White else Color.White.copy(alpha = 0.2f))
            .clickable(onClick = onClick)
            .heightIn(min = 48.dp)
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .semantics { role = Role.Tab; this.selected = selected },
    )
}

@Composable
private fun AlbumBrowserPicker(
    backStack: androidx.navigation3.runtime.NavBackStack<NavKey>,
    selectedIds: Set<FfiMediaUuid>,
    disabledIds: Set<FfiMediaUuid>,
    onToggle: (FfiMediaUuid) -> Unit,
    onCancel: () -> Unit,
) {
    NavDisplay(
        backStack = backStack,
        onBack = { if (backStack.size > 1) backStack.removeLastOrNull() else onCancel() },
        modifier = Modifier.fillMaxSize(),
        entryDecorators = listOf(rememberSaveableStateHolderNavEntryDecorator(), rememberViewModelStoreNavEntryDecorator()),
        entryProvider = entryProvider {
            entry<PickerAlbumKey> { key ->
                val path = backStack.filterIsInstance<PickerAlbumKey>().filter { it.albumId != null }
                val title = if (path.isEmpty()) "ALBUMS" else path.joinToString(" / ") { it.albumName.orEmpty().uppercase() }
                val backLabel = path.dropLast(1).lastOrNull()?.albumName ?: "Albums"
                AlbumListScreen(
                    albumId = key.albumId?.let(::FfiAlbumUuid),
                    albumName = key.albumName,
                    title = title,
                    backLabel = backLabel,
                    onBack = if (key.albumId != null) { { backStack.removeLastOrNull() } } else null,
                    onOpenChild = { child -> backStack.add(PickerAlbumKey(child.albumId.value, child.name)) },
                    pickerState = AlbumPickerState(disabledIds, selectedIds, onToggle),
                )
            }
        },
    )
}

@Composable
private fun AddMediaAllMediaPicker(
    selectedIds: Set<FfiMediaUuid>,
    disabledIds: Set<FfiMediaUuid>,
    onToggle: (FfiMediaUuid) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: RecentMediaViewModel = viewModel(factory = RecentMediaViewModel.Factory),
) {
    val colors = LascoTheme.colors
    val media = viewModel.media.collectAsLazyPagingItems()
    val showingOrphans by viewModel.showingOrphans.collectAsStateWithLifecycle()
    val repo = LibraryRepository.from(LocalContext.current)

    Column(modifier = modifier.fillMaxSize().background(colors.bg).padding(horizontal = 20.dp)) {
        Row(modifier = Modifier.fillMaxWidth().padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("LIBRARY", style = LascoTheme.type.categoryLarge(), color = colors.ink, modifier = Modifier.weight(1f))
            Text(if (showingOrphans) "Orphan" else "All", style = LascoTheme.type.body(14), color = colors.inkSub)
            LascoToggle(
                checked = showingOrphans,
                onCheckedChange = viewModel::setShowingOrphans,
                modifier = Modifier.padding(start = 4.dp),
            )
        }
        when {
            media.loadState.refresh is LoadState.Loading && media.itemCount == 0 -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator(color = colors.ink) }
            media.loadState.refresh is LoadState.Error && media.itemCount == 0 -> Text("Could not load media. Tap to retry.", color = colors.inkMuted, modifier = Modifier.clickable { media.retry() })
            media.itemCount == 0 -> Text(if (showingOrphans) "No orphan media." else "No media yet.", color = colors.inkMuted)
            else -> BoxWithConstraints {
                LazyVerticalGrid(
                    columns = GridCells.Fixed(if (maxWidth > 500.dp) 3 else 2),
                    state = rememberLazyGridState(),
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    items(count = media.itemCount, key = media.itemKey { it.mediaId.value }) { index ->
                        media[index]?.let { item ->
                            PickerMediaCell(item, repo, item.mediaId in selectedIds, item.mediaId in disabledIds) { onToggle(item.mediaId) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun PickerMediaCell(item: FfiMediaItem, repo: LibraryRepository, isSelected: Boolean, isDisabled: Boolean, onToggle: () -> Unit) {
    val colors = LascoTheme.colors
    Box(
        modifier = Modifier.fillMaxWidth().background(colors.surfaceAlt).clickable(enabled = !isDisabled, onClick = onToggle),
    ) {
        MediaThumbnail(mediaId = item.mediaId, repo = repo, modifier = Modifier.fillMaxWidth())
        if (isDisabled) Box(Modifier.fillMaxSize().background(colors.bg.copy(alpha = 0.58f)))
        if (isSelected) Text("✓", style = LascoTheme.type.body(16), color = colors.pink, modifier = Modifier.align(Alignment.TopEnd).padding(6.dp))
    }
}

@Composable
private fun PickerBottomBar(count: Int, onCancel: () -> Unit, onConfirm: () -> Unit, modifier: Modifier = Modifier) {
    val colors = LascoTheme.colors
    Row(modifier = modifier.fillMaxWidth().background(colors.bg).padding(horizontal = 20.dp, vertical = 16.dp)) {
        Text("Cancel", style = LascoTheme.type.body(), color = colors.inkMuted, modifier = Modifier.clickable { onCancel() })
        Spacer(modifier = Modifier.weight(1f))
        Text("Add ($count)", style = LascoTheme.type.body(), color = if (count == 0) colors.inkMuted else colors.ink, modifier = Modifier.clickable(enabled = count > 0) { onConfirm() })
    }
}
