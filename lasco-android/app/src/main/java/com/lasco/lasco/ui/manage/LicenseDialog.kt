package com.lasco.lasco.ui.manage

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel

/**
 * Minimal port of Swift's LicenseView. Core/UI dependency license text
 * generation is a Swift-specific tool (cargo about, vendored license files),
 * out of scope here, so those two rows just say so.
 */
@Composable
fun LicenseDialog(onDismiss: () -> Unit) {
    val colors = LascoTheme.colors
    Dialog(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier.fillMaxWidth().background(colors.bg).lascoPanel().padding(24.dp),
        ) {
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(text = "Licenses", style = LascoTheme.type.categoryLarge(), color = colors.ink)
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "✕",
                    style = LascoTheme.type.body(18),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { onDismiss() },
                )
            }
            Spacer(modifier = Modifier.height(20.dp))
            Text(text = "Lasco — GNU GPLv3", style = LascoTheme.type.body(), color = colors.ink)
            Spacer(modifier = Modifier.height(16.dp))
            Text(text = "Core Dependencies", style = LascoTheme.type.body(), color = colors.inkMuted)
            Text(
                text = "Not yet available on Android.",
                style = LascoTheme.type.body(13),
                color = colors.inkMuted,
            )
            Spacer(modifier = Modifier.height(16.dp))
            Text(text = "User Interface Dependencies", style = LascoTheme.type.body(), color = colors.inkMuted)
            Text(
                text = "Not yet available on Android.",
                style = LascoTheme.type.body(13),
                color = colors.inkMuted,
            )
        }
    }
}
