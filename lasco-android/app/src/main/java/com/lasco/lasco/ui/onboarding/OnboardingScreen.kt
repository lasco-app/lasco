package com.lasco.lasco.ui.onboarding

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
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
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.components.LascoSecondaryButton
import com.lasco.lasco.ui.theme.LascoTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.URL
import java.util.UUID

/** Resume information for a library whose onboarding was interrupted mid wizard. */
data class OnboardingResume(
    val sessionId: String,
    val libraryId: String,
    val nickname: String,
    val checkpoint: com.lasco.lasco.data.WizardCheckpoint,
)

private sealed interface OnboardingStep {
    data class Intro(val page: IntroPage) : OnboardingStep
    data object NewLibrary : OnboardingStep
    data object ExistingLibraryChoice : OnboardingStep
    data object AddExistingLibrary : OnboardingStep
}

private enum class IntroPage {
    SyncInfo,
    Encrypted,
    Safety,
    LibraryChoice,
}

private data class OnboardingUiState(
    val currentStep: OnboardingStep,
    val slideForward: Boolean,
)

/**
 * Top level onboarding flow, ported from the Swift OnboardingView. Shown only
 * when the library list is empty, or when resuming an interrupted wizard.
 * Internally reuses NewLibraryWizardScreen and AddExistingLibraryScreen, the
 * same composables LascoRoot also reaches directly from LibraryListScreen.
 *
 * The Swift .cloud flow is dead code upstream (nothing ever sets flow to
 * .cloud) and is intentionally not ported here.
 */
@Composable
fun OnboardingScreen(
    resume: OnboardingResume?,
    onComplete: () -> Unit,
    onLibraryOpened: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = LascoTheme.colors

    var state by remember {
        mutableStateOf(
            OnboardingUiState(
                currentStep = if (resume != null) OnboardingStep.NewLibrary else OnboardingStep.Intro(IntroPage.SyncInfo),
                slideForward = true,
            ),
        )
    }
    var wizardSessionId by remember { mutableStateOf(resume?.sessionId ?: UUID.randomUUID().toString()) }

    var encryptedMascot by remember { mutableStateOf<Bitmap?>(null) }
    var betaMascot by remember { mutableStateOf<Bitmap?>(null) }

    LaunchedEffect(Unit) {
        encryptedMascot = fetchNetworkBitmap("https://public.getlasco.app/mascot_encrypted_0_5x.png")
        betaMascot = fetchNetworkBitmap("https://public.getlasco.app/mascot_beta_0_5x.png")
    }

    fun transitionTo(step: OnboardingStep, slideForward: Boolean) {
        state = state.copy(slideForward = slideForward)
        state = state.copy(currentStep = step)
    }

    fun advanceIntro() {
        val currentPage = (state.currentStep as? OnboardingStep.Intro)?.page ?: return
        val nextPage = when (currentPage) {
            IntroPage.SyncInfo -> IntroPage.Encrypted
            IntroPage.Encrypted -> IntroPage.Safety
            IntroPage.Safety -> IntroPage.LibraryChoice
            IntroPage.LibraryChoice -> return
        }
        transitionTo(OnboardingStep.Intro(nextPage), slideForward = true)
    }

    fun goBack() {
        when (val step = state.currentStep) {
            is OnboardingStep.Intro -> when (step.page) {
                IntroPage.SyncInfo -> Unit
                IntroPage.Encrypted -> transitionTo(OnboardingStep.Intro(IntroPage.SyncInfo), slideForward = false)
                IntroPage.Safety -> transitionTo(OnboardingStep.Intro(IntroPage.Encrypted), slideForward = false)
                IntroPage.LibraryChoice -> transitionTo(OnboardingStep.Intro(IntroPage.Safety), slideForward = false)
            }
            OnboardingStep.NewLibrary -> transitionTo(
                OnboardingStep.Intro(IntroPage.LibraryChoice),
                slideForward = false,
            )
            OnboardingStep.ExistingLibraryChoice -> transitionTo(
                OnboardingStep.Intro(IntroPage.LibraryChoice),
                slideForward = false,
            )
            OnboardingStep.AddExistingLibrary -> transitionTo(
                OnboardingStep.ExistingLibraryChoice,
                slideForward = false,
            )
        }
    }

    fun skip() = onComplete()

    fun startFresh() {
        wizardSessionId = UUID.randomUUID().toString()
        transitionTo(OnboardingStep.NewLibrary, slideForward = true)
    }

    fun chooseExisting() = transitionTo(OnboardingStep.ExistingLibraryChoice, slideForward = true)

    fun openExistingLibrary() = transitionTo(OnboardingStep.AddExistingLibrary, slideForward = true)

    BackHandler(enabled = state.currentStep !is OnboardingStep.NewLibrary) {
        if (state.currentStep !is OnboardingStep.Intro) goBack()
    }

    Box(modifier = modifier.fillMaxSize().background(colors.bg)) {
        when (val step = state.currentStep) {
            is OnboardingStep.Intro -> MainCarousel(
                page = step.page,
                slideForward = state.slideForward,
                encryptedMascot = encryptedMascot,
                betaMascot = betaMascot,
                onSkip = ::skip,
                onNext = ::advanceIntro,
                onStartFresh = ::startFresh,
                onExisting = ::chooseExisting,
            )
            OnboardingStep.NewLibrary -> NewLibraryWizardScreen(
                sessionId = wizardSessionId,
                resume = resume?.takeIf { it.sessionId == wizardSessionId },
                onBack = ::goBack,
                onComplete = onLibraryOpened,
            )
            OnboardingStep.ExistingLibraryChoice -> ExistingChoiceBody(
                onBack = ::goBack,
                onOpenExistingLibrary = ::openExistingLibrary,
            )
            OnboardingStep.AddExistingLibrary -> AddExistingLibraryScreen(
                onBack = ::goBack,
                onLibraryOpened = onLibraryOpened,
            )
        }
    }
}

