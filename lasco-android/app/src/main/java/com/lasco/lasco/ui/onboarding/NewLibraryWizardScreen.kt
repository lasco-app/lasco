package com.lasco.lasco.ui.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.ui.components.ErrorBanner
import com.lasco.lasco.ui.components.LascoField
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.components.LascoSecondaryButton
import com.lasco.lasco.ui.manage.AddLocalFSRemoteDialog
import com.lasco.lasco.ui.manage.AddS3RemoteDialog
import com.lasco.lasco.ui.manage.ManageViewModel
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel

private const val TOTAL_STEPS = 7

/**
 * Multi step new library flow, ported from the Swift NewLibraryWizard (iOS
 * branch, 7 steps). Steps 3-6 are stubs since automatic photo import isn't
 * available on Android yet, they exist so the step count and dot indicator
 * match the iOS wizard, and so onboardingStep persistence has somewhere
 * meaningful to resume into.
 */
@Composable
fun NewLibraryWizardScreen(
    initialStep: Int,
    resumeLibraryId: String?,
    resumeNickname: String?,
    onBack: () -> Unit,
    onComplete: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: NewLibraryWizardViewModel = viewModel(
        factory = NewLibraryWizardViewModel.factory(resumeLibraryId, resumeNickname),
    ),
) {
    val colors = LascoTheme.colors
    val state by viewModel.uiState.collectAsStateWithLifecycle()

    var step by remember { mutableStateOf(initialStep) }
    var slideForward by remember { mutableStateOf(true) }

    LaunchedEffect(step) {
        if (step > 0) viewModel.recordStep(step)
    }

    LaunchedEffect(state.libraryId) {
        if (step == 0 && state.libraryId != null) {
            slideForward = true
            step = 1
        }
    }

    fun goTo(next: Int) {
        slideForward = next > step
        step = next
    }

    fun back() {
        if (step == 0) {
            onBack()
        } else {
            goTo(step - 1)
        }
    }

    fun finish() {
        viewModel.finish()
        onComplete()
    }

    Column(modifier = modifier.fillMaxSize().background(colors.bg)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 32.dp, bottom = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = "← Back",
                style = LascoTheme.type.body(14),
                color = colors.inkMuted,
                modifier = Modifier.clickable(interactionSource = null, indication = null) { back() },
            )
            Spacer(modifier = Modifier.weight(1f))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (i in 0 until TOTAL_STEPS) {
                    Box(
                        modifier = Modifier
                            .width(if (i == step) 20.dp else 8.dp)
                            .height(3.dp)
                            .background(if (i == step) colors.ink else colors.inkMuted.copy(alpha = 0.35f)),
                    )
                }
            }
        }

        Text(
            text = "LASCO",
            style = LascoTheme.type.categoryLarge(28),
            color = colors.ink,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(bottom = 8.dp),
        )

        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            AnimatedContent(
                targetState = step,
                transitionSpec = {
                    if (slideForward) {
                        slideInHorizontally(tween(300)) { it } togetherWith slideOutHorizontally(tween(300)) { -it }
                    } else {
                        slideInHorizontally(tween(300)) { -it } togetherWith slideOutHorizontally(tween(300)) { it }
                    }
                },
                label = "wizard-step",
            ) { s ->
                when (s) {
                    0 -> CreateStep(loading = state.loading, error = state.error, onCreate = viewModel::create)
                    1 -> MasterKeyStep(masterKeyHex = state.masterKeyHex)
                    2 -> RemoteStep(onAdvance = { goTo(3) })
                    3 -> StubStep(
                        title = "Import your device photos?",
                        body = "Automatic photo import isn't available on Android yet.",
                    )
                    4 -> StubStep(
                        title = "Access your photos.",
                        body = "Automatic photo import isn't available on Android yet.",
                    )
                    5 -> StubStep(
                        title = "Import your photo library?",
                        body = "Automatic photo import isn't available on Android yet.",
                    )
                    else -> StubStep(
                        title = "Automatically import new photos?",
                        body = "Automatic photo import isn't available on Android yet.",
                    )
                }
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 20.dp, bottom = 48.dp),
        ) {
            when (step) {
                1 -> LascoPrimaryButton(
                    text = "I've saved my key",
                    onClick = {
                        viewModel.clearMasterKey()
                        goTo(2)
                    },
                )
                in 3..5 -> LascoPrimaryButton(text = "Continue", onClick = { goTo(step + 1) })
                6 -> LascoPrimaryButton(text = "Get started", onClick = { finish() })
                else -> {}
            }
        }
    }
}

