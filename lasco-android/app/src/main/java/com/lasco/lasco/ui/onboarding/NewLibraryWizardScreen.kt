package com.lasco.lasco.ui.onboarding

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
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
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.media.DeviceMediaPermissions
import com.lasco.lasco.ui.components.ErrorBanner
import com.lasco.lasco.ui.components.LascoField
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.components.LascoSecondaryButton
import com.lasco.lasco.ui.manage.AddLocalFSRemoteDialog
import com.lasco.lasco.ui.manage.AddS3RemoteDialog
import com.lasco.lasco.ui.manage.LascoCloudLoginDialog
import com.lasco.lasco.ui.manage.ManageViewModel
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel

private fun requiredMediaPermissions(): Array<String> =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        arrayOf(Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.READ_MEDIA_VIDEO)
    } else {
        arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
    }

// Partial access (READ_MEDIA_VISUAL_USER_SELECTED on API 34+) is treated as
// denied, full access to the camera folder is required.
// Optional, unlike the read permissions. Without it MediaStore hands back
// copies with the GPS EXIF tags stripped, so photos still import, just
// without their location. Below API 29 nothing is redacted and the
// permission does not exist.
private fun hasMediaLocationAccess(context: Context): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.Q ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_MEDIA_LOCATION) ==
        PackageManager.PERMISSION_GRANTED

/**
 * Multi step new library flow, ported from the Swift NewLibraryWizard (iOS
 * branch). Steps 3-6 drive the device media import (ask, media access,
 * photo locations, scan and import), step 7 the auto-import toggle. The two
 * permission steps skip themselves when access is already granted.
 */