private suspend fun fetchNetworkBitmap(url: String): Bitmap? = withContext(Dispatchers.IO) {
    try {
        URL(url).openStream().use { BitmapFactory.decodeStream(it) }
    } catch (e: Exception) {
        null
    }
}

@Composable
private fun onboardingTopBar(dots: Int, current: Int, onBack: (() -> Unit)?) {
    val colors = LascoTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 32.dp)
            .padding(top = 32.dp, bottom = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            Text(
                text = "← Back",
                style = LascoTheme.type.body(14),
                color = colors.inkMuted,
                modifier = Modifier.clickable(interactionSource = null, indication = null) { onBack() },
            )
        } else {
            Spacer(modifier = Modifier.width(60.dp))
        }
        Spacer(modifier = Modifier.weight(1f))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (i in 0 until dots) {
                Box(
                    modifier = Modifier
                        .width(if (i == current) 20.dp else 8.dp)
                        .height(3.dp)
                        .background(if (i == current) colors.ink else colors.inkMuted.copy(alpha = 0.35f)),
                )
            }
        }
    }
}

@Composable
private fun onboardingLogo() {
    val colors = LascoTheme.colors
    Text(
        text = "LASCO",
        style = LascoTheme.type.categoryLarge(28),
        color = colors.ink,
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 32.dp)
            .padding(bottom = 8.dp),
    )
}

@Composable
private fun onboardingBottomBar(content: @Composable () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 32.dp)
            .padding(top = 20.dp, bottom = 48.dp),
    ) {
        content()
    }
}

