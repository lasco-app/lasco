package app.lasco.importer

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.FrameWindowScope
import androidx.compose.ui.window.WindowPlacement
import androidx.compose.ui.window.WindowState

private val isMacOS = System.getProperty("os.name").startsWith("Mac", ignoreCase = true)

/** AWT reads the native appearance when it starts, before any Compose windows are created. */
internal fun configureImporterAppearance() {
    if (isMacOS) {
        System.setProperty("apple.awt.application.appearance", "NSAppearanceNameDarkAqua")
    }
}

/** Paint beneath the native macOS title bar, retaining its controls and window gestures. */
@Composable
internal fun FrameWindowScope.ImporterWindowFrame(
    state: WindowState,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val background = MaterialTheme.colorScheme.background
    SideEffect {
        // Keep uncovered areas dark during live resizing as well as initial rendering.
        window.background = java.awt.Color(background.toArgb(), true)
        if (isMacOS) {
            window.rootPane.putClientProperty("apple.awt.fullWindowContent", true)
            window.rootPane.putClientProperty("apple.awt.transparentTitleBar", true)
            window.rootPane.putClientProperty("apple.awt.windowTitleVisible", false)
        }
    }

    Column(modifier.fillMaxSize().background(background)) {
        if (isMacOS && state.placement != WindowPlacement.Fullscreen) {
            // Full-window content removes AWT's top inset. Reserve the native title/control area.
            Spacer(Modifier.height(32.dp))
        }
        Box(Modifier.weight(1f).fillMaxSize()) { content() }
    }
}
