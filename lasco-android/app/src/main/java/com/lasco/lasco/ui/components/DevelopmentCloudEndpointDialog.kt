package com.lasco.lasco.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.foundation.clickable
import androidx.compose.ui.unit.dp
import com.lasco.lasco.data.DevelopmentCloudEndpoint
import com.lasco.lasco.ui.theme.LascoTheme

/** Blocks a debug build at launch until its local Lasco Cloud server is selected. */
@Composable
fun DevelopmentCloudEndpointDialog(onConfirm: (String) -> Unit) {
    val colors = LascoTheme.colors
    var endpoint by remember { mutableStateOf(DevelopmentCloudEndpoint.defaultUrl) }

    LascoDialogShell(onDismiss = {}) {
        Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Text(text = "Development server", style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Text(
                text = "Enter the Lasco Cloud address for this development build.",
                style = LascoTheme.type.body(14),
                color = colors.inkMuted,
            )
            LascoField(
                label = "Address and port",
                value = endpoint,
                onValueChange = { endpoint = it },
                placeholder = DevelopmentCloudEndpoint.defaultUrl,
            )
            Row(modifier = Modifier.fillMaxWidth()) {
                Spacer(modifier = Modifier.weight(1f))
                val trimmed = endpoint.trim()
                Text(
                    text = "Use address",
                    style = LascoTheme.type.body(),
                    color = if (trimmed.isEmpty()) colors.inkMuted else colors.ink,
                    modifier = Modifier.clickable(enabled = trimmed.isNotEmpty()) { onConfirm(trimmed) },
                )
            }
        }
    }
}
