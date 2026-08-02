package com.lasco.lasco.ui.album

import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.navigation3.runtime.NavBackStack
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.ui.NavDisplay
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import com.lasco.lasco.ui.media.MediaDetailKey
import com.lasco.lasco.ui.media.MediaDetailScreen
import kotlinx.serialization.Serializable

/** Nav 3 key for one Albums level, root (albumId null) or a specific album. */
@Serializable
data class AlbumKey(val albumId: String?, val albumName: String? = null) : NavKey

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
    onPickerVisibleChange: (Boolean) -> Unit = {},
) {
    NavDisplay(
        backStack = backStack,
        onBack = { backStack.removeLastOrNull() },
        modifier = modifier,
        entryDecorators = listOf(
            rememberSaveableStateHolderNavEntryDecorator(),
            rememberViewModelStoreNavEntryDecorator(),
        ),
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
                val path = backStack.filterIsInstance<AlbumKey>().filter { it.albumId != null }
                val title = if (path.isEmpty()) "ALBUMS" else path.joinToString(" / ") { it.albumName.orEmpty().uppercase() }
                val backLabel = path.dropLast(1).lastOrNull()?.albumName ?: "Albums"

                AlbumListScreen(
                    albumId = key.albumId,
                    albumName = key.albumName,
                    title = title,
                    backLabel = backLabel,
                    onBack = if (key.albumId != null) { { backStack.removeLastOrNull() } } else null,
                    onOpenChild = { child -> backStack.add(AlbumKey(child.albumId, child.name)) },
                    onOpenMedia = { itemId ->
                        backStack.add(MediaDetailKey(sourceAlbumId = key.albumId, startMediaId = itemId))
                    },
                    onPickerVisibleChange = onPickerVisibleChange,
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
