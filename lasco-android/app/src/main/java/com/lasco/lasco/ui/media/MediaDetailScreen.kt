package com.lasco.lasco.ui.media

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.AnchoredDraggableDefaults
import androidx.compose.foundation.gestures.AnchoredDraggableState
import androidx.compose.foundation.gestures.DraggableAnchors
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.anchoredDraggable
import androidx.compose.foundation.gestures.animateTo
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.draw.blur
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import android.graphics.BitmapFactory
import androidx.compose.material3.Icon
import androidx.media3.common.MediaItem as ExoMediaItem
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import com.lasco.lasco.R
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.media.VideoFileCache
import com.lasco.lasco.ui.components.AlbumCell
import com.lasco.lasco.ui.components.LascoTextInputDialog
import com.lasco.lasco.ui.components.MediaThumbnail
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import java.io.File
import java.text.DateFormat
import java.text.ParseException
import java.text.SimpleDateFormat
import java.util.Locale
import java.util.TimeZone
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiMediaItem

private val videoExtensions = setOf("mp4", "mov", "m4v", "avi", "3gp", "webm", "mkv")

private enum class PanelAnchor { Collapsed, Expanded }

private val PanelCollapsedHeight = 72.dp
private const val PanelExpandRatio = 0.58f

private fun isVideo(item: FfiMediaItem): Boolean =
    item.filenameOriginal.substringAfterLast('.', "").lowercase() in videoExtensions

private fun displayItemForPage(
    page: Int,
    items: List<DetailItem>,
    groupMediaCache: Map<String, List<FfiMediaItem>>,
    groupMediaIndex: Int,
    currentIndex: Int?,
): FfiMediaItem? = when (val entry = items.getOrNull(page)) {
    is DetailItem.Media -> entry.item
    is DetailItem.Group -> {
        val media = groupMediaCache[entry.group.groupId] ?: emptyList()
        val idx = if (page == currentIndex) groupMediaIndex else 0
        media.getOrNull(idx)
    }
    null -> null
}

/**
 * Media Detail, the Android equivalent of Swift's MediaDetailView (iOS body).
 * Full-screen pager over media/groups with real image and video rendering,
 * a group thumbnail strip, a live-photo toggle, and a collapsible info panel.
 */
