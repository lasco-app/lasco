package com.lasco.lasco.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.LascoRepository
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import com.lasco.lasco.ui.library.LibraryListScreen
import com.lasco.lasco.ui.library.LibraryListViewModel
import com.lasco.lasco.ui.library.LibraryOpenScreen
import com.lasco.lasco.ui.onboarding.AddExistingLibraryScreen
import com.lasco.lasco.ui.onboarding.NewLibraryWizardScreen
import com.lasco.lasco.ui.onboarding.OnboardingResume
import com.lasco.lasco.ui.onboarding.OnboardingScreen
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * The screens reachable around the library open flow. Kept as a small sealed
 * interface with a when below instead of pulling in a navigation library,
 * which is enough while the flow is shallow. It grows into real navigation
 * as more surfaces land.
 */
private sealed interface Screen {
    data object LibraryList : Screen
    data object NewLibrary : Screen
    data object AddExisting : Screen
    data class OpeningLibrary(val entryId: String) : Screen
    data object Opened : Screen
    data class Onboarding(val resume: OnboardingResume?) : Screen
}

/**
 * Root of the app UI. Owns the pre open navigation state and routes between
 * onboarding, the library list, the two add library flows, the open password
 * prompt, and the post open home surface.
 *
 * On startup, once the library list finishes loading, checks for a library
 * with an incomplete onboarding wizard (Prefs.onboardingStep) and tries to
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

        val incomplete = libraryListState.libraries.firstOrNull { prefs.onboardingStep(it.id) != null }
        if (incomplete != null) {
            val username = incomplete.username
            val opened = if (username != null) repository.openCached(nickname = incomplete.nickname, username = username) else null
            if (opened != null) {
                app.librarySession = LibraryRepository(
                    opened,
                    nickname = incomplete.nickname,
                    username = username!!,
                    appDir = repository.appDir,
                )
                var step = prefs.onboardingStep(incomplete.id) ?: 0
                // The master key can't be recovered after a process restart,
                // so resuming into the master key step is skipped.
                if (step == 1) step = 2
                screen = Screen.Onboarding(OnboardingResume(incomplete.id, incomplete.nickname, step))
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
            onNewLibrary = { screen = Screen.NewLibrary },
            onAddExisting = { screen = Screen.AddExisting },
            onOpenLibrary = { entryId -> screen = Screen.OpeningLibrary(entryId) },
            modifier = modifier,
            viewModel = libraryListViewModel,
        )
        Screen.NewLibrary -> NewLibraryWizardScreen(
            initialStep = 0,
            resumeLibraryId = null,
            resumeNickname = null,
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
        )
        is Screen.Onboarding -> OnboardingScreen(
            resume = current.resume,
            onComplete = { screen = Screen.LibraryList },
            onLibraryOpened = { screen = Screen.Opened },
            modifier = modifier,
        )
    }
}
