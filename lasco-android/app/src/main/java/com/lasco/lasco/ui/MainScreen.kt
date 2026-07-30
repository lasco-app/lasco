package com.lasco.lasco.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.ui.NavDisplay
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import com.lasco.lasco.ui.album.AlbumKey
import com.lasco.lasco.ui.album.AlbumsScreen
import com.lasco.lasco.ui.components.AppTab
import com.lasco.lasco.ui.components.FloatingTabBar
import com.lasco.lasco.ui.manage.ManageScreen
import com.lasco.lasco.ui.media.MediaDetailKey
import com.lasco.lasco.ui.media.MediaDetailScreen
import com.lasco.lasco.ui.media.RecentMediaScreen
import com.lasco.lasco.ui.status.StatusScreen
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.serialization.Serializable

@Serializable
private data object HomeKey : NavKey

/**
 * Post-open root screen, the Android equivalent of Swift's MainView. Owns
 * the selected tab and the Home/Albums backstacks (Nav3's guidance for
 * bottom-nav apps: one independent backstack per tab). Media Detail lives
 * on whichever of those backstacks pushed it, so the FloatingTabBar hides
 * exactly when the active tab's stack is topped by it, matching Swift's
 * HideTabBarKey preference. Status/Manage get no backstack, single screen.
 */
@Composable
fun MainScreen(
    modifier: Modifier = Modifier,
    onSignedOut: () -> Unit = {},
    onDeleteLibrary: () -> Unit = {},
) {
    var tab by remember { mutableStateOf(AppTab.Home) }
    val homeBackStack = rememberNavBackStack(HomeKey)
    val albumsBackStack = rememberNavBackStack(AlbumKey(null))
    val colors = LascoTheme.colors
    var isAlbumPickerVisible by remember { mutableStateOf(false) }

    fun openAlbum(albumId: String) {
        albumsBackStack.clear()
        albumsBackStack.add(AlbumKey(null))
        albumsBackStack.add(AlbumKey(albumId))
        tab = AppTab.Albums
    }

    val activeBackStack = if (tab == AppTab.Home) homeBackStack else albumsBackStack
    val showTabBar = activeBackStack.lastOrNull() !is MediaDetailKey && !isAlbumPickerVisible

    Box(modifier = modifier.fillMaxSize().background(colors.bg)) {
        when (tab) {
            AppTab.Home -> NavDisplay(
                backStack = homeBackStack,
                onBack = { homeBackStack.removeLastOrNull() },
                modifier = Modifier.fillMaxSize(),
                entryDecorators = listOf(
                    rememberSaveableStateHolderNavEntryDecorator(),
                    rememberViewModelStoreNavEntryDecorator(),
                ),
                entryProvider = entryProvider {
                    entry<HomeKey> {
                        RecentMediaScreen(
                            modifier = Modifier.fillMaxSize(),
                            onOpenMedia = { mediaId ->
                                homeBackStack.add(MediaDetailKey(sourceAlbumId = null, startMediaId = mediaId))
                            },
                            onOpenAlbum = { openAlbum(it) },
                        )
                    }
                    entry<MediaDetailKey> { key ->
                        MediaDetailScreen(
                            sourceAlbumId = key.sourceAlbumId,
                            startMediaId = key.startMediaId,
                            onBack = { homeBackStack.removeLastOrNull() },
                            onOpenAlbum = { openAlbum(it) },
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                },
            )
            AppTab.Albums -> AlbumsScreen(
                backStack = albumsBackStack,
                modifier = Modifier.fillMaxSize(),
                onOpenAlbum = { openAlbum(it) },
                onPickerVisibleChange = { isAlbumPickerVisible = it },
            )
            AppTab.Status -> StatusScreen(modifier = Modifier.fillMaxSize())
            AppTab.Manage -> ManageScreen(
                modifier = Modifier.fillMaxSize(),
                onSignedOut = onSignedOut,
                onDeleteLibrary = onDeleteLibrary,
            )
        }

        if (showTabBar) {
            FloatingTabBar(
                selectedTab = tab,
                onTabSelected = { tab = it },
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(horizontal = 44.dp, vertical = 24.dp),
            )
        }
    }
}
