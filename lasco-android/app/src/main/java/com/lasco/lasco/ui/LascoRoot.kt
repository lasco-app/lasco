package com.lasco.lasco.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.LascoRepository
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.ui.library.LibraryListScreen
import com.lasco.lasco.ui.library.LibraryListViewModel
import com.lasco.lasco.ui.library.LibraryOpenScreen
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.onboarding.AddExistingLibraryScreen
import com.lasco.lasco.ui.onboarding.NewLibraryWizardScreen
import com.lasco.lasco.ui.onboarding.OnboardingResume
import com.lasco.lasco.ui.onboarding.OnboardingScreen
import com.lasco.lasco.ui.theme.LascoTheme
import java.util.UUID

/**
 * The screens reachable around the library open flow. Kept as a small sealed
 * interface with a when below instead of pulling in a navigation library,
 * which is enough while the flow is shallow. It grows into real navigation
 * as more surfaces land.
 */
private sealed interface Screen {
    data object LibraryList : Screen
    data class NewLibrary(val sessionId: String) : Screen
    data object AddExisting : Screen
    data class OpeningLibrary(val entryId: String) : Screen
    data object Opened : Screen
    data class DeletingLibrary(val libraryId: String) : Screen
    data class DeletionFailed(val libraryId: String, val detail: String) : Screen
    data class Onboarding(val resume: OnboardingResume?) : Screen
}

/**
 * Root of the app UI. Owns the pre open navigation state and routes between
 * onboarding, the library list, the two add library flows, the open password
 * prompt, and the post open home surface.
 *
 * On startup, once the library list finishes loading, checks for a library
 * with an incomplete onboarding wizard (Prefs.onboardingCheckpoint) and tries to
 * reopen its cached session so the wizard can resume where it left off,
 * mirroring LibraryCore.swift's init() resume logic. Nothing is shown while
 * this resolves, to avoid flashing the library list before redirecting into
 * onboarding.
 */
@Composable
fun LascoRoot(modifier: Modifier = Modifier, onLibraryOpenChanged: (Boolean) -> Unit = {}) {
    val context = LocalContext.current
    val app = remember { context.applicationContext as LascoApp }
    val repository = remember { LascoRepository.from(context) }
    val prefs = remember { Prefs.from(context) }

    val libraryListViewModel: LibraryListViewModel = viewModel(factory = LibraryListViewModel.Factory)
    val libraryListState by libraryListViewModel.uiState.collectAsStateWithLifecycle()

    var screen by remember { mutableStateOf<Screen?>(null) }

    LaunchedEffect(libraryListState.loading, libraryListState.libraries) {
        if (screen != null || libraryListState.loading) return@LaunchedEffect

        val incomplete = libraryListState.libraries.firstOrNull { prefs.onboardingCheckpoint(it.id) != null }
        if (incomplete != null) {
            val username = incomplete.username
            val opened = if (username != null) repository.openCached(nickname = incomplete.nickname, username = username) else null
            if (opened != null) {
                app.librarySession = LibraryRepository(
                    opened,
                    nickname = incomplete.nickname,
                    username = username!!,
                    appDir = repository.appDir,
                    context = app,
                    prefs = prefs,
                )
                val checkpoint = prefs.onboardingCheckpoint(incomplete.id)
                if (checkpoint == null) {
                    prefs.clearOnboardingIncomplete(incomplete.id)
                    screen = Screen.LibraryList
                } else {
                    screen = Screen.Onboarding(
                        OnboardingResume(UUID.randomUUID().toString(), incomplete.id, incomplete.nickname, checkpoint),
                    )
                }
                return@LaunchedEffect
            } else {
                prefs.clearOnboardingIncomplete(incomplete.id)
            }
        }

        screen = if (libraryListState.libraries.isEmpty()) Screen.Onboarding(resume = null) else Screen.LibraryList
    }

    val current = screen
    if (current == null) {
        Box(modifier = modifier.fillMaxSize().background(LascoTheme.colors.bg))
        return
    }

    LaunchedEffect(current) { onLibraryOpenChanged(current is Screen.Opened) }

    when (current) {
        Screen.LibraryList -> LibraryListScreen(
            onNewLibrary = { screen = Screen.NewLibrary(UUID.randomUUID().toString()) },
            onAddExisting = { screen = Screen.AddExisting },
            onOpenLibrary = { entryId -> screen = Screen.OpeningLibrary(entryId) },
            modifier = modifier,
            viewModel = libraryListViewModel,
        )
        is Screen.NewLibrary -> NewLibraryWizardScreen(
            sessionId = current.sessionId,
            resume = null,
            onBack = { screen = Screen.LibraryList },
            onComplete = { screen = Screen.Opened },
            modifier = modifier,
        )
        Screen.AddExisting -> AddExistingLibraryScreen(
            onBack = { screen = Screen.LibraryList },
            onLibraryOpened = { screen = Screen.Opened },
            modifier = modifier,
        )
        is Screen.OpeningLibrary -> LibraryOpenScreen(
            entryId = current.entryId,
            onBack = { screen = Screen.LibraryList },
            onOpened = { screen = Screen.Opened },
            modifier = modifier,
        )
        Screen.Opened -> MainScreen(
            modifier = modifier,
            onSignedOut = { screen = Screen.LibraryList },
            onDeleteLibrary = {
                app.librarySession?.sessionState?.value?.libraryId?.let { libraryId ->
                    screen = Screen.DeletingLibrary(libraryId)
                }
            },
        )
        is Screen.DeletingLibrary -> {
            // This is composed only after MainScreen has left composition. That cancels its
            // collectors before native library files are removed, preventing them from racing
            // an FFI read against deletion.
            LaunchedEffect(current.libraryId) {
                try {
                    app.librarySession?.close()
                    app.librarySession = null
                    repository.deleteLibrary(current.libraryId)
                    screen = Screen.LibraryList
                } catch (e: Throwable) {
                    app.librarySession = null
                    screen = Screen.DeletionFailed(
                        libraryId = current.libraryId,
                        detail = e.message ?: "The library could not be fully deleted.",
                    )
                }
            }
            DeletingLibraryScreen(modifier)
        }
        is Screen.DeletionFailed -> DeletionFailedScreen(
            detail = current.detail,
            onRetry = { screen = Screen.DeletingLibrary(current.libraryId) },
            onBack = { screen = Screen.LibraryList },
            modifier = modifier,
        )
        is Screen.Onboarding -> OnboardingScreen(
            resume = current.resume,
            onComplete = { screen = Screen.LibraryList },
            onLibraryOpened = { screen = Screen.Opened },
            modifier = modifier,
        )
    }
}

@Composable
private fun DeletingLibraryScreen(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier.fillMaxSize().background(LascoTheme.colors.bg),
        contentAlignment = Alignment.Center,
    ) {
        CircularProgressIndicator(color = LascoTheme.colors.ink)
    }
}

@Composable
private fun DeletionFailedScreen(
    detail: String,
    onRetry: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors
    Column(
        modifier = modifier.fillMaxSize().background(colors.bg).padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text = "Could not delete library", style = LascoTheme.type.title(26), color = colors.ink)
        Text(text = detail, style = LascoTheme.type.body(), color = colors.inkMuted, modifier = Modifier.padding(top = 12.dp))
        LascoPrimaryButton(text = "Try again", onClick = onRetry, modifier = Modifier.padding(top = 24.dp))
        LascoPrimaryButton(text = "Back to libraries", onClick = onBack, modifier = Modifier.padding(top = 12.dp))
    }
}
