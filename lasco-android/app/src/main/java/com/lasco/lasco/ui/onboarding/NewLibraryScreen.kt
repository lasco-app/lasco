package com.lasco.lasco.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.ui.components.ErrorBanner
import com.lasco.lasco.ui.components.LascoField
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * First step of the new library flow, ported from the createStep of the Swift
 * NewLibraryWizard. Collects the library details, validates them client side,
 * then calls ffiCreateLibrary followed by FfiLibrary.open through
 * NewLibraryViewModel.
 */
@Composable
fun NewLibraryScreen(
    onBack: () -> Unit,
    onLibraryOpened: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: NewLibraryViewModel = viewModel(factory = NewLibraryViewModel.Factory),
) {
    val colors = LascoTheme.colors
    val state by viewModel.uiState.collectAsStateWithLifecycle()

    var name by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var confirmPassword by remember { mutableStateOf("") }

    LaunchedEffect(state.opened) {
        if (state.opened) onLibraryOpened()
    }

    val canCreate = name.isNotEmpty() && username.isNotEmpty() &&
        password.length >= 5 && password == confirmPassword && !state.loading

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg),
    ) {
        // Top bar with a back affordance.
        Text(
            text = "← Back",
            style = LascoTheme.type.body(14),
            color = colors.inkMuted,
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 32.dp, bottom = 16.dp)
                .clickable(interactionSource = null, indication = null) { onBack() },
        )

        Text(
            text = "LASCO",
            style = LascoTheme.type.categoryLarge(28),
            color = colors.ink,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp, vertical = 8.dp),
        )

        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 32.dp)
                .padding(top = 32.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(
                text = "Create your library.",
                style = LascoTheme.type.title(26),
                color = colors.ink,
            )
            Text(
                text = "Your library is encrypted locally. Choose a strong password.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )

            LascoField("Library name", name, { name = it }, placeholder = "My Photos")
            LascoField("Username", username, { username = it })
            LascoField("Password", password, { password = it }, secure = true)
            if (password.isNotEmpty() && password.length < 5) {
                Text(
                    text = "Password must be at least 5 characters.",
                    style = LascoTheme.type.body(14),
                    color = colors.ink,
                )
            }
            LascoField("Confirm password", confirmPassword, { confirmPassword = it }, secure = true)
            if (confirmPassword.isNotEmpty() && confirmPassword != password) {
                Text(
                    text = "Passwords do not match.",
                    style = LascoTheme.type.body(14),
                    color = colors.ink,
                )
            }

            state.error?.let { ErrorBanner(it) }
        }

        Column(
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 20.dp, bottom = 48.dp),
        ) {
            LascoPrimaryButton(
                text = if (state.loading) "Creating…" else "Create Library",
                onClick = { viewModel.create(name, username, password) },
                enabled = canCreate,
            )
        }
    }
}
