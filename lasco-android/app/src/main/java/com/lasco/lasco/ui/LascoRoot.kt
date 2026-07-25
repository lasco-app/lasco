package com.lasco.lasco.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.lasco.lasco.ui.library.LibraryListScreen
import com.lasco.lasco.ui.library.LibraryOpenScreen
import com.lasco.lasco.ui.onboarding.AddExistingLibraryScreen
import com.lasco.lasco.ui.onboarding.NewLibraryScreen

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
}

/**
 * Root of the app UI. Owns the pre open navigation state and routes between
 * the library list, the two add library flows, the open password prompt,
 * and the post open home surface.
 */
@Composable
fun LascoRoot(modifier: Modifier = Modifier, onLibraryOpenChanged: (Boolean) -> Unit = {}) {
    var screen by remember { mutableStateOf<Screen>(Screen.LibraryList) }

    LaunchedEffect(screen) { onLibraryOpenChanged(screen is Screen.Opened) }

    when (val current = screen) {
        Screen.LibraryList -> LibraryListScreen(
            onNewLibrary = { screen = Screen.NewLibrary },
            onAddExisting = { screen = Screen.AddExisting },
            onOpenLibrary = { entryId -> screen = Screen.OpeningLibrary(entryId) },
            modifier = modifier,
        )
        Screen.NewLibrary -> NewLibraryScreen(
            onBack = { screen = Screen.LibraryList },
            onLibraryOpened = { screen = Screen.Opened },
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
    }
}
