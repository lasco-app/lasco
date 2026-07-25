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
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * Password prompt for opening an existing library, ported from the Swift
 * LibraryOpenSheet. Tries the cached session first (openCached), only shows
 * the form when that comes back empty, exactly like LibraryListView does.
 * Takes the entry id rather than an FfiLibraryEntry, since Nav3 keys must
 * not carry FFI structs, and shows a loading state until the view model
 * resolves it back to the full entry.
 */
@Composable
fun LibraryOpenScreen(
    entryId: String,
    onBack: () -> Unit,
    onOpened: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: LibraryOpenViewModel = viewModel(key = entryId, factory = LibraryOpenViewModel.factory(entryId)),
) {
    val colors = LascoTheme.colors
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    val resolvedEntry by viewModel.entry.collectAsStateWithLifecycle()

    LaunchedEffect(state.opened) {
        if (state.opened) onOpened()
    }

    if (resolvedEntry == null) {
        Box(modifier = modifier.fillMaxSize().background(colors.bg), contentAlignment = Alignment.Center) {
            if (state.error != null) {
                ErrorBanner(state.error!!)
            } else {
                CircularProgressIndicator(color = colors.ink)
            }
        }
        return
    }
    val entry = resolvedEntry!!

    var username by remember(entry.id) { mutableStateOf(entry.username ?: "") }
    var password by remember { mutableStateOf("") }

    LaunchedEffect(entry.id) {
        viewModel.tryOpenCached(entry.nickname, entry.username)
    }

    if (state.checkingCache) {
        Box(modifier = modifier.fillMaxSize().background(colors.bg), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(color = colors.ink)
        }
        return
    }

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
            Text(text = entry.nickname, style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "Enter credentials to open",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )

            LascoField("Username", username, { username = it }, enabled = entry.username == null)
            LascoField("Password", password, { password = it }, secure = true)

            state.error?.let { ErrorBanner(it) }
        }

        Column(
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 20.dp, bottom = 48.dp),
        ) {
            LascoPrimaryButton(
                text = if (state.loading) "Opening…" else "Open Library",
                onClick = { viewModel.open(entry.nickname, username, password) },
                enabled = canSubmit,
            )
        }
    }
}
