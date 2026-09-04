package com.lasco.lasco.ui.manage

import android.content.Intent
import android.net.Uri
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.ui.components.LascoToggle
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel

private const val PRIVACY_POLICY_URL = "https://getlasco.app/privacy-policy"
private const val TERMS_OF_SERVICE_URL = "https://getlasco.app/terms-of-service"

/**
 * Ported from Swift's SettingsView, minus macOS storage-location and
 * log-sharing rows (no Android logging equivalent exists yet).
 */
@Composable
fun SettingsDialog(onDismiss: () -> Unit) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val prefs = remember { Prefs.from(context) }
    val expertMode by prefs.expertMode.collectAsStateWithLifecycle()
    var showLicense by remember { mutableStateOf(false) }

    val versionName = remember {
        context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: ""
    }

    Dialog(onDismissRequest = onDismiss) {
        Column(modifier = Modifier.fillMaxWidth().background(colors.bg).lascoPanel().padding(24.dp)) {
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(text = "Settings", style = LascoTheme.type.title(), color = colors.ink)
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = "Done",
                    style = LascoTheme.type.body(14),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { onDismiss() },
                )
            }
            Spacer(modifier = Modifier.height(20.dp))

            SettingsRow(label = "Licenses", onClick = { showLicense = true })
            SettingsRow(
                label = "Privacy Policy",
                onClick = {
                    context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(PRIVACY_POLICY_URL)))
                },
            )
            SettingsRow(
                label = "Terms of Service",
                onClick = {
                    context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(TERMS_OF_SERVICE_URL)))
                },
            )
            Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp)) {
                Text(text = "Version", style = LascoTheme.type.body(), color = colors.inkSub)
                Spacer(modifier = Modifier.weight(1f))
                Text(text = versionName, style = LascoTheme.type.mono(), color = colors.inkMuted)
            }
            Row(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp)) {
                Text(text = "Expert mode", style = LascoTheme.type.body(), color = colors.inkSub)
                Spacer(modifier = Modifier.weight(1f))
                LascoToggle(checked = expertMode, onCheckedChange = { prefs.setExpertMode(it) })
            }
        }
    }

    if (showLicense) {
        LicenseDialog(onDismiss = { showLicense = false })
    }
}

@Composable
private fun SettingsRow(label: String, onClick: () -> Unit) {
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
