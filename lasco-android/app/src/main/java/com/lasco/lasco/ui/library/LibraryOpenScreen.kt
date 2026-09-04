package com.lasco.lasco.ui.library

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.ui.components.ErrorBanner
import com.lasco.lasco.ui.components.LascoField
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.components.LascoConfirmDialog
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * Password prompt for opening an existing library. The library-list flow has
 * already tried its cached session, so this screen is composed only for a
 * genuine cache miss.
 */
@Composable
fun LibraryOpenScreen(
    request: LibraryOpenRequest,
    onBack: () -> Unit,
    onOpened: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: LibraryOpenViewModel = viewModel(key = request.attemptId, factory = LibraryOpenViewModel.Factory),
) {
    val colors = LascoTheme.colors
    val state by viewModel.uiState.collectAsStateWithLifecycle()

    LaunchedEffect(state.opened) {
        if (state.opened) onOpened()
    }

    if (state.opened) {
        Box(modifier = modifier.fillMaxSize().background(colors.bg), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(color = colors.ink)
        }
        return
    }

    var username by remember(request.attemptId) { mutableStateOf(request.username ?: "") }
    var password by remember { mutableStateOf("") }
    var showRecoveryConfirm by remember { mutableStateOf(false) }

    val canSubmit = username.isNotEmpty() && password.isNotEmpty() && !state.loading

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg),
    ) {
        Text(
            text = "← Back",
            style = LascoTheme.type.body(14),
            color = colors.inkMuted,
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 32.dp, bottom = 16.dp)
                .clickable(interactionSource = null, indication = null) { onBack() },
        )

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(text = request.nickname, style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "Enter credentials to open",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )

            LascoField("Username", username, { username = it }, enabled = request.username == null)
            LascoField("Password", password, { password = it }, secure = true)

            state.error?.let { ErrorBanner(it) }
            if (state.recoveryAvailable) {
                Text(
                    text = "Recover from operation log",
                    style = LascoTheme.type.body(14),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable { showRecoveryConfirm = true },
                )
            }
        }

        Column(
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 20.dp, bottom = 48.dp),
        ) {
            LascoPrimaryButton(
                text = if (state.loading) "Opening…" else "Open Library",
                onClick = { viewModel.open(request.nickname, username, password) },
                enabled = canSubmit,
            )
        }
    }

    if (showRecoveryConfirm) {
        LascoConfirmDialog(
            title = "Recover library state?",
            message = "This rebuilds the local library state from its encrypted operation log. Your photos and remote storage are not changed.",
            confirmLabel = "Recover",
            onConfirm = {
                showRecoveryConfirm = false
                viewModel.recover(request.nickname, username, password)
            },
            onCancel = { showRecoveryConfirm = false },
        )
    }
}
