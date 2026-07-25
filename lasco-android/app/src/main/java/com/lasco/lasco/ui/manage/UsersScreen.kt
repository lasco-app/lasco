package com.lasco.lasco.ui.manage

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.compose.material3.HorizontalDivider
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.components.ErrorBanner
import com.lasco.lasco.ui.components.LascoField
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import kotlinx.coroutines.launch

/**
 * Ported from Swift's UsersView. No removal action exists in the FFI, so this
 * screen is add only, matching the Swift UI.
 */
@Composable
fun UsersScreen(onBack: () -> Unit, modifier: Modifier = Modifier) {
    val colors = LascoTheme.colors
    val manageViewModel: ManageViewModel = viewModel(factory = ManageViewModel.Factory)
    val session by manageViewModel.sessionState.collectAsStateWithLifecycle()
    var showAddUser by remember { mutableStateOf(false) }

    Column(modifier = modifier.fillMaxSize().background(colors.bg).padding(horizontal = 16.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(top = 20.dp, bottom = 12.dp),
        ) {
            Text(
                text = "← Manage",
                style = LascoTheme.type.body(),
                color = colors.inkMuted,
                modifier = Modifier.clickable { onBack() },
            )
        }
        Text(text = "USERS", style = LascoTheme.type.categoryLarge(), color = colors.ink)
        Spacer(modifier = Modifier.height(16.dp))

        Column(modifier = Modifier.fillMaxWidth().lascoPanel()) {
            session.users.forEachIndexed { index, user ->
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    Text(text = user, style = LascoTheme.type.body(), color = colors.ink)
                    if (user == session.username) {
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = "you",
                            style = LascoTheme.type.mono(10),
                            color = colors.pink,
                            modifier = Modifier
                                .background(colors.pink.copy(alpha = 0.12f))
                                .padding(horizontal = 6.dp, vertical = 2.dp),
                        )
                    }
                }
                if (index != session.users.lastIndex) {
                    HorizontalDivider(color = colors.ink, thickness = 1.dp)
                }
            }
        }

        Spacer(modifier = Modifier.height(16.dp))
        LascoPrimaryButton(text = "Add user", onClick = { showAddUser = true })
    }

    if (showAddUser) {
        AddUserDialog(onDismiss = { showAddUser = false })
    }
}

@Composable
private fun AddUserDialog(onDismiss: () -> Unit) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val repo = remember { LibraryRepository.from(context) }
    val scope = rememberCoroutineScope()

    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var confirmPassword by remember { mutableStateOf("") }
    var submitting by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    val isValid = username.isNotBlank() && password.isNotBlank() &&
        password == confirmPassword && !submitting

    Dialog(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier.fillMaxWidth().background(colors.bg).lascoPanel().padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(text = "Add user", style = LascoTheme.type.title(), color = colors.ink)
            LascoField(label = "Username", value = username, onValueChange = { username = it })
            LascoField(label = "Password", value = password, onValueChange = { password = it }, secure = true)
            LascoField(
                label = "Confirm password",
                value = confirmPassword,
                onValueChange = { confirmPassword = it },
                secure = true,
            )
            error?.let { ErrorBanner(message = it) }
            LascoPrimaryButton(
                text = "Add user",
                enabled = isValid,
                onClick = {
                    submitting = true
                    error = null
                    scope.launch {
                        try {
                            repo.addUser(username, password)
                            onDismiss()
                        } catch (e: Exception) {
                            submitting = false
                            error = e.message?.ifBlank { null } ?: "Failed to add user"
                        }
                    }
                },
            )
        }
    }
}
