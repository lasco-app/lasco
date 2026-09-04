package com.lasco.lasco.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import com.lasco.lasco.ui.theme.LascoTheme

private const val ENDPOINT_INPUT_PREFIX = "https://"
private const val LASCO_CLOUD_URL = "https://cloud.getlasco.app"

/** Blocks a debug build at launch until its local Lasco Cloud server is selected. */
@Composable
fun DevelopmentCloudEndpointDialog(onConfirm: (String) -> Unit) {
    val colors = LascoTheme.colors
    var endpoint by remember { mutableStateOf(ENDPOINT_INPUT_PREFIX) }

    LascoDialogShell(onDismiss = {}) {
        Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Text(text = "Development server", style = LascoTheme.type.categoryLarge(), color = colors.ink)
            LascoField(
                label = "Address and port",
                value = endpoint,
                onValueChange = { endpoint = it },
                placeholder = ENDPOINT_INPUT_PREFIX,
                autoFocus = true,
            )
            LascoSecondaryButton(
                text = "Use Lasco Cloud",
                onClick = { endpoint = LASCO_CLOUD_URL },
            )
            val trimmed = endpoint.trim()
            LascoPrimaryButton(
                text = "Use address",
                onClick = { onConfirm(trimmed) },
                enabled = trimmed.isNotEmpty() && trimmed != ENDPOINT_INPUT_PREFIX,
            )
        }
    }
}
