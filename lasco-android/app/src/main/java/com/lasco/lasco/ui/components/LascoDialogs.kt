package com.lasco.lasco.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiMediaItem

/**
 * Flat, square-cornered dialog shell shared by the ported sheets below.
 * Plain Compose Dialog rather than Material AlertDialog, which defaults to
 * rounded corners that don't match the app's "no radius, ever" panel look.
 */
@Composable
private fun LascoDialogShell(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    val colors = LascoTheme.colors
    Dialog(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(colors.bg)
                .lascoPanel()
                .padding(24.dp),
        ) {
            content()
        }
    }
}

/**
 * Ported from NewAlbumSheet. A single text field with Cancel/Confirm,
 * confirm disabled while the trimmed value is empty.
 */
@Composable
fun LascoTextInputDialog(
    title: String,
    initialValue: String = "",
    fieldLabel: String = "Name",
    confirmLabel: String = "Confirm",
    onConfirm: (String) -> Unit,
    onCancel: () -> Unit,
) {
    val colors = LascoTheme.colors
    var value by remember { mutableStateOf(initialValue) }

    LascoDialogShell(onDismiss = onCancel) {
        Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Text(text = title, style = LascoTheme.type.categoryLarge(), color = colors.ink)
            LascoField(label = fieldLabel, value = value, onValueChange = { value = it })
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = "Cancel",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { onCancel() },
                )
                Spacer(modifier = Modifier.weight(1f))
                val trimmed = value.trim()
                Text(
                    text = confirmLabel,
                    style = LascoTheme.type.body(),
                    color = if (trimmed.isEmpty()) colors.inkMuted else colors.ink,
                    modifier = Modifier.clickable(enabled = trimmed.isNotEmpty()) { onConfirm(trimmed) },
                )
            }
        }
    }
}

/** A confirm-only dialog for destructive actions (delete album/group). */
@Composable
fun LascoConfirmDialog(
    title: String,
    message: String,
    confirmLabel: String = "Delete",
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
) {
    val colors = LascoTheme.colors
    LascoDialogShell(onDismiss = onCancel) {
        Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Text(text = title, style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Text(text = message, style = LascoTheme.type.body(14), color = colors.inkMuted)
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = "Cancel",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { onCancel() },
                )
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = confirmLabel,
                    style = LascoTheme.type.body(),
                    color = colors.error,
                    modifier = Modifier.clickable { onConfirm() },
                )
            }
        }
    }
}

/**
 * Ported from OpenAlbumPickerSheet / AlbumPickerView. A flat panel listing
 * albums to choose one from.
 */
@Composable
fun AlbumPickerDialog(
    title: String,
    albums: List<FfiAlbum>,
    onSelect: (FfiAlbum) -> Unit,
    onCancel: () -> Unit,
) {
    val colors = LascoTheme.colors
    LascoDialogShell(onDismiss = onCancel) {
        Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(text = title, style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Box(modifier = Modifier.fillMaxWidth().lascoPanel()) {
                LazyColumn {
                    items(albums, key = { it.albumId }) { album ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { onSelect(album) }
                                .padding(horizontal = 16.dp, vertical = 12.dp),
                        ) {
                            Text(text = album.name, style = LascoTheme.type.body(), color = colors.ink)
                        }
                    }
                }
            }
            Row(modifier = Modifier.fillMaxWidth()) {
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "Cancel",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { onCancel() },
                )
            }
        }
    }
}

/**
 * Multi-select picker over every media item in the library, used by "Add
 * file from library" to attach existing media to the current album without
 * duplicating the underlying file. Items already in the album are shown
 * dimmed and disabled rather than omitted.
 */
@Composable
fun MediaPickerDialog(
    title: String,
    media: List<FfiMediaItem>,
    disabledIds: Set<String>,
    repo: LibraryRepository,
    onConfirm: (Set<String>) -> Unit,
    onCancel: () -> Unit,
) {
    val colors = LascoTheme.colors
    var selected by remember { mutableStateOf(setOf<String>()) }

    LascoDialogShell(onDismiss = onCancel) {
        Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(text = title, style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Box(modifier = Modifier.fillMaxWidth().lascoPanel()) {
                LazyColumn {
                    items(media, key = { it.mediaId }) { item ->
                        val disabled = item.mediaId in disabledIds
                        val isSelected = item.mediaId in selected
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable(enabled = !disabled) {
                                    selected = if (isSelected) selected - item.mediaId else selected + item.mediaId
                                }
                                .padding(horizontal = 16.dp, vertical = 8.dp),
                        ) {
                            MediaThumbnail(
                                mediaId = item.mediaId,
                                repo = repo,
                                modifier = Modifier.size(40.dp),
                            )
                            Text(
                                text = item.name ?: item.filenameOriginal,
                                style = LascoTheme.type.body(14),
                                color = if (disabled) colors.inkMuted else colors.ink,
                                maxLines = 1,
                                modifier = Modifier.weight(1f).padding(start = 10.dp).align(Alignment.CenterVertically),
                            )
                            if (isSelected) {
                                Text(text = "✓", style = LascoTheme.type.body(16), color = colors.pink)
                            }
                        }
                    }
                }
            }
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = "Cancel",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { onCancel() },
                )
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "Add (${selected.size})",
                    style = LascoTheme.type.body(),
                    color = if (selected.isEmpty()) colors.inkMuted else colors.ink,
                    modifier = Modifier.clickable(enabled = selected.isNotEmpty()) { onConfirm(selected) },
                )
            }
        }
    }
}
