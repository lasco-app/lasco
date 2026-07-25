package com.lasco.lasco.ui.album

import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.NavBackStack
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.ui.NavDisplay
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.media.MediaDetailKey
import com.lasco.lasco.ui.media.MediaDetailScreen
import kotlinx.serialization.Serializable
import uniffi.lasco_ffi.FfiAlbum

/** Nav 3 key for one Albums level, root (albumId null) or a specific album. */
@Serializable
data class AlbumKey(val albumId: String?) : NavKey

/**
 * Nav-stack host for Albums, the Android equivalent of Swift's AlbumsView
 * path navigation, plus Media Detail pushed on top of it. The stack is
 * owned by MainScreen (not here) so a cross-tab "open this album" request
 * from Home can push onto it even while the Albums tab isn't visible. The
 * stack holds album ids, not FfiAlbum snapshots, so a rename of an ancestor
 * album shows up in the breadcrumb immediately instead of only after the
 * level is popped and re-pushed.
 */
@Composable
fun AlbumsScreen(
    backStack: NavBackStack<NavKey>,
    modifier: Modifier = Modifier,
    onOpenAlbum: (String) -> Unit = {},
) {
    val repo = LibraryRepository.from(LocalContext.current)
    val allAlbums by remember { repo.watch(Change.AlbumList) { repo.listAlbums() } }
        .collectAsState(initial = emptyList<FfiAlbum>())

    fun nameOf(albumId: String): String = allAlbums.firstOrNull { it.albumId == albumId }?.name ?: ""

    NavDisplay(
        backStack = backStack,
        onBack = { backStack.removeLastOrNull() },
        modifier = modifier,
        transitionSpec = {
            slideInHorizontally(tween(300)) { it } togetherWith
                slideOutHorizontally(tween(300)) { -it / 3 }
        },
        popTransitionSpec = {
            slideInHorizontally(tween(300)) { -it / 3 } togetherWith
                slideOutHorizontally(tween(300)) { it }
        },
        predictivePopTransitionSpec = {
            slideInHorizontally(tween(300)) { -it / 3 } togetherWith
                slideOutHorizontally(tween(300)) { it }
        },
        entryProvider = entryProvider {
            entry<AlbumKey> { key ->
                val ids = backStack.filterIsInstance<AlbumKey>().mapNotNull { it.albumId }
                val current = key.albumId?.let { id -> allAlbums.firstOrNull { it.albumId == id } }
                val title = if (ids.isEmpty()) "ALBUMS" else ids.joinToString(" / ") { nameOf(it).uppercase() }
                val backLabel = if (ids.size >= 2) nameOf(ids[ids.size - 2]) else "Albums"

                AlbumListScreen(
                    album = current,
                    title = title,
                    backLabel = backLabel,
                    onBack = if (key.albumId != null) { { backStack.removeLastOrNull() } } else null,
                    onOpenChild = { child -> backStack.add(AlbumKey(child.albumId)) },
                    onOpenMedia = { itemId ->
                        backStack.add(MediaDetailKey(sourceAlbumId = key.albumId, startMediaId = itemId))
                    },
                )
            }
            entry<MediaDetailKey> { key ->
                MediaDetailScreen(
                    sourceAlbumId = key.sourceAlbumId,
                    startMediaId = key.startMediaId,
                    onBack = { backStack.removeLastOrNull() },
                    onOpenAlbum = onOpenAlbum,
                )
            }
        },
    )
}