@Composable
fun MediaDetailScreen(
    sourceAlbumId: String?,
    startMediaId: String,
    onBack: () -> Unit,
    onOpenAlbum: (String) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: MediaDetailViewModel = viewModel(
        key = "$sourceAlbumId:$startMediaId",
        factory = MediaDetailViewModel.factory(sourceAlbumId, startMediaId),
    ),
) {
    val context = LocalContext.current
    val colors = LascoTheme.colors
    val repo = LibraryRepository.from(context)
    val prefs = remember { Prefs.from(context) }
    val expertMode by prefs.expertMode.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()

    BackHandler(onBack = onBack)

    val items by viewModel.items.collectAsStateWithLifecycle()
    val startIndex by viewModel.currentIndex.collectAsStateWithLifecycle()

    if (startIndex == null || items.isEmpty()) {
        Box(modifier = modifier.fillMaxSize().background(Color.Black))
        return
    }

    val pagerState = rememberPagerState(initialPage = startIndex!!) { items.size }

    LaunchedEffect(pagerState.currentPage) { viewModel.setCurrentIndex(pagerState.currentPage) }

    val currentIndex by viewModel.currentIndex.collectAsStateWithLifecycle()
    val groupMediaIndex by viewModel.groupMediaIndex.collectAsStateWithLifecycle()
    val groupMediaCache by viewModel.groupMediaCache.collectAsStateWithLifecycle()
    val currentGroupMedia by viewModel.currentGroupMedia.collectAsStateWithLifecycle()
    val currentDisplayItem by viewModel.currentDisplayItem.collectAsStateWithLifecycle()
    val infoDisplayItem by viewModel.infoDisplayItem.collectAsStateWithLifecycle()
    val showingLivePhotoVideo by viewModel.showingLivePhotoVideo.collectAsStateWithLifecycle()
    val containingAlbums by viewModel.containingAlbums.collectAsStateWithLifecycle()

    val panelState = remember { AnchoredDraggableState(initialValue = PanelAnchor.Collapsed) }
    var showRenameDialog by remember { mutableStateOf(false) }
    var showCounter by remember { mutableStateOf(true) }

    LaunchedEffect(currentIndex) {
        showCounter = true
        delay(1000)
        showCounter = false
    }

    if (showRenameDialog && currentDisplayItem != null) {
        val media = currentDisplayItem!!
        LascoTextInputDialog(
            title = "Rename",
            fieldLabel = "Name",
            initialValue = media.name ?: media.filenameOriginal,
            onConfirm = { name ->
                showRenameDialog = false
                viewModel.rename(media.mediaId, name)
            },
            onCancel = { showRenameDialog = false },
        )
    }

    Box(modifier = modifier.fillMaxSize().background(Color.Black)) {
        HorizontalPager(state = pagerState, modifier = Modifier.fillMaxSize()) { page ->
            val entry = items[page]
            LaunchedEffect(entry) {
                if (entry is DetailItem.Group) viewModel.loadGroupMediaIfNeeded(entry.group.groupId)
            }
            val displayItem = displayItemForPage(page, items, groupMediaCache, groupMediaIndex, currentIndex)
            val isActive = page == currentIndex
            if (displayItem != null) {
                LaunchedEffect(displayItem.mediaId, isActive) {
                    if (isActive) viewModel.loadLivePhotoVideoIfNeeded(displayItem)
                }
                val liveVideoItem = if (isActive && showingLivePhotoVideo) infoDisplayItem else null
                MediaPageContent(
                    item = displayItem,
                    repo = repo,
                    isActive = isActive,
                    liveVideoItem = liveVideoItem,
                    modifier = Modifier.fillMaxSize(),
                )
            } else {
                Box(modifier = Modifier.fillMaxSize().background(Color.Black))
            }
        }

        val currentEntry = items.getOrNull(currentIndex ?: -1)
        if (currentEntry is DetailItem.Group && currentGroupMedia.isNotEmpty()) {
            GroupThumbnailStrip(
                media = currentGroupMedia,
                selectedIndex = groupMediaIndex,
                repo = repo,
                onSelect = { viewModel.setGroupMediaIndex(it) },
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 88.dp),
            )
        }

        Row(
            modifier = Modifier
                .align(Alignment.TopStart)
                .fillMaxWidth()
                .windowInsetsPadding(WindowInsets.statusBars)
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Box(
                modifier = Modifier
                    .size(36.dp)
                    .background(Color.Black)
                    .border(2.dp, Color.White)
                    .clickable { onBack() },
                contentAlignment = Alignment.Center,
            ) {
                Text(text = "←", style = LascoTheme.type.body(18), color = Color.White)
            }

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (currentDisplayItem?.appleLivePhotoMediaId != null) {
                    Text(
                        text = if (showingLivePhotoVideo) "IMG" else "▶",
                        style = LascoTheme.type.pixel(14),
                        color = Color.White,
                        modifier = Modifier
                            .background(Color.Black)
                            .border(2.dp, Color.White)
                            .clickable { viewModel.toggleLivePhotoVideo() }
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                    )
                }
                AnimatedVisibility(visible = showCounter, enter = fadeIn(), exit = fadeOut()) {
                    Text(
                        text = "${(currentIndex ?: 0) + 1} / ${items.size}",
                        style = LascoTheme.type.pixel(14),
                        color = Color.White,
                        modifier = Modifier
                            .background(Color.Black.copy(alpha = 0.5f))
                            .border(1.dp, Color.White.copy(alpha = 0.5f))
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                    )
                }
            }
        }

        BoxWithConstraints(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth(),
        ) {
            val density = LocalDensity.current
            val expandedHeight = maxHeight * PanelExpandRatio
            val travelPx = with(density) { (expandedHeight - PanelCollapsedHeight).toPx() }

            panelState.updateAnchors(
                DraggableAnchors {
                    PanelAnchor.Expanded at 0f
                    PanelAnchor.Collapsed at travelPx
                },
            )

            val infoExpanded = panelState.currentValue == PanelAnchor.Expanded
            val progress = (1f - (panelState.requireOffset() / travelPx)).coerceIn(0f, 1f)

            // Lets the panel consume vertical drags before the scrollable content below it does,
            // so dragging anywhere on the card (not just the header) moves the panel first and
            // only scrolls the content once the panel is fully expanded.
            val panelNestedScrollConnection = remember(panelState) {
                object : NestedScrollConnection {
                    override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                        val delta = available.y
                        return if (delta < 0f && panelState.requireOffset() > 0f) {
                            Offset(0f, panelState.dispatchRawDelta(delta))
                        } else {
                            Offset.Zero
                        }
                    }

                    override fun onPostScroll(
                        consumed: Offset,
                        available: Offset,
                        source: NestedScrollSource,
                    ): Offset {
                        val delta = available.y
                        return if (delta > 0f) {
                            Offset(0f, panelState.dispatchRawDelta(delta))
                        } else {
                            Offset.Zero
                        }
                    }

                    override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
                        val offset = panelState.requireOffset()
                        if (offset != 0f && offset != travelPx) {
                            panelState.settle(AnchoredDraggableDefaults.SnapAnimationSpec)
                        }
                        return Velocity.Zero
                    }
                }
            }

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(expandedHeight)
                    .offset { IntOffset(0, panelState.requireOffset().roundToInt()) }
                    .lascoPanel()
                    .nestedScroll(panelNestedScrollConnection)
                    .anchoredDraggable(panelState, Orientation.Vertical)
                    .padding(16.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth().clickable {
                        scope.launch {
                            panelState.animateTo(if (infoExpanded) PanelAnchor.Collapsed else PanelAnchor.Expanded)
                        }
                    },
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(
                        text = currentDisplayItem?.name ?: "",
                        style = LascoTheme.type.title(18),
                        color = colors.ink,
                        maxLines = 1,
                        modifier = Modifier.weight(1f),
                    )
                    Text(
                        text = "✎",
                        style = LascoTheme.type.body(16),
                        color = if (showingLivePhotoVideo) colors.inkMuted else colors.ink,
                        modifier = Modifier.clickable(enabled = !showingLivePhotoVideo) { showRenameDialog = true },
                    )
                    Text(
                        text = if (infoExpanded) "▾" else "▴",
                        style = LascoTheme.type.body(16),
                        color = colors.ink,
                    )
                }

                Column(
                    modifier = Modifier
                        .padding(top = 12.dp)
                        .weight(1f)
                        .graphicsLayer { alpha = progress }
                        .verticalScroll(rememberScrollState()),
                ) {
                    infoDisplayItem?.let { media ->
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            if (media.appleAaeMediaId != null) {
                                Text(
                                    text = "There is an associated metadata edit file (.aae). It's included in the library, but not currently applied when showing the photo (crop, rotation, etc).",
                                    style = LascoTheme.type.mono(11),
                                    color = colors.inkMuted,
                                    modifier = Modifier.padding(bottom = 8.dp),
                                )
                            }
                            MetadataRow("FILE", media.filenameOriginal)
                            MetadataRow("DATE", formatMediaDate(media.date))
                            MetadataRow("SIZE", formatSize(media.sizeBytes.toLong()))
                            MetadataRow("ADDED BY", media.author)
                            if (expertMode) {
                                MetadataRow("ID", media.mediaId)
                                MetadataRow("HASH", media.contentHash)
                                media.appleAaeMediaId?.let { MetadataRow("AAE", it) }
                            }
                        }
                    }

                    Row(
                        modifier = Modifier
                            .padding(top = 12.dp)
                            .border(1.dp, colors.ink.copy(alpha = 0.4f))
                            .clickable(enabled = progress > 0.5f) {
                                currentDisplayItem?.let { item ->
                                    scope.launch { exportMedia(context, repo, item) }
                                }
                            }
                            .padding(horizontal = 14.dp, vertical = 10.dp),
                    ) {
                        Text(text = "EXPORT", style = LascoTheme.type.pixel(14), color = colors.ink)
                    }

                    if (!showingLivePhotoVideo && containingAlbums.isNotEmpty()) {
                        val heading = if (sourceAlbumId == null) {
                            if (containingAlbums.size == 1) "CONTAINED IN THIS ALBUM" else "CONTAINED IN THESE ALBUMS"
                        } else {
                            "ALSO IN THESE ALBUMS"
                        }
                        Text(
                            text = heading,
                            style = LascoTheme.type.categorySmall(16),
                            color = colors.ink,
                            modifier = Modifier.padding(top = 16.dp, bottom = 8.dp),
                        )
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            itemsIndexed(containingAlbums, key = { _, album -> album.albumId }) { _, album ->
                                AlbumCell(
                                    album = album,
                                    repo = repo,
                                    modifier = Modifier.width(96.dp),
                                    onClick = { if (progress > 0.5f) onOpenAlbum(album.albumId) },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

private suspend fun exportMedia(context: Context, repo: LibraryRepository, item: FfiMediaItem) {
    val file = VideoFileCache.file(context, repo, item.mediaId, item.filenameOriginal) ?: return
    val uri = FileProvider.getUriForFile(context, "com.lasco.lasco.fileprovider", file)
    val ext = item.filenameOriginal.substringAfterLast('.', "")
    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
    val intent = Intent(Intent.ACTION_SEND).apply {
        type = mime
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(Intent.createChooser(intent, null))
}

@Composable
private fun MediaPageContent(
    item: FfiMediaItem,
    repo: LibraryRepository,
    isActive: Boolean,
    liveVideoItem: FfiMediaItem?,
    modifier: Modifier = Modifier,
) {
    val videoItem = when {
        isVideo(item) -> item
        liveVideoItem != null -> liveVideoItem
        else -> null
    }
    Box(modifier = modifier.background(Color.Black), contentAlignment = Alignment.Center) {
        if (videoItem != null) {
            VideoCell(item = videoItem, repo = repo, isActive = isActive)
        } else {
            ImageCell(item = item, repo = repo)
        }
    }
}

@Composable
private fun ImageCell(item: FfiMediaItem, repo: LibraryRepository) {
    var thumbnail by remember(item.mediaId) { mutableStateOf<ImageBitmap?>(null) }
    var fullImage by remember(item.mediaId) { mutableStateOf<ImageBitmap?>(null) }

    LaunchedEffect(item.mediaId) {
        repo.mediaThumbnail(item.mediaId)?.let { bytes ->
            thumbnail = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
        }
        repo.mediaBytes(item.mediaId)?.let { bytes ->
            fullImage = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
        }
    }

    val full = fullImage
    val thumb = thumbnail
    when {
        full != null -> Image(
            bitmap = full,
            contentDescription = null,
            contentScale = ContentScale.Fit,
            modifier = Modifier.fillMaxSize(),
        )
        thumb != null -> Image(
            bitmap = thumb,
            contentDescription = null,
            contentScale = ContentScale.Fit,
            modifier = Modifier.fillMaxSize().blur(8.dp),
        )
        else -> Icon(
            painter = painterResource(R.drawable.ic_tab_image),
            contentDescription = null,
            tint = Color.White.copy(alpha = 0.3f),
            modifier = Modifier.size(72.dp),
        )
    }
}

@Composable
private fun VideoCell(item: FfiMediaItem, repo: LibraryRepository, isActive: Boolean) {
    val context = LocalContext.current
    var file by remember(item.mediaId) { mutableStateOf<File?>(null) }

    LaunchedEffect(item.mediaId) {
        file = VideoFileCache.file(context, repo, item.mediaId, item.filenameOriginal)
    }

    val exoPlayer = remember(item.mediaId) { ExoPlayer.Builder(context).build() }
    DisposableEffect(exoPlayer) { onDispose { exoPlayer.release() } }

    LaunchedEffect(file) {
        file?.let {
            exoPlayer.setMediaItem(ExoMediaItem.fromUri(Uri.fromFile(it)))
            exoPlayer.prepare()
        }
    }

    LaunchedEffect(isActive) {
        if (isActive) exoPlayer.play() else exoPlayer.pause()
    }

    AndroidView(
        factory = {
            PlayerView(it).apply {
                player = exoPlayer
                useController = false
            }
        },
        modifier = Modifier.fillMaxSize(),
    )
}

@Composable
private fun GroupThumbnailStrip(
    media: List<FfiMediaItem>,
    selectedIndex: Int,
    repo: LibraryRepository,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    val listState = rememberLazyListState()
    LaunchedEffect(selectedIndex) { listState.animateScrollToItem(selectedIndex) }

    LazyRow(
        state = listState,
        modifier = modifier
            .height(66.dp)
            .background(Color.Black.copy(alpha = 0.6f)),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        contentPadding = PaddingValues(horizontal = 12.dp),
    ) {
        itemsIndexed(media, key = { _, m -> m.mediaId }) { idx, m ->
            Box(
                modifier = Modifier
                    .size(52.dp)
                    .border(2.dp, if (idx == selectedIndex) colors.pink else Color.Transparent)
                    .clickable { onSelect(idx) },
            ) {
                MediaThumbnail(mediaId = m.mediaId, repo = repo, modifier = Modifier.fillMaxSize())
            }
        }
    }
}

@Composable
private fun MetadataRow(label: String, value: String) {
    val colors = LascoTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(text = label, style = LascoTheme.type.mono(12), color = colors.inkMuted, modifier = Modifier.width(64.dp))
        Text(text = value, style = LascoTheme.type.mono(12), color = colors.inkSub, maxLines = 1)
    }
}

private fun formatSize(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val kb = bytes / 1024.0
    if (kb < 1024) return "%.1f KB".format(kb)
    val mb = kb / 1024.0
    if (mb < 1024) return "%.1f MB".format(mb)
    return "%.1f GB".format(mb / 1024.0)
}

private val isoDateFormats = listOf(
    "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'",
    "yyyy-MM-dd'T'HH:mm:ss'Z'",
)

private fun formatMediaDate(raw: String): String {
    for (pattern in isoDateFormats) {
        try {
            val parser = SimpleDateFormat(pattern, Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") }
            val date = parser.parse(raw) ?: continue
            return DateFormat.getDateInstance(DateFormat.MEDIUM).format(date)
        } catch (e: ParseException) {
            // try the next pattern
        }
    }
    return raw
}
