package com.lasco.lasco.ui.manage

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.ui.components.AlbumPickerDialog
import com.lasco.lasco.ui.components.LascoConfirmDialog
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import kotlinx.coroutines.launch

private const val PRIVACY_POLICY_URL = "https://getlasco.app/privacy-policy"

private sealed interface ManageDestination {
    data object Root : ManageDestination
    data object Remotes : ManageDestination
    data object Users : ManageDestination
    data object Operations : ManageDestination
}

/**
 * Ported from Swift's ManageView. Skips log-file sharing (no Android logging
 * equivalent exists). The Operations row mirrors Swift, only shown when
 * expert mode is on.
 */
@Composable
fun ManageScreen(modifier: Modifier = Modifier, onSignedOut: () -> Unit = {}) {
    var destination by remember { mutableStateOf<ManageDestination>(ManageDestination.Root) }

    when (destination) {
        ManageDestination.Root -> ManageRootScreen(
            modifier = modifier,
            onOpenRemotes = { destination = ManageDestination.Remotes },
            onOpenUsers = { destination = ManageDestination.Users },
            onOpenOperations = { destination = ManageDestination.Operations },
            onSignedOut = onSignedOut,
        )
        ManageDestination.Remotes -> RemotesScreen(
            modifier = modifier,
            onBack = { destination = ManageDestination.Root },
        )
        ManageDestination.Users -> UsersScreen(
            modifier = modifier,
            onBack = { destination = ManageDestination.Root },
        )
        ManageDestination.Operations -> OperationsScreen(
            modifier = modifier,
            onBack = { destination = ManageDestination.Root },
        )
    }
}

@Composable
private fun ManageRootScreen(
    modifier: Modifier,
    onOpenRemotes: () -> Unit,
    onOpenUsers: () -> Unit,
    onOpenOperations: () -> Unit,
    onSignedOut: () -> Unit,
) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val viewModel: ManageViewModel = viewModel(factory = ManageViewModel.Factory)
    val session by viewModel.sessionState.collectAsStateWithLifecycle()
    val albums by viewModel.albums.collectAsStateWithLifecycle()
    val expertMode by viewModel.prefs.expertMode.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()

    var showSettings by remember { mutableStateOf(false) }
    var showAlbumPicker by remember { mutableStateOf(false) }
    var showLicenses by remember { mutableStateOf(false) }
    var confirmSignOut by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }

    val defaultAlbumName = albums.firstOrNull { it.albumId == session.defaultUploadAlbumId }?.name ?: "None"

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg)
            .padding(horizontal = 16.dp),
    ) {
        Column(modifier = Modifier.padding(top = 20.dp, bottom = 16.dp)) {
            Text(text = "MANAGE", style = LascoTheme.type.categoryLarge(), color = colors.ink)
            Text(text = session.nickname, style = LascoTheme.type.subtitle(), color = colors.inkMuted)
        }

        Column(modifier = Modifier.fillMaxWidth().lascoPanel()) {
            ManageRow(label = "Remotes", onClick = onOpenRemotes)
            HorizontalDivider(color = colors.ink, thickness = 1.dp)
            ManageRow(label = "Users", onClick = onOpenUsers)
            HorizontalDivider(color = colors.ink, thickness = 1.dp)
            ManageRow(label = "Global settings", onClick = { showSettings = true })
            HorizontalDivider(color = colors.ink, thickness = 1.dp)
            ManageRow(
                label = "Sign out",
                onClick = { confirmSignOut = true },
                labelColor = colors.error,
            )
        }

        Spacer(modifier = Modifier.height(16.dp))

        Column(modifier = Modifier.fillMaxWidth().lascoPanel()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { showAlbumPicker = true }
                    .padding(horizontal = 16.dp, vertical = 14.dp),
            ) {
                Text(text = "Default import album", style = LascoTheme.type.body(), color = colors.inkSub)
                Spacer(modifier = Modifier.weight(1f))
                Text(text = defaultAlbumName, style = LascoTheme.type.mono(), color = colors.inkMuted)
            }
        }

        if (expertMode) {
            Spacer(modifier = Modifier.height(16.dp))
            Column(modifier = Modifier.fillMaxWidth().lascoPanel()) {
                ManageRow(label = "Operations", onClick = onOpenOperations)
            }
        }

        Spacer(modifier = Modifier.height(16.dp))

        Column(modifier = Modifier.fillMaxWidth().lascoPanel()) {
            ManageRow(label = "Licenses", onClick = { showLicenses = true })
            HorizontalDivider(color = colors.ink, thickness = 1.dp)
            ManageRow(
                label = "Privacy Policy",
                onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(PRIVACY_POLICY_URL))) },
            )
        }

        Spacer(modifier = Modifier.height(16.dp))

        Column(modifier = Modifier.fillMaxWidth().lascoPanel()) {
            ManageRow(
                label = "Delete library",
                onClick = { confirmDelete = true },
                labelColor = colors.error,
            )
        }

        Spacer(modifier = Modifier.height(100.dp))
    }

    if (showSettings) {
        SettingsDialog(onDismiss = { showSettings = false })
    }
    if (showAlbumPicker) {
        AlbumPickerDialog(
            title = "Default import album",
            albums = albums,
            onSelect = {
                viewModel.setDefaultUploadAlbum(it.albumId)
                showAlbumPicker = false
            },
            onCancel = { showAlbumPicker = false },
        )
    }
    if (showLicenses) {
        LicenseDialog(onDismiss = { showLicenses = false })
    }
    if (confirmSignOut) {
        LascoConfirmDialog(
            title = "Sign out",
            message = "You can sign back in with your username and password.",
            confirmLabel = "Sign out",
            onConfirm = {
                confirmSignOut = false
                scope.launch {
                    viewModel.signOut()
                    onSignedOut()
                }
            },
            onCancel = { confirmSignOut = false },
        )
    }
    if (confirmDelete) {
        LascoConfirmDialog(
            title = "Delete library",
            message = "This removes all local data and unregisters the library from this device. Remote storage is not touched.",
            confirmLabel = "Delete",
            onConfirm = {
                confirmDelete = false
                scope.launch {
                    viewModel.deleteLibrary()
                    onSignedOut()
                }
            },
            onCancel = { confirmDelete = false },
        )
    }
}

@Composable
private fun ManageRow(label: String, onClick: () -> Unit, labelColor: Color? = null) {
    val colors = LascoTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onClick() }
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Text(text = label, style = LascoTheme.type.body(), color = labelColor ?: colors.inkSub)
        Spacer(modifier = Modifier.weight(1f))
        Text(text = "→", style = LascoTheme.type.mono(), color = colors.inkMuted)
    }
}