@Composable
private fun CreateStep(
    loading: Boolean,
    error: String?,
    onCreate: (name: String, username: String, password: String) -> Unit,
) {
    val colors = LascoTheme.colors
    var name by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var confirmPassword by remember { mutableStateOf("") }

    val canCreate = name.isNotEmpty() && username.isNotEmpty() &&
        password.length >= 5 && password == confirmPassword && !loading

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 32.dp)
            .padding(top = 40.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Text(text = "Create your library.", style = LascoTheme.type.title(26), color = colors.ink)
        Text(
            text = "Your library is encrypted locally. Choose a strong password.",
            style = LascoTheme.type.body(16),
            color = colors.inkSub,
        )

        LascoField("Library name", name, { name = it }, placeholder = "My Photos")
        LascoField("Username", username, { username = it })
        LascoField("Password", password, { password = it }, secure = true)
        if (password.isNotEmpty() && password.length < 5) {
            Text(text = "Password must be at least 5 characters.", style = LascoTheme.type.body(14), color = colors.ink)
        }
        LascoField("Confirm password", confirmPassword, { confirmPassword = it }, secure = true)
        if (confirmPassword.isNotEmpty() && confirmPassword != password) {
            Text(text = "Passwords do not match.", style = LascoTheme.type.body(14), color = colors.ink)
        }

        error?.let { ErrorBanner(it) }

        LascoPrimaryButton(
            text = if (loading) "Creating…" else "Create Library",
            onClick = { onCreate(name, username, password) },
            enabled = canCreate,
        )
    }
}

@Composable
private fun MasterKeyStep(masterKeyHex: String?) {
    val colors = LascoTheme.colors
    val clipboard = LocalClipboardManager.current
    var copied by remember { mutableStateOf(false) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 32.dp)
            .padding(top = 40.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Text(text = "Save your master key.", style = LascoTheme.type.title(26), color = colors.ink)
        Text(
            text = "This key can restore your library if you forget your password. Store it somewhere safe.",
            style = LascoTheme.type.body(16),
            color = colors.inkSub,
        )

        if (masterKeyHex != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .lascoPanel()
                    .padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = masterKeyHex,
                    style = LascoTheme.type.mono(13),
                    color = colors.inkSub,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    text = "Copy",
                    style = LascoTheme.type.body(13),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable {
                        clipboard.setText(AnnotatedString(masterKeyHex))
                        copied = true
                    },
                )
            }
            if (copied) {
                Text(text = "Master key copied", style = LascoTheme.type.body(13), color = colors.inkSub)
            }
        }
    }
}

@Composable
private fun RemoteStep(onAdvance: () -> Unit) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val prefs = remember { Prefs.from(context) }
    val expertMode by prefs.expertMode.collectAsStateWithLifecycle()
    val manageViewModel: ManageViewModel = viewModel(factory = ManageViewModel.Factory)
    val session by manageViewModel.sessionState.collectAsStateWithLifecycle()

    var showAddS3 by remember { mutableStateOf(false) }
    var showAddLocalFS by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(text = "Add your first remote.", style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "Connect a destination to store your photos.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )
            Text(
                text = "You can add another remote later.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            LascoPrimaryButton(text = "Add S3-compatible remote", onClick = { showAddS3 = true })
            if (expertMode) {
                LascoSecondaryButton(text = "Add local filesystem remote", onClick = { showAddLocalFS = true })
            }
            LascoSecondaryButton(text = "Skip for now", onClick = onAdvance)
        }
    }

    if (showAddS3) {
        AddS3RemoteDialog(
            onDismiss = { showAddS3 = false },
            onResult = { _, _ -> onAdvance() },
        )
    }
    if (showAddLocalFS) {
        AddLocalFSRemoteDialog(
            onDismiss = { showAddLocalFS = false },
            onResult = { _, _ -> onAdvance() },
        )
    }
}

@Composable
private fun StubStep(title: String, body: String) {
    val colors = LascoTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 32.dp)
            .padding(top = 40.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Text(text = title, style = LascoTheme.type.title(26), color = colors.ink)
        Text(text = body, style = LascoTheme.type.body(16), color = colors.inkSub)
    }
}
