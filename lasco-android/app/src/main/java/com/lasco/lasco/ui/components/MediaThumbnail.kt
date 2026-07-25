package com.lasco.lasco.ui.components

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.lasco.lasco.R
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * Thumbnail slot backed by the Rust FFI. Fetches lazily, only once this
 * composable enters composition, which for LazyVerticalGrid/FlowRow means
 * only once a cell scrolls into view. Mirrors Swift's MediaGridCell, which
 * fetches thumbnailAsync from a `.task(id:)` modifier for the same reason.
 * Shows placeholderThumbnail() until the bytes resolve, or if mediaId is
 * null (album/group with no thumbnail set yet).
 */
@Composable
fun MediaThumbnail(
    mediaId: String?,
    repo: LibraryRepository,
    modifier: Modifier = Modifier,
) {
    var bitmap by remember(mediaId) { mutableStateOf<ImageBitmap?>(null) }

    LaunchedEffect(mediaId) {
        val id = mediaId ?: return@LaunchedEffect
        val bytes = repo.mediaThumbnail(id) ?: return@LaunchedEffect
        bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
    }

    val loaded = bitmap
    if (loaded != null) {
        Image(
            bitmap = loaded,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = modifier.aspectRatio(1f),
        )
    } else {
        val colors = LascoTheme.colors
        Box(modifier = modifier.placeholderThumbnail(), contentAlignment = Alignment.Center) {
            Icon(
                painter = painterResource(R.drawable.ic_tab_image),
                contentDescription = null,
                tint = colors.inkMuted,
                modifier = Modifier.size(28.dp),
            )
        }
    }
}