@Composable
fun NewLibraryWizardScreen(
    sessionId: String,
    resume: OnboardingResume?,
    onBack: () -> Unit,
    onComplete: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: NewLibraryWizardViewModel = viewModel(factory = NewLibraryWizardViewModel.factory()),
) {
    val colors = LascoTheme.colors
    val state by viewModel.uiState.collectAsStateWithLifecycle()

    LaunchedEffect(sessionId) {
        if (resume == null) viewModel.startFresh(sessionId)
        else viewModel.resume(sessionId, resume.libraryId, resume.nickname, resume.checkpoint)
    }

    fun handleBack() {
        if (viewModel.back() == BackResult.ExitWizard) {
            viewModel.cancel()
            onBack()
        }
    }

    BackHandler(onBack = ::handleBack)
    Column(modifier = modifier.fillMaxSize().background(colors.bg)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 32.dp, bottom = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (state.step == WizardStep.CreateLibrary) {
                Text(
                    text = "← Back",
                    style = LascoTheme.type.body(14),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable(interactionSource = null, indication = null) { handleBack() },
                )
            }
            Spacer(modifier = Modifier.weight(1f))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (i in 0 until 8) {
                    Box(
                        modifier = Modifier
                            .width(if (i == state.step.progressIndex()) 20.dp else 8.dp)
                            .height(3.dp)
                            .background(if (i == state.step.progressIndex()) colors.ink else colors.inkMuted.copy(alpha = 0.35f)),
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
                targetState = state.step,
                transitionSpec = {
                    if (state.slideForward) {
                        slideInHorizontally(tween(300)) { it } togetherWith slideOutHorizontally(tween(300)) { -it }
                    } else {
                        slideInHorizontally(tween(300)) { -it } togetherWith slideOutHorizontally(tween(300)) { it }
                    }
                },
                label = "wizard-step",
            ) { s ->
                when (s) {
                    WizardStep.CreateLibrary -> CreateStep(loading = state.loading, error = state.error, onCreate = viewModel::createLibrary)
                    WizardStep.SaveRecoveryKey -> MasterKeyStep(masterKeyHex = state.masterKeyHex)
                    WizardStep.AddRemote -> RemoteStep(onAdvance = viewModel::remoteCompleted)
                    WizardStep.ChooseDeviceImport -> AskImportStep(
                        hasRemote = state.hasRemote,
                        onImport = viewModel::chooseDeviceImport,
                        onSkip = viewModel::skipDeviceImport,
                        onGetStarted = {
                            viewModel.complete()
                            onComplete()
                        },
                    )
                    WizardStep.GrantMediaAccess -> PermissionStep(
                        autoSkip = state.slideForward,
                        onGranted = viewModel::mediaAccessGranted,
                        onSkip = viewModel::skipDeviceImport,
                    )
                    WizardStep.GrantLocationAccess -> MediaLocationStep(
                        autoSkip = state.slideForward,
                        onDone = viewModel::locationAccessGranted,
                        onSkip = viewModel::skipDeviceImport,
                    )
                    WizardStep.ImportDeviceMedia -> ImportStep(viewModel = viewModel, state = state, onDone = viewModel::importCompleted)
                    WizardStep.ChooseAutoImport -> AutoImportStep(
                        onYes = {
                            viewModel.setAutoImport(true, onComplete)
                        },
                        onNo = {
                            viewModel.setAutoImport(false, onComplete)
                        },
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
            when (state.step) {
                WizardStep.SaveRecoveryKey -> LascoPrimaryButton(
                    text = "I've saved my key",
                    onClick = {
                        viewModel.confirmRecoveryKey()
                    },
                )
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
    var showCloudLogin by remember { mutableStateOf(false) }

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
            LascoPrimaryButton(text = "Authenticate with Lasco Cloud", onClick = { showCloudLogin = true })
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
    if (showCloudLogin) {
        LascoCloudLoginDialog(
            onDismiss = { showCloudLogin = false },
            onResult = { error -> if (error == null) onAdvance() },
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
private fun AskImportStep(
    hasRemote: Boolean,
    onImport: () -> Unit,
    onSkip: () -> Unit,
    onGetStarted: () -> Unit,
) {
    val colors = LascoTheme.colors
    Column(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(
                text = if (hasRemote) "Import your device photos?" else "Can't import your current photo library yet.",
                style = LascoTheme.type.title(26),
                color = colors.ink,
            )
            if (hasRemote) {
                Text(
                    text = "Lasco can import the photos and videos in your camera folder and back them up to your remote. Albums are not replicated, and photos stored outside the camera folder are not imported.",
                    style = LascoTheme.type.body(16),
                    color = colors.inkSub,
                )
                Text(text = "Nothing is deleted from your device.", style = LascoTheme.type.body(16), color = colors.inkSub)
            } else {
                Text(
                    text = "Because there is no remote yet, it would mean that everything should be saved twice locally on your device.",
                    style = LascoTheme.type.body(16),
                    color = colors.inkSub,
                )
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (hasRemote) {
                LascoPrimaryButton(text = "Yes, import my photos", onClick = onImport)
                LascoSecondaryButton(text = "No, not now", onClick = onSkip)
            } else {
                LascoPrimaryButton(text = "Get started", onClick = onGetStarted)
            }
        }
    }
}

@Composable
private fun PermissionStep(autoSkip: Boolean, onGranted: () -> Unit, onSkip: () -> Unit) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    var denied by remember { mutableStateOf(false) }

    // Access can already be granted from an earlier run or an earlier pass
    // through the wizard. Only skipped when moving forward, so stepping back
    // from the next screen does not bounce straight in again.
    val alreadyGranted = remember { autoSkip && DeviceMediaPermissions.canReadFullLibrary(context) }
    LaunchedEffect(alreadyGranted) { if (alreadyGranted) onGranted() }
    if (alreadyGranted) return

    val permissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        if (DeviceMediaPermissions.canReadFullLibrary(context)) {
            denied = false
            onGranted()
        } else {
            denied = true
        }
    }

    // Granting in Settings does not restart the app, so without this the step
    // would sit on its denied state after the user comes back having granted.
    LifecycleResumeEffect(denied) {
        if (denied && DeviceMediaPermissions.canReadFullLibrary(context)) onGranted()
        onPauseOrDispose {}
    }

    Column(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(text = "Access your photos.", style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "We'll ask for permission to access your photos so Lasco can import them.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )
            if (denied) {
                Text(
                    text = "Access was denied, or only partial access was granted. You can grant full access in Settings and come back here.",
                    style = LascoTheme.type.body(16),
                    color = colors.inkSub,
                )
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (denied) {
                LascoPrimaryButton(
                    text = "Open Settings",
                    onClick = {
                        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                            data = Uri.fromParts("package", context.packageName, null)
                        }
                        context.startActivity(intent)
                    },
                )
                LascoSecondaryButton(text = "Skip for now", onClick = onSkip)
            } else {
                LascoPrimaryButton(
                    text = "Continue",
                    onClick = {
                        if (DeviceMediaPermissions.canReadFullLibrary(context)) {
                            onGranted()
                        } else {
                            permissionLauncher.launch(requiredMediaPermissions())
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun MediaLocationStep(autoSkip: Boolean, onDone: () -> Unit, onSkip: () -> Unit) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    var denied by remember { mutableStateOf(false) }

    // Often granted alongside the read permissions, since the system treats
    // them as one group, and it does not exist at all below API 29. Either
    // way there is nothing to ask, so the step advances itself.
    val alreadyGranted = remember { autoSkip && hasMediaLocationAccess(context) }
    LaunchedEffect(alreadyGranted) { if (alreadyGranted) onDone() }
    if (alreadyGranted) return

    // Required, not optional. Only originals are imported, so without it
    // there is nothing to advance to.
    val locationLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) {
            denied = false
            onDone()
        } else {
            denied = true
        }
    }

    // Granting in Settings does not restart the app, so without this the step
    // would sit on its denied state after the user comes back having granted.
    LifecycleResumeEffect(denied) {
        if (denied && hasMediaLocationAccess(context)) onDone()
        onPauseOrDispose {}
    }

    Column(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(text = "Keep photo locations.", style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "Android strips the place a photo was taken out of every copy it hands to an app, unless you allow access to photo locations.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )
            Text(
                text = "Lasco imports originals only, so this is needed to import at all. The location cannot be recovered later.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )
            if (denied) {
                Text(
                    text = "Access to photo locations was denied. You can grant it in Settings and come back here, or skip importing your photos for now.",
                    style = LascoTheme.type.body(16),
                    color = colors.inkSub,
                )
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (denied) {
                LascoPrimaryButton(
                    text = "Open Settings",
                    onClick = {
                        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                            data = Uri.fromParts("package", context.packageName, null)
                        }
                        context.startActivity(intent)
                    },
                )
            } else {
                LascoPrimaryButton(
                    text = "Allow photo locations",
                    onClick = { locationLauncher.launch(Manifest.permission.ACCESS_MEDIA_LOCATION) },
                )
            }
            LascoSecondaryButton(text = "Skip for now", onClick = onSkip)
        }
    }
}

@Composable
private fun ImportStep(
    viewModel: NewLibraryWizardViewModel,
    state: WizardUiState,
    onDone: () -> Unit,
) {
    val colors = LascoTheme.colors
    val context = LocalContext.current

    LaunchedEffect(Unit) {
        if (state.deviceScan == null) viewModel.scanDeviceMedia()
    }

    val importState = state.importState
    val done = importState as? ImportState.Done
    val failure = importState as? ImportState.Error

    Column(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            if (done != null) {
                Text(text = "All done.", style = LascoTheme.type.title(26), color = colors.ink)
                val photoWord = if (done.photos == 1) "photo" else "photos"
                val videoWord = if (done.videos == 1) "video" else "videos"
                Text(
                    text = "${done.photos} $photoWord and ${done.videos} $videoWord were successfully imported.",
                    style = LascoTheme.type.body(16),
                    color = colors.inkSub,
                )
                if (done.failed > 0) {
                    val itemWord = if (done.failed == 1) "item" else "items"
                    Text(
                        text = "${done.failed} $itemWord could not be imported and were left on your device.",
                        style = LascoTheme.type.body(16),
                        color = colors.inkSub,
                    )
                }
            } else {
                Text(text = "Import your photo library?", style = LascoTheme.type.title(26), color = colors.ink)
                Text(
                    text = "Lasco will import your existing photos and videos and back them up to your remote.",
                    style = LascoTheme.type.body(16),
                    color = colors.inkSub,
                )

                if (state.scanning) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        CircularProgressIndicator(color = colors.inkMuted, modifier = Modifier.height(16.dp).width(16.dp))
                        Text(text = "Scanning library…", style = LascoTheme.type.body(14), color = colors.inkMuted)
                    }
                } else {
                    state.deviceScan?.let { scan ->
                        Column(modifier = Modifier.fillMaxWidth().lascoPanel().padding(horizontal = 16.dp, vertical = 12.dp)) {
                            ImportStatRow(label = "Photos", value = scan.photoCount.toString())
                            ImportStatRow(label = "Videos", value = scan.videoCount.toString())
                            if (scan.ignoredCount > 0) {
                                ImportStatRow(label = "Ignored", value = scan.ignoredCount.toString())
                            }
                        }
                    }
                }

                // Blocking, unlike the location note below. The import is
                // refused while any of these are in the camera folder, so the
                // Import Now button is disabled alongside this.
                state.deviceScan?.let { scan ->
                    if (scan.tooLargeCount > 0) {
                        ErrorBanner(message = InitialImportController.tooLargeMessage(scan.tooLargeCount))
                    }
                }

                if (!hasMediaLocationAccess(context)) {
                    Text(
                        text = "Photo locations were not allowed, so imported photos will not carry the place they were taken.",
                        style = LascoTheme.type.body(14),
                        color = colors.inkMuted,
                    )
                }

                if (failure != null) {
                    ErrorBanner(message = failure.message)
                }

                if (importState is ImportState.Importing) {
                    val importing = importState as ImportState.Importing
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(
                            text = "Backed up ${importing.backedUp} of ${importing.total} items",
                            style = LascoTheme.type.mono(13),
                            color = colors.inkMuted,
                        )
                        LinearProgressIndicator(
                            progress = { if (importing.total > 0) importing.backedUp.toFloat() / importing.total else 0f },
                            color = colors.ink,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        InitialImportPhaseProgress(phase = importing.phase)
                    }
                }
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            when {
                done != null -> LascoPrimaryButton(text = "Continue", onClick = onDone)
                importState is ImportState.Importing -> {}
                else -> {
                    LascoPrimaryButton(
                        text = "Import Now",
                        onClick = { viewModel.startDeviceImport() },
                        enabled = state.deviceScan?.let { it.tooLargeCount == 0 } == true,
                    )
                    LascoSecondaryButton(text = "Skip for now", onClick = onDone)
                }
            }
        }
    }
}

@Composable
private fun InitialImportPhaseProgress(phase: ImportPhase) {
    val colors = LascoTheme.colors
    when (phase) {
        ImportPhase.PreparingLibrary -> Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CircularProgressIndicator(color = colors.inkMuted, modifier = Modifier.height(14.dp).width(14.dp))
            Text(text = "Preparing your library…", style = LascoTheme.type.body(14), color = colors.inkMuted)
        }
        is ImportPhase.Adding -> {
            Text(
                text = "Adding items ${phase.range.first}–${phase.range.last}",
                style = LascoTheme.type.body(14),
                color = colors.inkMuted,
            )
            LinearProgressIndicator(
                progress = { phase.completed.toFloat() / (phase.range.last - phase.range.first + 1) },
                color = colors.inkMuted,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        is ImportPhase.Uploading -> {
            Text(
                text = "Uploading items ${phase.range.first}–${phase.range.last}",
                style = LascoTheme.type.body(14),
                color = colors.inkMuted,
            )
            LinearProgressIndicator(
                progress = { phase.progress },
                color = colors.inkMuted,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        is ImportPhase.Finalizing -> Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CircularProgressIndicator(color = colors.inkMuted, modifier = Modifier.height(14.dp).width(14.dp))
            Text(
                text = "Finalising backup for items ${phase.range.first}–${phase.range.last}…",
                style = LascoTheme.type.body(14),
                color = colors.inkMuted,
            )
        }
    }
}

@Composable
private fun ImportStatRow(label: String, value: String) {
    val colors = LascoTheme.colors
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(text = label, style = LascoTheme.type.body(14), color = colors.inkSub)
        Text(text = value, style = LascoTheme.type.mono(14), color = colors.ink)
    }
}

@Composable
private fun AutoImportStep(onYes: () -> Unit, onNo: () -> Unit) {
    val colors = LascoTheme.colors
    Column(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(text = "Automatically import new photos?", style = LascoTheme.type.title(26), color = colors.ink)
            Text(
                text = "Lasco can check for new photos each time you open the app and import any taken after this point.",
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
            LascoPrimaryButton(text = "Yes, auto-import new photos", onClick = onYes)
            LascoSecondaryButton(text = "No, not now", onClick = onNo)
        }
    }
}
