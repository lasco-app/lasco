package com.lasco.lasco.ui.album

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.paging.LoadState
import androidx.paging.compose.collectAsLazyPagingItems
import androidx.paging.compose.itemKey
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.components.MediaThumbnail
import com.lasco.lasco.ui.theme.LascoTheme
import uniffi.lasco_ffi.FfiMediaItem

@Composable
fun TrashScreen(
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: TrashViewModel = viewModel(factory = TrashViewModel.factory()),
) {
    val colors = LascoTheme.colors
    val repo = LibraryRepository.from(LocalContext.current)
    val media = viewModel.media.collectAsLazyPagingItems()

    Column(
        modifier = modifier.fillMaxSize().background(colors.bg).padding(20.dp),
    ) {
        Text(
            text = "←  TRASH",
            style = LascoTheme.type.categoryLarge(),
            color = colors.ink,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable(onClick = onBack)
                .padding(bottom = 16.dp),
        )
        when {
            media.loadState.refresh is LoadState.Loading && media.itemCount == 0 ->
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = colors.ink)
                }
            media.itemCount == 0 ->
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text("Trash is empty", style = LascoTheme.type.body(), color = colors.inkMuted)
                }
            else -> LazyVerticalGrid(
                columns = GridCells.Adaptive(140.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(media.itemCount, key = media.itemKey { it.mediaId.value }) { index ->
                    media[index]?.let { item -> TrashMediaCard(item, repo, viewModel::restore) }
                }
            }
        }
    }
}

@Composable
private fun TrashMediaCard(
    item: FfiMediaItem,
    repo: LibraryRepository,
    onRestore: (uniffi.lasco_ffi.FfiMediaUuid) -> Unit,
) {
    val colors = LascoTheme.colors
    Column(modifier = Modifier.background(colors.surfaceAlt)) {
        MediaThumbnail(item.mediaId, repo, modifier = Modifier.fillMaxWidth().aspectRatio(1f))
        Text(item.name ?: item.filenameOriginal, style = LascoTheme.type.body(14), color = colors.ink, maxLines = 1, modifier = Modifier.padding(8.dp))
        Text(
            text = "Restore",
            style = LascoTheme.type.body(14),
            color = colors.ink,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable { onRestore(item.mediaId) }
                .padding(12.dp),
        )
    }
}
