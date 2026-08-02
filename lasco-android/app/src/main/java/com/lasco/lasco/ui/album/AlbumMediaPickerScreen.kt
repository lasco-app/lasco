package com.lasco.lasco.ui.album

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.ui.NavDisplay
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.serialization.Serializable

@Serializable
private data class PickerAlbumKey(val albumId: String?, val albumName: String? = null) : NavKey

/**
 * Full-screen, modal media picker for "Add file from library". Reuses
 * AlbumListScreen for browsing each level, in picker mode, with its own
 * isolated Nav3 backstack that is discarded when the picker is dismissed.
 */
@Composable
fun AlbumMediaPickerScreen(
    destAlbumName: String,
    disabledIds: Set<String>,
    onConfirm: (Set<String>) -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val backStack = rememberNavBackStack(PickerAlbumKey(null))
    var selectedIds by remember { mutableStateOf(setOf<String>()) }

    Column(modifier = modifier.fillMaxSize()) {
        PickerTopBar(destAlbumName = destAlbumName)

        Box(modifier = Modifier.weight(1f)) {
            NavDisplay(
                backStack = backStack,
                onBack = {
                    if (backStack.size > 1) backStack.removeLastOrNull() else onCancel()
                },
                modifier = Modifier.fillMaxSize(),
                entryDecorators = listOf(
                    rememberSaveableStateHolderNavEntryDecorator(),
                    rememberViewModelStoreNavEntryDecorator(),
                ),
                entryProvider = entryProvider {
                    entry<PickerAlbumKey> { key ->
                        val path = backStack.filterIsInstance<PickerAlbumKey>().filter { it.albumId != null }
                        val title = if (path.isEmpty()) "ALBUMS" else path.joinToString(" / ") { it.albumName.orEmpty().uppercase() }
                        val backLabel = path.dropLast(1).lastOrNull()?.albumName ?: "Albums"

                        AlbumListScreen(
                            albumId = key.albumId,
                            albumName = key.albumName,
                            title = title,
                            backLabel = backLabel,
                            onBack = if (key.albumId != null) { { backStack.removeLastOrNull() } } else null,
                            onOpenChild = { child -> backStack.add(PickerAlbumKey(child.albumId, child.name)) },
                            pickerState = AlbumPickerState(
                                disabledIds = disabledIds,
                                selectedIds = selectedIds,
                                onToggle = { id -> selectedIds = if (id in selectedIds) selectedIds - id else selectedIds + id },
                            ),
                        )
                    }
                },
            )
        }

        PickerBottomBar(
            count = selectedIds.size,
            onCancel = onCancel,
            onConfirm = { onConfirm(selectedIds) },
        )
    }
}

@Composable
private fun PickerTopBar(destAlbumName: String, modifier: Modifier = Modifier) {
    val colors = LascoTheme.colors
    Row(modifier = modifier.fillMaxWidth().background(colors.pink).padding(horizontal = 20.dp, vertical = 20.dp)) {
        Text(
            text = "Select to add to ${destAlbumName.uppercase()}",
            style = LascoTheme.type.categoryLarge(),
            color = Color.White,
            maxLines = 1,
        )
    }
}

@Composable
private fun PickerBottomBar(
    count: Int,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(colors.bg)
            .padding(horizontal = 20.dp, vertical = 16.dp),
    ) {
        Text(
            text = "Cancel",
            style = LascoTheme.type.body(),
            color = colors.inkMuted,
            modifier = Modifier.clickable { onCancel() },
        )
        Spacer(modifier = Modifier.weight(1f))
        Text(
            text = "Add ($count)",
            style = LascoTheme.type.body(),
            color = if (count == 0) colors.inkMuted else colors.ink,
            modifier = Modifier.clickable(enabled = count > 0) { onConfirm() },
        )
    }
}
