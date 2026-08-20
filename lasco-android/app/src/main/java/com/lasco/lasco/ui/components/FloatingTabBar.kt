package com.lasco.lasco.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.lasco.lasco.R
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * The four tabs behind the post-open root screen, ported from the Swift
 * AppTab enum. Each carries its outline and solid (selected) icon.
 */
enum class AppTab(val icon: Int, val selectedIcon: Int, val label: String) {
    Home(R.drawable.ic_tab_home, R.drawable.ic_tab_home_solid, "HOME"),
    Albums(R.drawable.ic_tab_image, R.drawable.ic_tab_image_solid, "ALBUMS"),
    Status(R.drawable.ic_tab_disc, R.drawable.ic_tab_disc_solid, "STATUS"),
    Manage(R.drawable.ic_tab_cog, R.drawable.ic_tab_cog_solid, "MANAGE"),
}

/**
 * Ported from the Swift FloatingTabBar. A bordered surfaceAlt bar with
 * icon-only tabs, meant to float over the tab content with horizontal and
 * bottom padding applied by the caller.
 */
@Composable
fun FloatingTabBar(
    selectedTab: AppTab,
    onTabSelected: (AppTab) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(colors.surfaceAlt)
            .border(2.dp, colors.ink),
    ) {
        AppTab.entries.forEach { tab ->
            val selected = tab == selectedTab
            Box(
                modifier = Modifier
                    .weight(1f)
                    .clickable(interactionSource = null, indication = null) { onTabSelected(tab) }
                    .padding(vertical = 12.dp),
                contentAlignment = Alignment.Center,
            ) {
                Image(
                    painter = painterResource(if (selected) tab.selectedIcon else tab.icon),
                    contentDescription = tab.label,
                    colorFilter = ColorFilter.tint(if (selected) colors.ink else colors.inkMuted),
                    modifier = Modifier.size(20.dp),
                )
            }
        }
    }
}
