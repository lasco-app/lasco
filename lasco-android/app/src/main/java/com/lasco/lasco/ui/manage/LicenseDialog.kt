package com.lasco.lasco.ui.manage

import android.webkit.WebView
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel

/**
 * Ported from Swift's LicenseView. Each dependency row opens an HTML page
 * bundled in assets, the same way the Swift app renders them in a WKWebView.
 *
 * The three pages come from different generators, see CLAUDE.md:
 *   third-party-licenses.html  cargo about, the Rust crate graph
 *   open_source_licenses.html  the jaredsburrows Gradle plugin, the Maven graph
 *   ui-dependencies.html       hand written, the vendored fonts and icons
 */
@Composable
fun LicenseDialog(onDismiss: () -> Unit) {
    val colors = LascoTheme.colors
    var page by remember { mutableStateOf<LicensePage?>(null) }

    Dialog(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier.fillMaxWidth().lascoPanel().padding(24.dp),
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

            Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp)) {
                Text(text = "Lasco", style = LascoTheme.type.body(), color = colors.inkSub)
                Spacer(modifier = Modifier.weight(1f))
                Text(text = "GNU GPLv3", style = LascoTheme.type.mono(), color = colors.inkMuted)
            }
            LicensePage.entries.forEach { entry ->
                LicenseRow(label = entry.title, onClick = { page = entry })
            }
        }
    }

    page?.let { open ->
        LicensePageDialog(page = open, onDismiss = { page = null })
    }
}

private enum class LicensePage(val title: String, val asset: String) {
    UserInterface("User Interface Dependencies", "ui-dependencies.html"),
    Library("Library Dependencies", "open_source_licenses.html"),
    Core("Core Dependencies", "third-party-licenses.html"),
}

@Composable
private fun LicenseRow(label: String, onClick: () -> Unit) {
    val colors = LascoTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(indication = null, interactionSource = null) { onClick() }
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Text(text = label, style = LascoTheme.type.body(), color = colors.inkSub)
        Spacer(modifier = Modifier.weight(1f))
        Text(text = "→", style = LascoTheme.type.mono(), color = colors.inkMuted)
    }
}

@Composable
private fun LicensePageDialog(page: LicensePage, onDismiss: () -> Unit) {
    val colors = LascoTheme.colors
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(0.9f)
                .padding(16.dp)

                .lascoPanel()
                .padding(16.dp),
        ) {
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(text = page.title, style = LascoTheme.type.body(16), color = colors.ink)
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "✕",
                    style = LascoTheme.type.body(18),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { onDismiss() },
                )
            }
            Spacer(modifier = Modifier.height(12.dp))
            AndroidView(
                modifier = Modifier.fillMaxWidth().weight(1f),
                factory = { context ->
                    WebView(context).apply {
                        setBackgroundColor(android.graphics.Color.rgb(17, 19, 21))
                        val appearance = """
                            <style>
                              :root { color-scheme: dark !important; }
                              html, body { background: #111315 !important; color: #f1f0eb !important; font-family: sans-serif !important; }
                              a { color: #f4b8d5 !important; }
                              pre, blockquote, .license-text { background: #191c1f !important; color: #b2b8b9 !important; border-radius: 16px; }
                            </style>
                        """.trimIndent()
                        val report = context.assets.open(page.asset).bufferedReader().use { it.readText() }
                        loadDataWithBaseURL(
                            "file:///android_asset/",
                            report.replace("</head>", "$appearance</head>"),
                            "text/html", "UTF-8", null,
                        )
                    }
                },
            )
        }
    }
}