@Composable
private fun MainCarousel(
    page: IntroPage,
    slideForward: Boolean,
    encryptedMascot: Bitmap?,
    betaMascot: Bitmap?,
    onSkip: () -> Unit,
    onNext: () -> Unit,
    onStartFresh: () -> Unit,
    onExisting: () -> Unit,
) {
    val colors = LascoTheme.colors
    val progressIndex = IntroPage.entries.indexOf(page)
    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 32.dp, bottom = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (i in 0 until 4) {
                    Box(
                        modifier = Modifier
                            .width(if (i == progressIndex) 20.dp else 8.dp)
                            .height(3.dp)
                        .background(if (i == progressIndex) colors.ink else colors.inkMuted.copy(alpha = 0.35f)),
                    )
                }
            }
            Spacer(modifier = Modifier.weight(1f))
            Text(
                text = "Skip",
                style = LascoTheme.type.body(14),
                color = colors.inkMuted,
                modifier = Modifier.clickable(interactionSource = null, indication = null) { onSkip() },
            )
        }

        onboardingLogo()

        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            AnimatedContent(
                targetState = page,
                transitionSpec = {
                    if (slideForward) {
                        slideInHorizontally(tween(300)) { it } togetherWith slideOutHorizontally(tween(300)) { -it }
                    } else {
                        slideInHorizontally(tween(300)) { -it } togetherWith slideOutHorizontally(tween(300)) { it }
                    }
                },
                label = "onboarding-page",
            ) { p ->
                when (p) {
                    IntroPage.SyncInfo -> SyncInfoPage()
                    IntroPage.Encrypted -> MascotPage(
                        title = "Fully encrypted.",
                        body = "Your photos are encrypted on your device before they ever leave it.",
                        mascot = encryptedMascot,
                    )
                    IntroPage.Safety -> MascotPage(
                        title = "Safety first !",
                        body = "Always make sure your data is correctly replicated at several places, within or without Lasco, before deleting things.",
                        mascot = betaMascot,
                    )
                    IntroPage.LibraryChoice -> ExistingLibraryPage(onStartFresh = onStartFresh, onExisting = onExisting)
                }
            }
        }

        if (page != IntroPage.LibraryChoice) {
            onboardingBottomBar {
                LascoPrimaryButton(text = "Next", onClick = onNext)
            }
        }
    }
}

@Composable
private fun SyncInfoPage() {
    val colors = LascoTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 32.dp)
            .padding(top = 40.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Text(
            text = "Sync your photo library to S3 file storage.",
            style = LascoTheme.type.title(26),
            color = colors.ink,
        )
        Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
            FeatureRow(
                title = "No server to self-host",
                body = buildAnnotatedString {
                    append("Syncs directly to ")
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("your S3") }
                    append(". No backend to deploy. No data sent to us.")
                },
            )
            FeatureRow(
                title = "Multi-device",
                body = buildAnnotatedString {
                    append("Lasco uses CRDT algorithms, the standard for local-first sync, so edits from every device merge nicely.")
                },
            )
            FeatureRow(
                title = "E2EE",
                body = buildAnnotatedString { append("Your photos are encrypted on your device before they leave.") },
            )
        }
    }
}

@Composable
private fun FeatureRow(title: String, body: androidx.compose.ui.text.AnnotatedString) {
    val colors = LascoTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(text = title, style = LascoTheme.type.title(16), color = colors.ink)
        Text(text = body, style = LascoTheme.type.body(16), color = colors.inkSub)
    }
}

@Composable
private fun MascotPage(title: String, body: String, mascot: Bitmap?) {
    val colors = LascoTheme.colors
    Box(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text(text = title, style = LascoTheme.type.title(26), color = colors.ink)
            Text(text = body, style = LascoTheme.type.body(16), color = colors.inkSub)
        }
        if (mascot != null) {
            Image(
                bitmap = mascot.asImageBitmap(),
                contentDescription = null,
                contentScale = ContentScale.Fit,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun ExistingLibraryPage(onStartFresh: () -> Unit, onExisting: () -> Unit) {
    val colors = LascoTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 32.dp)
            .padding(top = 40.dp),
    ) {
        Text(
            text = "Are you new in Lasco?",
            style = LascoTheme.type.title(26),
            color = colors.ink,
        )
        Spacer(modifier = Modifier.height(20.dp))
        Text(
            text = "If you've already set up a Lasco library on another device, you can add it here.",
            style = LascoTheme.type.body(16),
            color = colors.inkSub,
        )
        Spacer(modifier = Modifier.weight(1f))
        Column(
            modifier = Modifier.padding(bottom = 48.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            LascoPrimaryButton(text = "Yes, start fresh", onClick = onStartFresh)
            LascoSecondaryButton(text = "No, I have an existing Lasco library", onClick = onExisting)
        }
    }
}

@Composable
private fun ExistingChoiceBody(onBack: () -> Unit, onOpenExistingLibrary: () -> Unit) {
    val colors = LascoTheme.colors
    Column(modifier = Modifier.fillMaxSize()) {
        onboardingTopBar(dots = 1, current = 0, onBack = onBack)
        onboardingLogo()
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 32.dp)
                .padding(top = 40.dp),
        ) {
            Text(
                text = "Where is your library?",
                style = LascoTheme.type.title(26),
                color = colors.ink,
            )
        }
        onboardingBottomBar {
            LascoPrimaryButton(text = "S3-compatible storage", onClick = onOpenExistingLibrary)
        }
    }
}
