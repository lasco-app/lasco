package com.lasco.lasco.ui.library

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import com.lasco.lasco.ui.theme.lascoPanelHard
import uniffi.lasco_ffi.FfiLibraryEntry

/**
 * Library list screen, ported from the Swift LibraryListView. Same layout, a
 * LASCO title, the list of libraries as flat panels, and the two entry points
 * into the new and existing library flows at the bottom.
 */
@Composable
fun LibraryListScreen(
    onNewLibrary: () -> Unit,
    onAddExisting: () -> Unit,
    onOpenLibrary: (String) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: LibraryListViewModel = viewModel(factory = LibraryListViewModel.Factory),
) {
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    val colors = LascoTheme.colors

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg),
    ) {
        // Header.
        Column(
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 48.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(text = "LASCO", style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Text(
                text = "Your libraries",
                style = LascoTheme.type.subtitle(),
                color = colors.inkMuted,
            )
        }

        // List.
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 32.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            when {
                state.loading -> CircularProgressIndicator(color = colors.ink)
                state.error != null -> ErrorPanel(title = "Could not load libraries", detail = state.error!!)
                state.libraries.isEmpty() -> Text(
                    text = "No libraries yet.",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                    modifier = Modifier
                        .fillMaxWidth()
                        .lascoPanel()
                        .padding(horizontal = 16.dp, vertical = 20.dp),
                )
                else -> state.libraries.forEach { entry ->
                    LibraryRow(entry, onClick = { if (entry.loadError == null) onOpenLibrary(entry.id) })
                }
            }
        }

        // Bottom actions.
        Column(
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(bottom = 48.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            LascoPrimaryButton(text = "New library", onClick = onNewLibrary)
            Text(
                text = "Add existing library",
                style = LascoTheme.type.body(15),
                color = colors.inkMuted,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(interactionSource = null, indication = null) { onAddExisting() },
            )
        }
    }
}

@Composable
private fun LibraryRow(entry: FfiLibraryEntry, onClick: () -> Unit) {
    val colors = LascoTheme.colors
    val loadError = entry.loadError
    if (loadError != null) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .lascoPanel()
                .padding(horizontal = 16.dp, vertical = 14.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = if (entry.id.isEmpty()) "Unknown library" else entry.nickname,
                style = LascoTheme.type.body(),
                color = colors.ink,
            )
            Text(text = loadError, style = LascoTheme.type.mono(), color = colors.inkMuted)
        }
    } else {
        Text(
            text = entry.nickname,
            style = LascoTheme.type.body(),
            color = colors.inkSub,
            modifier = Modifier
                .fillMaxWidth()
                .lascoPanelHard()
                .clickable(interactionSource = null, indication = null) { onClick() }
                .padding(horizontal = 16.dp, vertical = 14.dp),
        )
    }
}

@Composable
private fun ErrorPanel(title: String, detail: String) {
    val colors = LascoTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .lascoPanel()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(text = title, style = LascoTheme.type.body(), color = colors.ink)
        Text(text = detail, style = LascoTheme.type.mono(), color = colors.inkMuted)
    }
}
