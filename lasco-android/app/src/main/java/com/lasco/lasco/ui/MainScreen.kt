package com.lasco.lasco.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.LocalContext
import android.content.Intent
import android.net.Uri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lasco.lasco.LascoApp
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
import com.lasco.lasco.ui.media.MediaDetailSource
import com.lasco.lasco.ui.media.DetailTarget
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
    val context = LocalContext.current
    val releasePolicy by (context.applicationContext as LascoApp).releasePolicy.decision.collectAsStateWithLifecycle()
    val showUpdateBanner =
        (tab == AppTab.Home || tab == AppTab.Albums) && releasePolicy?.updateAvailable == true
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
        Column(modifier = Modifier.fillMaxSize()) {
            if (showUpdateBanner) {
                val decision = requireNotNull(releasePolicy)
                Text(
                    text = "${decision.message}  Update",
                    color = colors.bg,
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(colors.pink)
                        .clickable {
                            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(decision.storeUrl)))
                        }
                        .padding(horizontal = 16.dp, vertical = 10.dp),
                )
            }

            Box(modifier = Modifier.weight(1f)) {
                when (tab) {
                    AppTab.Home -> NavDisplay(
                        backStack = homeBackStack,
                        onBack = { homeBackStack.removeLastOrNull() },
                        modifier = Modifier.fillMaxSize(),
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
                            entry<HomeKey> {
                                RecentMediaScreen(
                                    modifier = Modifier.fillMaxSize(),
                                    onOpenMedia = { position, mediaId, showingOrphans ->
                                        homeBackStack.add(
                                            MediaDetailKey(
                                                if (showingOrphans) MediaDetailSource.OrphansByDate else MediaDetailSource.HomeByDate,
                                                position,
                                                DetailTarget.Media(mediaId.value),
                                            ),
                                        )
                                    },
                                    onOpenAlbum = { openAlbum(it) },
                                )
                            }
                            entry<MediaDetailKey> { key ->
                                MediaDetailScreen(
                                    source = key.source,
                                    startPosition = key.startPosition,
                                    expectedTarget = key.expectedTarget,
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
            }
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
