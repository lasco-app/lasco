package app.lasco.importer

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.VerticalScrollbar
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.rememberScrollbarAdapter
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import app.lasco.lasco_desktop_importer.generated.resources.Res
import app.lasco.lasco_desktop_importer.generated.resources.lasco_icon
import org.jetbrains.compose.resources.painterResource
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.platform.Font
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.application
import app.lasco.importer.ffi.ExistingRemote
import app.lasco.importer.ffi.DEFAULT_LASCO_CLOUD_BASE_URL
import app.lasco.importer.ffi.LascoGateway
import app.lasco.importer.ffi.LibraryCredentials
import app.lasco.importer.ffi.OpenResult
import app.lasco.importer.ffi.RemoteConfig
import app.lasco.importer.ffi.UniffiImporterLibraryRepository
import app.lasco.importer.engine.ImportCoordinator
import app.lasco.importer.model.ImportPlan
import app.lasco.importer.model.ImportProgress
import app.lasco.importer.model.ImportRunState
import app.lasco.importer.model.RemoteBenchmark
import app.lasco.importer.source.ApplePhotosReader
import app.lasco.importer.source.GoogleTakeoutReader
import app.lasco.importer.source.ImportSourceReader
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.collect
import java.awt.FileDialog
import java.awt.Frame
import java.awt.Desktop
import java.net.URI
import java.nio.file.Path
import java.nio.file.Files
import java.util.Comparator

private enum class Page(val stage: Int) {
    CLOUD_SERVER(0), WELCOME(0), DESTINATION(1), CLOUD(1), S3(1), SMB(1),
    REMOTE_SYNC(1), SOURCE(2), TAKEOUT(2), PHOTOS(2), SCANNING(2), LIBRARY_SUMMARY(3), UPLOAD_ESTIMATE(4), IMPORT(5),
}

internal enum class RemoteType { CLOUD, S3, SMB }
private enum class SourceType { TAKEOUT, PHOTOS }
private data class ConnectionFailure(val remoteType: RemoteType, val message: String)
private data class DiscoveryFailure(val sourceType: SourceType, val message: String)
private data class DiscoveryProgress(val completed: Int = 0, val total: Int = 0)
private data class RemoteSyncProgress(
    val fetchedRemoteNames: List<String> = emptyList(),
    val currentRemoteName: String? = null,
    val totalRemotes: Int = 0,
)

internal fun isConnectionFormComplete(
    type: RemoteType,
    nickname: String,
    libraryUser: String,
    libraryPassword: String,
    remoteName: String,
    cloudEmail: String,
    cloudPassword: String,
    endpoint: String,
    bucket: String,
    region: String,
    accessKey: String,
    secretKey: String,
    server: String,
    port: String,
    share: String,
    smbUser: String,
    smbPassword: String,
): Boolean {
    fun String.isFilled() = isNotBlank()
    val libraryCredentialsComplete = nickname.isFilled() && libraryUser.isFilled() && libraryPassword.isFilled()
    if (!libraryCredentialsComplete) return false

    return when (type) {
        RemoteType.CLOUD -> cloudEmail.isFilled() && cloudPassword.isFilled()
        RemoteType.S3 -> remoteName.isFilled() && endpoint.isFilled() && bucket.isFilled() && region.isFilled() &&
            accessKey.isFilled() && secretKey.isFilled()
        RemoteType.SMB -> remoteName.isFilled() && server.isFilled() &&
            (port.toIntOrNull() in 1..65535) && share.isFilled() && smbUser.isFilled() && smbPassword.isFilled()
    }
}

internal fun isDevelopmentImporterBuild(): Boolean =
    System.getProperty("lasco.importer.release") == "false"

// The Plaster theme used by lasco-android.
private val Plaster = Color(0xFFE6E2D4)
private val PlasterDeep = Color(0xFFD2CDBA)
private val Panel = Color(0xFFF3EFE2)
private val Ink = Color(0xFF1A1A1A)
private val InkSub = Color(0xFF4A4A48)
private val InkMuted = Color(0xFF8A8682)
private val Accent = Color(0xFF0A0F2E)
private val Pink = Color(0xFFE84A8A)
private val Good = Color(0xFF5B8B3E)
private val Error = Color(0xFFC44A3E)
private val Jersey10 = FontFamily(Font("jersey10_regular.ttf"))
private val VT323 = FontFamily(Font("vt323_regular.ttf"))
private val SpaceGrotesk = FontFamily(
    Font("space_grotesk_regular.ttf", FontWeight.Normal),
    Font("space_grotesk_bold.ttf", FontWeight.Bold),
)
private val JetBrainsMono = FontFamily(Font("jetbrains_mono_regular.ttf"))
private val LascoHeading = TextStyle(fontFamily = Jersey10, fontSize = 26.sp)
private val LascoBody = TextStyle(fontFamily = SpaceGrotesk, fontSize = 15.sp)
private val LascoLabel = TextStyle(fontFamily = Jersey10, fontSize = 12.sp, letterSpacing = 1.sp)
private val LascoPixel = TextStyle(fontFamily = VT323, fontSize = 15.sp)
private val LascoMono = TextStyle(fontFamily = JetBrainsMono, fontSize = 13.sp)

private fun clearTransientStaging() {
    val staging = Path.of(System.getProperty("user.home"), ".lasco-desktop-importer", "staging")
    if (!Files.exists(staging)) return
    Files.walk(staging).use { paths ->
        paths.sorted(Comparator.reverseOrder()).forEach(Files::deleteIfExists)
    }
}

fun main() = application {
    Window(
        onCloseRequest = ::exitApplication,
        title = "Lasco Desktop Importer",
        icon = painterResource(Res.drawable.lasco_icon),
    ) {
        MaterialTheme(
            colorScheme = lightColorScheme(
                primary = Accent, onPrimary = Color.White, background = Plaster,
                onBackground = Ink, surface = Panel, onSurface = Ink, error = Error,
            ),
        ) { FfiReadinessGate { ImporterWizard() } }
    }
}

/** Do not show the wizard until the generated FFI can load and read its app configuration. */
@Composable
private fun FfiReadinessGate(content: @Composable () -> Unit) {
    var checking by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    fun checkFfi() {
        checking = true
        error = null
        scope.launch {
            error = withContext(Dispatchers.IO) {
                runCatching {
                    clearTransientStaging()
                    UniffiImporterLibraryRepository(
                        Path.of(System.getProperty("user.home"), ".lasco-desktop-importer"),
                    ).use { it.list() }
                }.exceptionOrNull()?.message?.ifBlank { "The Lasco native library could not be loaded." }
            }
            checking = false
        }
    }

    LaunchedEffect(Unit) { checkFfi() }
    when {
        checking -> Box(Modifier.fillMaxSize().background(Plaster), contentAlignment = Alignment.Center) {
            Text("CHECKING LASCO…", color = InkSub, style = LascoPixel)
        }
        error != null -> Column(
            Modifier.fillMaxSize().background(Plaster).padding(40.dp),
            verticalArrangement = Arrangement.Center,
        ) {
            Text("LASCO IS NOT CONFIGURED", color = Error, style = LascoHeading)
            Spacer(Modifier.height(10.dp))
            Text(error ?: "The native Lasco importer is unavailable.", color = InkSub, style = LascoBody)
            Spacer(Modifier.height(20.dp))
            LascoButton("RETRY FFI CHECK", ::checkFfi, fillWidth = false)
        }
        else -> content()
    }
}

@Composable
private fun ImporterWizard() {
    val isDevelopmentBuild = remember { isDevelopmentImporterBuild() }
    var page by remember {
        mutableStateOf(if (isDevelopmentBuild) Page.CLOUD_SERVER else Page.WELCOME)
    }
    var forward by remember { mutableStateOf(true) }
    var remoteType by remember { mutableStateOf<RemoteType?>(null) }
    var sourceType by remember { mutableStateOf<SourceType?>(null) }
    var nickname by remember { mutableStateOf("") }
    var libraryUser by remember { mutableStateOf("") }
    var libraryPassword by remember { mutableStateOf("") }
    var remoteName by remember { mutableStateOf("") }
    var cloudUrl by remember {
        mutableStateOf(if (isDevelopmentBuild) "" else DEFAULT_LASCO_CLOUD_BASE_URL)
    }
    var cloudEmail by remember { mutableStateOf("") }
    var cloudPassword by remember { mutableStateOf("") }
    var endpoint by remember { mutableStateOf("") }
    var bucket by remember { mutableStateOf("") }
    var region by remember { mutableStateOf("") }
    var prefix by remember { mutableStateOf("") }
    var accessKey by remember { mutableStateOf("") }
    var secretKey by remember { mutableStateOf("") }
    var server by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("445") }
    var share by remember { mutableStateOf("") }
    var smbUser by remember { mutableStateOf("") }
    var smbPassword by remember { mutableStateOf("") }
    var domain by remember { mutableStateOf("") }
    var gateway by remember { mutableStateOf<LascoGateway?>(null) }
    var connecting by remember { mutableStateOf(false) }
    var connectionFailure by remember { mutableStateOf<ConnectionFailure?>(null) }
    var archives by remember { mutableStateOf(emptyList<String>()) }
    var photosAllowed by remember { mutableStateOf(false) }
    var requestingPhotos by remember { mutableStateOf(false) }
    var photosError by remember { mutableStateOf<String?>(null) }
    var photosPermissionDenied by remember { mutableStateOf(false) }
    var coordinator by remember { mutableStateOf<ImportCoordinator?>(null) }
    var importPlan by remember { mutableStateOf<ImportPlan?>(null) }
    var benchmarks by remember { mutableStateOf(emptyList<RemoteBenchmark>()) }
    var benchmarking by remember { mutableStateOf(false) }
    var benchmarkError by remember { mutableStateOf<String?>(null) }
    var importProgress by remember { mutableStateOf(ImportProgress(ImportRunState.READY, 0, 0, 0, 0)) }
    var discovering by remember { mutableStateOf(false) }
    var discoveryFailure by remember { mutableStateOf<DiscoveryFailure?>(null) }
    var discoveryProgress by remember { mutableStateOf(DiscoveryProgress()) }
    var syncingRemotes by remember { mutableStateOf(false) }
    var remoteSyncProgress by remember { mutableStateOf(RemoteSyncProgress()) }
    var remoteSyncError by remember { mutableStateOf<String?>(null) }
    var remoteSyncReady by remember { mutableStateOf(false) }
    var importRunning by remember { mutableStateOf(false) }
    var importError by remember { mutableStateOf<String?>(null) }
    var destinations by remember { mutableStateOf(DestinationUiState()) }
    val scope = rememberCoroutineScope()
    val libraryRepository = remember {
        UniffiImporterLibraryRepository(
            Path.of(System.getProperty("user.home"), ".lasco-desktop-importer"),
            cloudBaseUrl = { cloudUrl },
        )
    }

    DisposableEffect(libraryRepository) {
        onDispose { libraryRepository.close() }
    }

    fun refreshDestinations() {
        destinations = destinations.copy(loading = true, listError = null)
        scope.launch {
            try {
                destinations = destinations.loaded(withContext(Dispatchers.IO) { libraryRepository.list() })
            } catch (failure: Throwable) {
                destinations = destinations.listFailed(
                    failure.message?.ifBlank { null } ?: "Could not load local importer setups.",
                )
            }
        }
    }

    LaunchedEffect(Unit) { refreshDestinations() }

    fun go(to: Page) {
        forward = to.ordinal > page.ordinal
        page = to
    }

    fun connectionFormIsComplete(): Boolean {
        val type = remoteType ?: return false
        return isConnectionFormComplete(
            type = type,
            nickname = nickname,
            libraryUser = libraryUser,
            libraryPassword = libraryPassword,
            remoteName = remoteName,
            cloudEmail = cloudEmail,
            cloudPassword = cloudPassword,
            endpoint = endpoint,
            bucket = bucket,
            region = region,
            accessKey = accessKey,
            secretKey = secretKey,
            server = server,
            port = port,
            share = share,
            smbUser = smbUser,
            smbPassword = smbPassword,
        )
    }

    fun connect() {
        val kind = remoteType ?: return
        if (!connectionFormIsComplete()) return
        connecting = true
        connectionFailure = null
        scope.launch {
            try {
                val remote = when (kind) {
                    RemoteType.CLOUD -> ExistingRemote.LascoCloud(remoteName.ifBlank { "Lasco Cloud" }, cloudUrl, cloudEmail, cloudPassword)
                    RemoteType.S3 -> ExistingRemote.S3(remoteName, endpoint, bucket, region, prefix, accessKey, secretKey)
                    RemoteType.SMB -> ExistingRemote.Smb(remoteName, server, port.toIntOrNull() ?: 0, share, prefix, smbUser, smbPassword, domain.ifBlank { null })
                }
                withContext(Dispatchers.IO) {
                    libraryRepository.addInitialLibrary(LibraryCredentials(nickname, libraryUser, libraryPassword), remote)
                }
                refreshDestinations()
                go(Page.DESTINATION)
            } catch (failure: Throwable) {
                connectionFailure = ConnectionFailure(
                    remoteType = kind,
                    message = failure.message?.ifBlank { null }
                        ?: "Could not connect to this remote. Check the values and try again.",
                )
            } finally {
                connecting = false
            }
        }
    }

    fun syncRemotesBeforeChoosingSource(connectedGateway: LascoGateway) {
        gateway = connectedGateway
        syncingRemotes = true
        remoteSyncError = null
        remoteSyncReady = false
        remoteSyncProgress = RemoteSyncProgress()
        go(Page.REMOTE_SYNC)
        scope.launch {
            try {
                val remotes = withContext(Dispatchers.IO) { connectedGateway.remotes() }
                require(remotes.isNotEmpty()) { "This library has no remote to synchronize." }
                remoteSyncProgress = RemoteSyncProgress(totalRemotes = remotes.size)
                remotes.forEach { remote ->
                    remoteSyncProgress = remoteSyncProgress.copy(currentRemoteName = remote.name)
                    withContext(Dispatchers.IO) { connectedGateway.fetchRemoteOperations(remote) }
                    remoteSyncProgress = remoteSyncProgress.copy(
                        fetchedRemoteNames = remoteSyncProgress.fetchedRemoteNames + remote.name,
                        currentRemoteName = null,
                    )
                }
                val remotesMissingOperations = withContext(Dispatchers.IO) {
                    remotes.filter(connectedGateway::hasUnpushedOperations)
                }
                if (remotesMissingOperations.isNotEmpty()) {
                    remoteSyncError = "These remotes do not contain the same library operation-log state. Open Lasco and sync this library with every configured remote, then return here and try again."
                } else {
                    remoteSyncReady = true
                }
            } catch (failure: Throwable) {
                remoteSyncError = failure.message?.ifBlank { null }
                    ?: "Could not synchronize the configured remotes."
            } finally {
                syncingRemotes = false
            }
        }
    }

    fun discover() {
        val connectedGateway = gateway ?: return
        val selectedSource = sourceType ?: return
        discovering = true
        discoveryFailure = null
        discoveryProgress = DiscoveryProgress()
        go(Page.SCANNING)
        scope.launch {
            val reader: ImportSourceReader = when (selectedSource) {
                SourceType.TAKEOUT -> GoogleTakeoutReader(archives.map(Path::of))
                SourceType.PHOTOS -> ApplePhotosReader()
            }
            val progressPolling = (reader as? ApplePhotosReader)?.let { photosReader ->
                scope.launch {
                    while (discovering) {
                        val (completed, total) = photosReader.discoveryProgress()
                        discoveryProgress = DiscoveryProgress(completed, total)
                        delay(150)
                    }
                }
            }
            try {
                val prepared = withContext(Dispatchers.IO) {
                    val newCoordinator = ImportCoordinator(
                        connectedGateway,
                        Path.of(System.getProperty("user.home"), ".lasco-desktop-importer", "staging"),
                    )
                    newCoordinator to newCoordinator.discover(reader)
                }
                coordinator = prepared.first
                importPlan = prepared.second
                benchmarks = emptyList()
                benchmarkError = null
                go(Page.LIBRARY_SUMMARY)
            } catch (failure: Throwable) {
                discoveryFailure = DiscoveryFailure(
                    sourceType = selectedSource,
                    message = failure.message?.ifBlank { null } ?: "Could not scan this source.",
                )
            } finally {
                progressPolling?.cancel()
                discovering = false
            }
        }
    }

    fun benchmarkUploadSpeed() {
        val activeCoordinator = coordinator ?: return
        val connectedGateway = gateway ?: return
        benchmarking = true
        benchmarkError = null
        go(Page.UPLOAD_ESTIMATE)
        scope.launch {
            try {
                benchmarks = withContext(Dispatchers.IO) {
                    activeCoordinator.benchmark(connectedGateway.remotes())
                }
            } catch (failure: Throwable) {
                benchmarkError = failure.message?.ifBlank { null } ?: "Could not benchmark upload speed."
            } finally {
                benchmarking = false
            }
        }
    }

    fun startImport() {
        if (importProgress.state == ImportRunState.COMPLETE) return
        val activeCoordinator = coordinator ?: return
        importRunning = true
        importError = null
        go(Page.IMPORT)
        scope.launch {
            try {
                withContext(Dispatchers.IO) { activeCoordinator.startOrResume(benchmarks) }
            } catch (failure: Throwable) {
                importError = failure.message?.ifBlank { null } ?: "The import stopped unexpectedly. You can fix the issue and resume."
            } finally { importRunning = false }
        }
    }

    LaunchedEffect(coordinator) {
        coordinator?.progress?.collect { importProgress = it }
    }

    Column(Modifier.fillMaxSize().background(Plaster).padding(horizontal = 32.dp, vertical = 24.dp)) {
        ProgressHeader(page.stage)
        Spacer(Modifier.height(18.dp))
        Surface(Modifier.fillMaxWidth().weight(1f), color = Panel) {
            AnimatedContent(
                targetState = page,
                transitionSpec = {
                    val direction = if (forward) 1 else -1
                    (slideInHorizontally(tween(280, easing = FastOutSlowInEasing)) { it * direction } + fadeIn(tween(150))) togetherWith
                        (slideOutHorizontally(tween(220, easing = FastOutSlowInEasing)) { -it * direction } + fadeOut(tween(120)))
                },
                label = "wizard-swipe",
            ) { current ->
                val pageScrollState = rememberScrollState()
                Box(Modifier.fillMaxSize()) {
                    Column(Modifier.fillMaxSize().verticalScroll(pageScrollState).padding(28.dp)) {
                        when (current) {
                        Page.CLOUD_SERVER -> CloudServerPage(
                            cloudUrl = cloudUrl,
                            setCloudUrl = { cloudUrl = it },
                            onContinue = { go(Page.WELCOME) },
                        )
                        Page.WELCOME -> WelcomePage()
                        Page.DESTINATION -> DestinationPicker(
                            state = destinations,
                            onUse = { summary ->
                                destinations = destinations.copy(selectedLibraryId = summary.libraryId).clearLibraryError(summary.libraryId)
                                scope.launch {
                                    when (val opened = withContext(Dispatchers.IO) { libraryRepository.openCached(summary.libraryId) }) {
                                        is OpenResult.Open -> {
                                            syncRemotesBeforeChoosingSource(opened.gateway)
                                        }
                                        OpenResult.CredentialsRequired -> destinations = destinations.copy(dialog = DestinationDialog.Unlock(summary))
                                        is OpenResult.Failed -> destinations = destinations.withLibraryError(summary.libraryId, opened.message)
                                    }
                                }
                            },
                            onUnlock = { summary -> destinations = destinations.copy(dialog = DestinationDialog.Unlock(summary)) },
                            onAddRemote = { summary -> destinations = destinations.copy(dialog = DestinationDialog.AddRemote(summary)) },
                            onRemove = { summary -> destinations = destinations.copy(dialog = DestinationDialog.RemoveSetup(summary)) },
                            onCloud = {
                                connectionFailure = null
                                remoteType = RemoteType.CLOUD
                                remoteName = "Lasco Cloud"
                                go(Page.CLOUD)
                            },
                            onS3 = {
                                connectionFailure = null
                                remoteType = RemoteType.S3
                                go(Page.S3)
                            },
                            onSmb = {
                                connectionFailure = null
                                remoteType = RemoteType.SMB
                                go(Page.SMB)
                            },
                        )
                        Page.CLOUD, Page.S3, Page.SMB -> ConnectionForm(
                            type = remoteType ?: RemoteType.CLOUD,
                            nickname = nickname, setNickname = { nickname = it }, libraryUser = libraryUser, setLibraryUser = { libraryUser = it }, libraryPassword = libraryPassword, setLibraryPassword = { libraryPassword = it },
                            remoteName = remoteName, setRemoteName = { remoteName = it }, cloudEmail = cloudEmail, setCloudEmail = { cloudEmail = it }, cloudPassword = cloudPassword, setCloudPassword = { cloudPassword = it },
                            endpoint = endpoint, setEndpoint = { endpoint = it }, bucket = bucket, setBucket = { bucket = it }, region = region, setRegion = { region = it }, prefix = prefix, setPrefix = { prefix = it }, accessKey = accessKey, setAccessKey = { accessKey = it }, secretKey = secretKey, setSecretKey = { secretKey = it },
                            server = server, setServer = { server = it }, port = port, setPort = { port = it }, share = share, setShare = { share = it }, smbUser = smbUser, setSmbUser = { smbUser = it }, smbPassword = smbPassword, setSmbPassword = { smbPassword = it }, domain = domain, setDomain = { domain = it },
                            error = connectionFailure?.takeIf { it.remoteType == (remoteType ?: RemoteType.CLOUD) }?.message,
                        )
                        Page.REMOTE_SYNC -> RemoteSyncPage(
                            progress = remoteSyncProgress,
                            syncing = syncingRemotes,
                            error = remoteSyncError,
                            ready = remoteSyncReady,
                            onRetry = { gateway?.let(::syncRemotesBeforeChoosingSource) },
                            onChooseAnotherLibrary = { go(Page.DESTINATION) },
                        )
                        Page.SOURCE -> SourcePicker(
                            onTakeout = { discoveryFailure = null; sourceType = SourceType.TAKEOUT; go(Page.TAKEOUT) },
                            onPhotos = { discoveryFailure = null; sourceType = SourceType.PHOTOS; go(Page.PHOTOS) },
                        )
                        Page.TAKEOUT -> TakeoutPage(
                            archives,
                            discoveryFailure?.takeIf { it.sourceType == SourceType.TAKEOUT }?.message,
                        ) { archives = chooseTakeoutZips() }
                        Page.PHOTOS -> PhotosPage(
                            granted = photosAllowed,
                            requesting = requestingPhotos,
                            error = photosError ?: discoveryFailure?.takeIf { it.sourceType == SourceType.PHOTOS }?.message,
                            permissionDenied = photosPermissionDenied,
                            request = {
                                photosError = null
                                photosPermissionDenied = false
                                requestingPhotos = true
                                scope.launch {
                                    try {
                                        photosAllowed = withContext(Dispatchers.IO) {
                                            ApplePhotosReader().let { if (it.hasPermission()) true else it.requestPermission() }
                                        }
                                        if (photosAllowed) {
                                            discover()
                                        } else {
                                            photosPermissionDenied = true
                                            photosError = "Photos access was not granted. Allow Lasco in System Settings, then try again."
                                        }
                                    } catch (failure: Throwable) {
                                        photosError = failure.message?.ifBlank { null } ?: "Could not request Photos access."
                                    } finally { requestingPhotos = false }
                                }
                            },
                            openSettings = {
                                runCatching(::openPhotosPrivacySettings).onFailure { failure ->
                                    photosError = failure.message?.ifBlank { null }
                                        ?: "Could not open macOS Photos settings."
                                }
                            },
                        )
                        Page.SCANNING -> ScanningPage(
                            source = sourceType,
                            progress = discoveryProgress,
                            error = discoveryFailure?.takeIf { it.sourceType == sourceType }?.message,
                            onRetry = ::discover,
                            onChooseAnotherSource = { go(Page.SOURCE) },
                        )
                        Page.LIBRARY_SUMMARY -> LibrarySummaryPage(sourceType, archives, importPlan)
                        Page.UPLOAD_ESTIMATE -> UploadEstimatePage(importPlan, benchmarks, benchmarking, benchmarkError)
                        Page.IMPORT -> ImportPage(
                            progress = importProgress,
                            running = importRunning,
                            error = importError,
                            onStart = ::startImport,
                            onPause = { coordinator?.requestPause() },
                            onBackToStart = {
                                coordinator = null
                                importPlan = null
                                benchmarks = emptyList()
                                importProgress = ImportProgress(ImportRunState.READY, 0, 0, 0, 0)
                                importError = null
                                sourceType = null
                                refreshDestinations()
                                go(if (isDevelopmentBuild) Page.CLOUD_SERVER else Page.WELCOME)
                            },
                        )
                        }
                    }
                    VerticalScrollbar(
                        adapter = rememberScrollbarAdapter(pageScrollState),
                        modifier = Modifier.align(Alignment.CenterEnd).fillMaxHeight(),
                    )
                }
            }
        }
        Spacer(Modifier.height(16.dp))
        WizardFooter(
            page = page, source = sourceType, connecting = connecting, connectEnabled = connectionFormIsComplete(), discovering = discovering, archivesReady = archives.isNotEmpty(), photosReady = photosAllowed,
            benchmarking = benchmarking, benchmarkReady = benchmarks.isNotEmpty(), remoteSyncReady = remoteSyncReady,
            librarySummaryHasWork = importPlan?.let { it.hasMediaToUpload || it.metadataToAdd } == true,
            librarySummaryHasMediaToUpload = importPlan?.hasMediaToUpload == true,
            onBack = {
                when (page) {
                    Page.CLOUD_SERVER -> Unit
                    Page.DESTINATION -> go(Page.WELCOME)
                    Page.CLOUD, Page.S3, Page.SMB -> {
                        connectionFailure = null
                        go(Page.DESTINATION)
                    }
                    Page.REMOTE_SYNC -> go(Page.DESTINATION)
                    Page.SOURCE -> go(Page.DESTINATION)
                    Page.TAKEOUT, Page.PHOTOS -> go(Page.SOURCE)
                    Page.SCANNING -> Unit
                    Page.LIBRARY_SUMMARY -> go(if (sourceType == SourceType.PHOTOS) Page.PHOTOS else Page.TAKEOUT)
                    Page.UPLOAD_ESTIMATE -> go(Page.LIBRARY_SUMMARY)
                    Page.IMPORT -> go(Page.UPLOAD_ESTIMATE)
                    Page.WELCOME -> Unit
                }
            },
            onConnect = ::connect,
            onContinue = { when (page) {
                Page.WELCOME -> go(Page.DESTINATION)
                Page.REMOTE_SYNC -> if (remoteSyncReady) go(Page.SOURCE)
                Page.LIBRARY_SUMMARY -> if (importPlan?.hasMediaToUpload == true) benchmarkUploadSpeed() else startImport()
                Page.UPLOAD_ESTIMATE -> if (benchmarks.isNotEmpty()) startImport()
                else -> discover()
            } },
        )
    }
    DestinationDialogHost(
        dialog = destinations.dialog,
        onDismiss = { destinations = destinations.copy(dialog = null) },
        onUnlock = { summary, username, password ->
            scope.launch {
                when (val opened = withContext(Dispatchers.IO) {
                    libraryRepository.openWithCredentials(LibraryCredentials(summary.nickname, username, password))
                }) {
                    is OpenResult.Open -> {
                        destinations = destinations.copy(dialog = null).clearLibraryError(summary.libraryId)
                        syncRemotesBeforeChoosingSource(opened.gateway)
                    }
                    OpenResult.CredentialsRequired -> destinations = destinations.withLibraryError(summary.libraryId, "Credentials are required to unlock this library.")
                    is OpenResult.Failed -> destinations = destinations.withLibraryError(summary.libraryId, opened.message)
                }
            }
        },
        onAddRemote = { summary, remote ->
            scope.launch {
                try {
                    withContext(Dispatchers.IO) { libraryRepository.addRemote(summary.libraryId, remote) }
                    destinations = destinations.copy(dialog = null).clearLibraryError(summary.libraryId)
                    refreshDestinations()
                } catch (failure: Throwable) {
                    destinations = destinations.withLibraryError(
                        summary.libraryId,
                        failure.message?.ifBlank { null } ?: "Could not add this remote. The incomplete remote was removed.",
                    )
                }
            }
        },
        onRemove = { summary ->
            scope.launch {
                try {
                    withContext(Dispatchers.IO) { libraryRepository.deleteLocalSetup(summary.libraryId) }
                    destinations = destinations.copy(dialog = null)
                    refreshDestinations()
                } catch (failure: Throwable) {
                    destinations = destinations.withLibraryError(
                        summary.libraryId,
                        failure.message?.ifBlank { null } ?: "Could not remove this local importer setup.",
                    )
                }
            }
        },
    )
}

@Composable
private fun ProgressHeader(stage: Int) {
    Text("LASCO", color = Ink, style = LascoHeading.copy(fontSize = 30.sp), fontWeight = FontWeight.Black, letterSpacing = 2.sp)
    Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        listOf("1. HOW IT WORKS", "2. DESTINATION", "3. SOURCE", "4. LIBRARY SUMMARY", "5. EXPECTED TIME", "6. IMPORT").forEachIndexed { index, label ->
            Text(label, color = if (index == stage) Pink else InkMuted, style = LascoLabel.copy(fontSize = 16.sp), fontWeight = FontWeight.Bold)
        }
    }
}

@Composable
private fun WelcomePage() {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        Text("How importing works", color = Ink, style = LascoHeading, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center)
        Spacer(Modifier.height(24.dp))
        Column(Modifier.widthIn(max = 620.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            HowItWorksItem("1", "You need an existing Lasco library.")
            HowItWorksItem("2", "Set up the remote or remotes used to connect to it.")
            HowItWorksItem("3", "Choose Apple Photos / iCloud, or Google Takeout with its ZIP archives.")
            HowItWorksItem("4", "Review your library and what each remote already contains.")
            HowItWorksItem("5", "Benchmark each remote, review the expected upload time, then start. You can safely pause and resume it.")
        }
    }
}

@Composable
private fun CloudServerPage(
    cloudUrl: String,
    setCloudUrl: (String) -> Unit,
    onContinue: () -> Unit,
) {
    PageTitle(
        "Lasco Cloud server",
        "Choose the Lasco Cloud server for this session. This address is not saved.",
    )
    Spacer(Modifier.height(20.dp))
    Column(Modifier.widthIn(max = 620.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        LascoField("Cloud server", cloudUrl, setCloudUrl, DEFAULT_LASCO_CLOUD_BASE_URL)
        LascoButton("CONTINUE", onContinue, enabled = cloudUrl.isNotBlank(), fillWidth = false)
    }
}

@Composable
private fun HowItWorksItem(number: String, text: String) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Start, verticalAlignment = Alignment.Top) {
        Text(number, color = Color.White, style = LascoLabel.copy(fontSize = 13.sp), fontWeight = FontWeight.Bold, modifier = Modifier.background(Accent).border(1.dp, Ink).padding(horizontal = 8.dp, vertical = 4.dp))
        Text(text, color = InkSub, style = LascoBody.copy(fontSize = 16.sp), textAlign = TextAlign.Start, modifier = Modifier.padding(start = 12.dp, top = 3.dp).widthIn(max = 500.dp))
    }
}

@Composable
private fun DestinationPicker(
    state: DestinationUiState,
    onUse: (app.lasco.importer.ffi.ImporterLibrarySummary) -> Unit,
    onUnlock: (app.lasco.importer.ffi.ImporterLibrarySummary) -> Unit,
    onAddRemote: (app.lasco.importer.ffi.ImporterLibrarySummary) -> Unit,
    onRemove: (app.lasco.importer.ffi.ImporterLibrarySummary) -> Unit,
    onCloud: () -> Unit,
    onS3: () -> Unit,
    onSmb: () -> Unit,
) {
    var addingAnotherLibrary by remember { mutableStateOf(false) }

    PageTitle("Import photos into Lasco")
    Spacer(Modifier.height(24.dp))
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (state.loading) Text("LOADING LOCAL LIBRARIES…", color = InkSub, style = LascoPixel)
        state.listError?.let { ErrorMessage(it) }
        state.libraries.forEach { summary ->
            DestinationCard(
                summary = summary,
                error = state.errorByLibraryId[summary.libraryId],
                onUse = { onUse(summary) },
                onUnlock = { onUnlock(summary) },
                onAddRemote = { onAddRemote(summary) },
                onRemove = { onRemove(summary) },
            )
        }
        if (!state.loading) {
            if (state.libraries.isEmpty()) {
                NewLibrarySetupPanel("ADD NEW LIBRARY", onCloud, onS3, onSmb)
            } else if (addingAnotherLibrary) {
                NewLibrarySetupPanel("ADD ANOTHER LIBRARY", onCloud, onS3, onSmb)
            } else {
                LascoButton(
                    "ADD ANOTHER LIBRARY",
                    { addingAnotherLibrary = true },
                    primary = false,
                    modifier = Modifier.widthIn(max = 460.dp),
                )
            }
        }
    }
}

@Composable
private fun NewLibrarySetupPanel(
    title: String,
    onCloud: () -> Unit,
    onS3: () -> Unit,
    onSmb: () -> Unit,
) {
    Column(
        Modifier.widthIn(max = 460.dp).fillMaxWidth().background(Color.White).border(2.dp, Ink).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(title, color = Ink, style = LascoLabel, fontWeight = FontWeight.Bold)
        LascoButton("LASCO CLOUD", onCloud)
        LascoButton("S3-COMPATIBLE STORAGE", onS3)
        LascoButton("SMB NETWORK SHARE", onSmb)
    }
}

@Composable
private fun DestinationCard(
    summary: app.lasco.importer.ffi.ImporterLibrarySummary,
    error: String?,
    onUse: () -> Unit,
    onUnlock: () -> Unit,
    onAddRemote: () -> Unit,
    onRemove: () -> Unit,
) {
    Column(
        Modifier.widthIn(max = 620.dp).fillMaxWidth().background(Color.White).border(2.dp, Ink).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(summary.nickname, color = Ink, style = LascoHeading.copy(fontSize = 20.sp), fontWeight = FontWeight.Bold)
        Text(
            if (summary.username == null) "LOCKED · unlock with this library's credentials" else "READY · ${summary.username}",
            color = if (summary.username == null) InkMuted else Good,
            style = LascoLabel,
            fontWeight = FontWeight.Bold,
        )
        Text(
            if (summary.remotes.isEmpty()) "No remotes configured"
            else "REMOTES: ${summary.remotes.joinToString { "${it.name} (${importerRemoteTypeLabel(it.kind)})" }}",
            color = InkSub,
            style = LascoBody.copy(fontSize = 13.sp),
        )
        summary.loadError?.let { ErrorMessage(it) }
        error?.let { ErrorMessage(it) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            LascoButton("USE THIS LIBRARY", onUse, fillWidth = false)
            if (summary.username == null) LascoButton("UNLOCK", onUnlock, primary = false, fillWidth = false)
            LascoButton("ADD REMOTE", onAddRemote, primary = false, fillWidth = false)
            LascoButton("REMOVE LOCAL SETUP", onRemove, primary = false, fillWidth = false)
        }
    }
}

private fun importerRemoteTypeLabel(kind: String): String = when (kind) {
    "lasco_cloud_s3" -> "Lasco Cloud"
    "s3" -> "S3"
    "smb" -> "SMB"
    "fixed_path" -> "Local folder"
    "usb_android" -> "Android USB"
    "usb_apple" -> "Apple USB"
    "debug_local_apple" -> "Local Apple (debug)"
    "debug_local_android" -> "Local Android (debug)"
    else -> kind.ifBlank { "Unknown" }
}

@Composable
private fun DestinationDialogHost(
    dialog: DestinationDialog?,
    onDismiss: () -> Unit,
    onUnlock: (app.lasco.importer.ffi.ImporterLibrarySummary, String, String) -> Unit,
    onAddRemote: (app.lasco.importer.ffi.ImporterLibrarySummary, RemoteConfig) -> Unit,
    onRemove: (app.lasco.importer.ffi.ImporterLibrarySummary) -> Unit,
) {
    when (dialog) {
        null -> Unit
        is DestinationDialog.Unlock -> Dialog(onDismissRequest = onDismiss) {
            var username by remember(dialog.library.libraryId) { mutableStateOf(dialog.library.username.orEmpty()) }
            var password by remember(dialog.library.libraryId) { mutableStateOf("") }
            Surface(Modifier.widthIn(min = 420.dp, max = 620.dp), color = Panel) {
                Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    PageTitle("Unlock ${dialog.library.nickname}", "Enter the library credentials. They are used only to open the local importer setup.")
                    LascoField("Library username", username, { username = it })
                    LascoField("Library password", password, { password = it }, secure = true)
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        LascoButton("UNLOCK", { onUnlock(dialog.library, username, password) }, enabled = username.isNotBlank() && password.isNotBlank(), fillWidth = false)
                        LascoButton("CANCEL", onDismiss, primary = false, fillWidth = false)
                    }
                }
            }
        }
        is DestinationDialog.AddRemote -> Dialog(onDismissRequest = onDismiss) {
            var kind by remember(dialog.library.libraryId) { mutableStateOf(RemoteType.S3) }
            var name by remember(dialog.library.libraryId) { mutableStateOf("") }
            var endpoint by remember(dialog.library.libraryId) { mutableStateOf("") }
            var bucket by remember(dialog.library.libraryId) { mutableStateOf("") }
            var region by remember(dialog.library.libraryId) { mutableStateOf("") }
            var prefix by remember(dialog.library.libraryId) { mutableStateOf("") }
            var accessKey by remember(dialog.library.libraryId) { mutableStateOf("") }
            var secretKey by remember(dialog.library.libraryId) { mutableStateOf("") }
            var server by remember(dialog.library.libraryId) { mutableStateOf("") }
            var port by remember(dialog.library.libraryId) { mutableStateOf("445") }
            var share by remember(dialog.library.libraryId) { mutableStateOf("") }
            var username by remember(dialog.library.libraryId) { mutableStateOf("") }
            var password by remember(dialog.library.libraryId) { mutableStateOf("") }
            var domain by remember(dialog.library.libraryId) { mutableStateOf("") }
            val remote = when (kind) {
                RemoteType.S3 -> RemoteConfig.S3(name, endpoint, bucket, region, prefix, accessKey, secretKey)
                RemoteType.SMB -> RemoteConfig.Smb(name, server, port.toIntOrNull() ?: 0, share, prefix, username, password, domain.ifBlank { null })
                RemoteType.CLOUD -> error("Lasco Cloud remotes are configured when adding a library.")
            }
            val ready = when (kind) {
                RemoteType.S3 -> name.isNotBlank() && endpoint.isNotBlank() && bucket.isNotBlank() && region.isNotBlank() && accessKey.isNotBlank() && secretKey.isNotBlank()
                RemoteType.SMB -> name.isNotBlank() && server.isNotBlank() && port.toIntOrNull() in 1..65535 && share.isNotBlank() && username.isNotBlank() && password.isNotBlank()
                RemoteType.CLOUD -> false
            }
            Surface(Modifier.widthIn(min = 420.dp, max = 620.dp), color = Panel) {
                val remoteScrollState = rememberScrollState()
                Box {
                    Column(Modifier.verticalScroll(remoteScrollState).padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    PageTitle("Add remote", "This remote is initialized immediately. If initialization fails, Lasco removes the incomplete remote.")
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        LascoButton("S3", { kind = RemoteType.S3 }, primary = kind == RemoteType.S3, fillWidth = false)
                        LascoButton("SMB", { kind = RemoteType.SMB }, primary = kind == RemoteType.SMB, fillWidth = false)
                    }
                    LascoField("Remote name", name, { name = it })
                    when (kind) {
                        RemoteType.S3 -> {
                            LascoField("Endpoint URL", endpoint, { endpoint = it }); LascoField("Bucket", bucket, { bucket = it }); LascoField("Region", region, { region = it }); LascoField("Path prefix", prefix, { prefix = it }); LascoField("Access key", accessKey, { accessKey = it }); LascoField("Secret key", secretKey, { secretKey = it }, secure = true)
                        }
                        RemoteType.SMB -> {
                            LascoField("Server address", server, { server = it }); LascoField("Port", port, { port = it }); LascoField("Shared folder", share, { share = it }); LascoField("Path prefix", prefix, { prefix = it }); LascoField("SMB username", username, { username = it }); LascoField("SMB password", password, { password = it }, secure = true); LascoField("Domain or workgroup", domain, { domain = it })
                        }
                        RemoteType.CLOUD -> Unit
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        LascoButton("ADD REMOTE", { onAddRemote(dialog.library, remote) }, enabled = ready, fillWidth = false)
                        LascoButton("CANCEL", onDismiss, primary = false, fillWidth = false)
                    }
                    }
                    VerticalScrollbar(
                        adapter = rememberScrollbarAdapter(remoteScrollState),
                        modifier = Modifier.align(Alignment.CenterEnd).fillMaxHeight(),
                    )
                }
            }
        }
        is DestinationDialog.RemoveSetup -> Dialog(onDismissRequest = onDismiss) {
            Surface(Modifier.widthIn(min = 420.dp, max = 620.dp), color = Panel) {
                Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    PageTitle("Remove local importer setup?", "This removes only this computer's saved setup for ${dialog.library.nickname}. It does not delete the Lasco library or any remote data.")
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        LascoButton("REMOVE LOCAL SETUP", { onRemove(dialog.library) }, fillWidth = false)
                        LascoButton("CANCEL", onDismiss, primary = false, fillWidth = false)
                    }
                }
            }
        }
    }
}

@Composable
private fun SourcePicker(onTakeout: () -> Unit, onPhotos: () -> Unit) {
    PageTitle("Where are your photos now?", "Choose a source to open its dedicated setup. This selection continues immediately, with no Continue button on this screen.")
    Spacer(Modifier.height(24.dp))
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        Column(Modifier.widthIn(max = 460.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            LascoButton("GOOGLE TAKEOUT", onTakeout)
            if (System.getProperty("os.name").lowercase().contains("mac")) LascoButton("APPLE PHOTOS / ICLOUD", onPhotos)
        }
    }
}

@Composable
private fun ConnectionForm(
    type: RemoteType,
    nickname: String, setNickname: (String) -> Unit, libraryUser: String, setLibraryUser: (String) -> Unit, libraryPassword: String, setLibraryPassword: (String) -> Unit,
    remoteName: String, setRemoteName: (String) -> Unit, cloudEmail: String, setCloudEmail: (String) -> Unit, cloudPassword: String, setCloudPassword: (String) -> Unit,
    endpoint: String, setEndpoint: (String) -> Unit, bucket: String, setBucket: (String) -> Unit, region: String, setRegion: (String) -> Unit, prefix: String, setPrefix: (String) -> Unit, accessKey: String, setAccessKey: (String) -> Unit, secretKey: String, setSecretKey: (String) -> Unit,
    server: String, setServer: (String) -> Unit, port: String, setPort: (String) -> Unit, share: String, setShare: (String) -> Unit, smbUser: String, setSmbUser: (String) -> Unit, smbPassword: String, setSmbPassword: (String) -> Unit, domain: String, setDomain: (String) -> Unit, error: String?,
) {
    val title = when (type) {
        RemoteType.CLOUD -> "Connect Lasco Cloud"
        RemoteType.S3 -> "Connect an S3 remote"
        RemoteType.SMB -> "Connect an SMB remote"
    }
    PageTitle(title)
    Spacer(Modifier.height(20.dp))
    Column(Modifier.widthIn(max = 620.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        LascoField("Library nickname", nickname, setNickname, "family-library")
        LascoField("Library username", libraryUser, setLibraryUser)
        LascoField("Library password", libraryPassword, setLibraryPassword, secure = true)
        when (type) {
            RemoteType.CLOUD -> {
                LascoField("Cloud email", cloudEmail, setCloudEmail)
                LascoField("Cloud password", cloudPassword, setCloudPassword, secure = true)
            }
            RemoteType.S3 -> { LascoField("Remote name", remoteName, setRemoteName); LascoField("Endpoint URL", endpoint, setEndpoint); LascoField("Bucket", bucket, setBucket); LascoField("Region", region, setRegion); LascoField("Path prefix", prefix, setPrefix); LascoField("Access key", accessKey, setAccessKey); LascoField("Secret key", secretKey, setSecretKey, secure = true) }
            RemoteType.SMB -> { LascoField("Remote name", remoteName, setRemoteName); LascoField("Server address", server, setServer); LascoField("Port", port, setPort); LascoField("Shared folder", share, setShare); LascoField("Path prefix", prefix, setPrefix); LascoField("SMB username", smbUser, setSmbUser); LascoField("SMB password", smbPassword, setSmbPassword, secure = true); LascoField("Domain or workgroup", domain, setDomain) }
        }
        error?.let { ErrorMessage(it) }
    }
}

@Composable
private fun TakeoutPage(archives: List<String>, error: String?, chooseArchives: () -> Unit) {
    PageTitle("Import Google Takeout", "Select one or more Takeout ZIP archives. They remain in place and are read lazily in import chunks.")
    Spacer(Modifier.height(24.dp)); LascoButton("CHOOSE ZIP ARCHIVES", chooseArchives, modifier = Modifier.widthIn(max = 320.dp))
    archives.forEach { Text(it, color = InkSub, style = LascoMono, modifier = Modifier.padding(top = 12.dp)) }
    error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) }
}

@Composable
private fun PhotosPage(
    granted: Boolean,
    requesting: Boolean,
    error: String?,
    permissionDenied: Boolean,
    request: () -> Unit,
    openSettings: () -> Unit,
) {
    PageTitle("Import Apple Photos", "Allow Photos access to discover your library. iCloud-only originals download only into the staging directory when their chunk is imported.")
    Spacer(Modifier.height(24.dp))
    if (granted) {
        Text("PHOTOS ACCESS GRANTED", color = Good, style = LascoPixel, fontWeight = FontWeight.Bold)
    } else {
        LascoButton(if (requesting) "REQUESTING PHOTOS ACCESS…" else "ALLOW PHOTOS ACCESS", request, enabled = !requesting, modifier = Modifier.widthIn(max = 320.dp))
        if (permissionDenied) {
            Spacer(Modifier.height(12.dp))
            LascoButton("OPEN PHOTOS SETTINGS", openSettings, primary = false, modifier = Modifier.widthIn(max = 320.dp))
            Spacer(Modifier.height(8.dp))
            Text("Enable Lasco in Photos, then return here and allow access again.", color = InkSub, style = LascoBody)
        }
    }
    error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) }
}

@Composable
private fun ScanningPage(
    source: SourceType?,
    progress: DiscoveryProgress,
    error: String?,
    onRetry: () -> Unit,
    onChooseAnotherSource: () -> Unit,
) {
    val isPhotos = source == SourceType.PHOTOS
    PageTitle(if (isPhotos) "Scanning Apple Photos" else "Scanning Google Takeout")
    Spacer(Modifier.height(28.dp))
    if (error != null) {
        ErrorMessage(error)
        Spacer(Modifier.height(16.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            LascoButton("TRY SCAN AGAIN", onRetry, fillWidth = false)
            LascoButton("CHOOSE ANOTHER SOURCE", onChooseAnotherSource, primary = false, fillWidth = false)
        }
    } else if (isPhotos && progress.total > 0) {
        val fraction = (progress.completed.toFloat() / progress.total).coerceIn(0f, 1f)
        Text("${progress.completed} OF ${progress.total} PHOTOS SCANNED", color = Ink, style = LascoPixel)
        Spacer(Modifier.height(10.dp))
        Box(Modifier.widthIn(max = 620.dp).fillMaxWidth().height(16.dp).background(PlasterDeep).border(2.dp, Ink)) {
            Box(Modifier.fillMaxWidth(fraction).height(12.dp).background(Pink))
        }
    } else {
        Text(if (isPhotos) "CONNECTING TO YOUR PHOTOS LIBRARY…" else "READING TAKEOUT ARCHIVES…", color = InkSub, style = LascoPixel)
    }
}

@Composable
private fun RemoteSyncPage(
    progress: RemoteSyncProgress,
    syncing: Boolean,
    error: String?,
    ready: Boolean,
    onRetry: () -> Unit,
    onChooseAnotherLibrary: () -> Unit,
) {
    PageTitle("Syncing remotes")
    Spacer(Modifier.height(24.dp))
    progress.fetchedRemoteNames.forEach { name ->
        Text("Fetched remote $name", color = Good, style = LascoBody)
        Spacer(Modifier.height(8.dp))
    }
    when {
        syncing -> {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                CircularProgressIndicator(modifier = Modifier.widthIn(max = 22.dp), color = Pink, strokeWidth = 2.dp)
                Text(
                    progress.currentRemoteName?.let { "Fetching remote $it…" } ?: "Preparing remotes…",
                    color = InkSub,
                    style = LascoBody,
                )
            }
            if (progress.totalRemotes > 0) {
                Spacer(Modifier.height(12.dp))
                Text(
                    "${progress.fetchedRemoteNames.size} of ${progress.totalRemotes} remotes fetched",
                    color = InkMuted,
                    style = LascoLabel,
                )
            }
        }
        error != null -> {
            ErrorMessage(error)
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                LascoButton("TRY AGAIN", onRetry, fillWidth = false)
                LascoButton("CHOOSE ANOTHER LIBRARY", onChooseAnotherLibrary, primary = false, fillWidth = false)
            }
        }
        ready -> Text(
            "All remotes contain the same state!",
            color = Good,
            style = LascoBody,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun LibrarySummaryPage(source: SourceType?, archives: List<String>, plan: ImportPlan?) {
    PageTitle("Library summary")
    Spacer(Modifier.height(20.dp))
    Detail("SOURCE", if (source == SourceType.PHOTOS) "Apple Photos / iCloud" else "Google Takeout")
    if (source == SourceType.TAKEOUT) Detail("ARCHIVES", archives.size.toString())
    plan?.let {
        MediaCountSummary("APPLE PHOTOS LIBRARY", it.library)
        Spacer(Modifier.height(18.dp))
        ImportWorkSummary(it)
        it.remotes.forEach { remote ->
            Spacer(Modifier.height(18.dp))
            Column(
                Modifier.widthIn(max = 680.dp).fillMaxWidth().background(Color.White).border(2.dp, Ink).padding(16.dp),
            ) {
                Text(
                    "${remote.remoteName} ${importerRemoteTypeLabel(remote.remoteType)}",
                    color = Ink,
                    style = LascoLabel.copy(fontSize = 15.sp),
                    fontWeight = FontWeight.Bold,
                )
                Spacer(Modifier.height(8.dp))
                Text("MEDIA BLOBS", color = InkMuted, style = LascoLabel, fontWeight = FontWeight.Bold)
                Spacer(Modifier.height(8.dp))
                if (remote.alreadyThere.resourceCount > 0) {
                    MediaCountSummary("ALREADY THERE", remote.alreadyThere)
                    Spacer(Modifier.height(8.dp))
                }
                if (remote.toUpload.resourceCount > 0) {
                    MediaCountSummary("MISSING — TO BE UPLOADED", remote.toUpload, includeTotal = true)
                } else {
                    Text("No media blobs missing — nothing to upload.", color = Good, style = LascoBody)
                }
            }
        }
    }
}

/** The remote-sync gate has already fetched every operation log before this page can appear. */
@Composable
private fun ImportWorkSummary(plan: ImportPlan) {
    Column(
        Modifier.widthIn(max = 680.dp).fillMaxWidth().background(Color.White).border(2.dp, Ink).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("METADATA / OPERATION LOG", color = InkMuted, style = LascoLabel, fontWeight = FontWeight.Bold)
        Text("All remotes contain the same metadata state. Nothing to push.", color = Good, style = LascoBody)
        if (plan.metadataToAdd) {
            Text(
                "Apple Photos collection metadata will be added during import.",
                color = InkSub,
                style = LascoBody,
            )
        } else {
            Text("No Apple Photos metadata changes are needed.", color = InkSub, style = LascoBody)
        }
        if (!plan.hasMediaToUpload && !plan.metadataToAdd) {
            Text("Nothing to do — every remote already has this import.", color = Good, style = LascoBody)
        }
    }
}

@Composable
private fun MediaCountSummary(label: String, counts: app.lasco.importer.model.MediaCounts, includeTotal: Boolean = false) {
    Text(label, color = InkMuted, style = LascoLabel, fontWeight = FontWeight.Bold)
    Detail("PHOTOS", "${counts.photos} (+ ${counts.livePhotoVideos} Live Photos attached, + ${counts.aaeFiles} AAE files attached)")
    Detail("VIDEOS", counts.videos.toString())
    if (includeTotal) Detail("TOTAL", formatBytes(counts.bytes))
}

private val app.lasco.importer.model.MediaCounts.resourceCount: Int
    get() = photos + videos + livePhotoVideos + aaeFiles

@Composable
private fun UploadEstimatePage(
    plan: ImportPlan?,
    benchmarks: List<RemoteBenchmark>,
    benchmarking: Boolean,
    error: String?,
) {
    PageTitle("Expected upload time")
    Spacer(Modifier.height(24.dp))
    if (benchmarking) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            CircularProgressIndicator(modifier = Modifier.widthIn(max = 22.dp), color = Pink, strokeWidth = 2.dp)
            Text("Benchmarking upload speed…", color = InkSub, style = LascoBody)
        }
    } else if (error != null) {
        ErrorMessage(error)
    } else {
        val summaries = plan?.remotes.orEmpty().associateBy { it.remoteId }
        benchmarks.forEach { benchmark ->
            val remote = summaries[benchmark.remoteId] ?: return@forEach
            val seconds = if (benchmark.isolatedBytesPerSecond > 0) {
                (remote.toUpload.bytes + benchmark.isolatedBytesPerSecond - 1) / benchmark.isolatedBytesPerSecond
            } else {
                null
            }
            Text("${remote.remoteName} ${importerRemoteTypeLabel(remote.remoteType)}", color = Ink, style = LascoLabel.copy(fontSize = 15.sp), fontWeight = FontWeight.Bold)
            Detail("TO UPLOAD", formatBytes(remote.toUpload.bytes))
            Detail("BEST SPEED", "${formatRate(benchmark.isolatedBytesPerSecond)} at ${benchmark.selectedParallelism} parallel uploads")
            Detail("EXPECTED TIME", seconds?.let(::formatDuration) ?: "Unavailable")
            Spacer(Modifier.height(18.dp))
        }
    }
}

@Composable
private fun ImportPage(
    progress: ImportProgress,
    running: Boolean,
    error: String?,
    onStart: () -> Unit,
    onPause: () -> Unit,
    onBackToStart: () -> Unit,
) {
    PageTitle("Import")
    Spacer(Modifier.height(24.dp))
    Detail("STATUS", progress.detail.ifBlank { progress.state.name.lowercase().replaceFirstChar(Char::uppercase) })
    Detail("PROGRESS", "${progress.completedAssets} / ${progress.totalAssets} items")
    if (progress.state == ImportRunState.COMPLETE) {
        Text("IMPORT COMPLETE", color = Good, style = LascoPixel, fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 20.dp))
        Spacer(Modifier.height(16.dp))
        Text("This library setup is kept so you can import again later.", color = InkSub, style = LascoBody)
        Spacer(Modifier.height(12.dp))
        LascoButton("BACK TO START", onBackToStart, primary = false, fillWidth = false)
    } else {
        Row(Modifier.padding(top = 20.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            LascoButton(if (running) "IMPORTING…" else "START OR RESUME IMPORT", onStart, enabled = !running, fillWidth = false)
            LascoButton("PAUSE AFTER CURRENT BATCH", onPause, primary = false, enabled = running, fillWidth = false)
        }
    }
    error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) }
}

@Composable
private fun WizardFooter(page: Page, source: SourceType?, connecting: Boolean, connectEnabled: Boolean, discovering: Boolean, archivesReady: Boolean, photosReady: Boolean, benchmarking: Boolean, benchmarkReady: Boolean, remoteSyncReady: Boolean, librarySummaryHasWork: Boolean, librarySummaryHasMediaToUpload: Boolean, onBack: () -> Unit, onConnect: () -> Unit, onContinue: () -> Unit) {
    val picker = page == Page.DESTINATION || page == Page.SOURCE
    val connectPage = page in setOf(Page.CLOUD, Page.S3, Page.SMB)
    val scanning = page == Page.SCANNING || (page == Page.REMOTE_SYNC && !remoteSyncReady)
    val continueEnabled = page == Page.WELCOME || (page == Page.TAKEOUT && archivesReady) || (page == Page.PHOTOS && photosReady) || (page == Page.LIBRARY_SUMMARY && librarySummaryHasWork) || (page == Page.UPLOAD_ESTIMATE && benchmarkReady && !benchmarking) || (page == Page.REMOTE_SYNC && remoteSyncReady)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        if (page != Page.WELCOME && page != Page.CLOUD_SERVER && page != Page.IMPORT && !scanning) LascoButton("BACK", onBack, primary = false, fillWidth = false)
        Spacer(Modifier.weight(1f))
        if (connectPage) LascoButton(if (connecting) "CONNECTING…" else "CONNECT REMOTE", onConnect, enabled = connectEnabled && !connecting, fillWidth = false)
        if (page == Page.LIBRARY_SUMMARY && !librarySummaryHasWork) {
            Text("Nothing at all to import.", color = InkSub, style = LascoBody)
        } else if (!picker && !connectPage && !scanning && page != Page.IMPORT && (page != Page.UPLOAD_ESTIMATE || benchmarkReady)) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                val label = when {
                    discovering -> "DISCOVERING…"
                    page == Page.LIBRARY_SUMMARY && librarySummaryHasMediaToUpload -> "BENCHMARK UPLOAD SPEED"
                    page == Page.LIBRARY_SUMMARY -> "START IMPORT"
                    page == Page.UPLOAD_ESTIMATE -> "START IMPORT"
                    else -> "CONTINUE"
                }
                LascoButton(label, onContinue, enabled = continueEnabled && !discovering, fillWidth = false)
                if (page == Page.UPLOAD_ESTIMATE && benchmarkReady && source == SourceType.PHOTOS) {
                    Text("(It will not delete your iCloud files.)", color = InkSub, style = LascoBody.copy(fontSize = 12.sp), modifier = Modifier.padding(top = 6.dp))
                }
            }
        }
    }
}

@Composable
private fun PageTitle(title: String, text: String? = null) {
    Text(title, color = Ink, style = LascoHeading, fontWeight = FontWeight.Bold)
    text?.let {
        Spacer(Modifier.height(8.dp))
        Text(it, color = InkSub, style = LascoBody.copy(fontSize = 16.sp, lineHeight = 23.sp), modifier = Modifier.widthIn(max = 680.dp))
    }
}
@Composable private fun Detail(label: String, value: String) { Row(Modifier.padding(vertical = 5.dp)) { Text(label, color = InkMuted, style = LascoLabel, fontWeight = FontWeight.Bold, modifier = Modifier.widthIn(min = 120.dp)); Text(value, color = Ink, style = LascoBody.copy(fontSize = 14.sp)) } }
@Composable private fun ErrorMessage(text: String) { Text(text, color = Error, style = LascoBody.copy(fontSize = 14.sp), modifier = Modifier.fillMaxWidth().background(Error.copy(alpha = .08f)).border(1.dp, Error).padding(10.dp)) }

private fun formatDuration(seconds: Long): String = when {
    seconds < 60 -> "$seconds sec"
    seconds < 3_600 -> "${seconds / 60} min ${seconds % 60} sec"
    else -> "${seconds / 3_600} hr ${(seconds % 3_600) / 60} min"
}

@Composable
private fun LascoButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, primary: Boolean = true, enabled: Boolean = true, fillWidth: Boolean = true) {
    val background = if (enabled) {
        if (primary) Accent else PlasterDeep
    } else {
        PlasterDeep
    }
    val border = if (enabled) Ink else InkMuted
    val textColor = if (enabled) {
        if (primary) Color.White else Ink
    } else {
        InkMuted
    }
    Box(
        modifier.then(if (fillWidth) Modifier.fillMaxWidth() else Modifier)
            .background(background)
            .border(2.dp, border)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = textColor, style = LascoBody.copy(fontSize = 14.sp), fontWeight = FontWeight.Bold)
    }
}

/** Tab and Shift-Tab move between fields, matching standard desktop form behavior. */
@Composable
private fun LascoField(label: String, value: String, onValueChange: (String) -> Unit, placeholder: String = "", secure: Boolean = false) {
    var fieldValue by remember { mutableStateOf(TextFieldValue(value)) }
    val focusManager = LocalFocusManager.current
    LaunchedEffect(value) { if (value != fieldValue.text) fieldValue = TextFieldValue(value, TextRange(value.length)) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label.uppercase(), color = InkSub, style = LascoLabel, fontWeight = FontWeight.Bold)
        BasicTextField(
            value = fieldValue, onValueChange = { fieldValue = it; onValueChange(it.text) }, textStyle = LascoBody.copy(color = Ink), cursorBrush = SolidColor(Pink), visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
            modifier = Modifier.fillMaxWidth().onPreviewKeyEvent { event ->
                if (event.key == Key.Tab && event.type == KeyEventType.KeyDown) {
                    focusManager.moveFocus(if (event.isShiftPressed) FocusDirection.Previous else FocusDirection.Next)
                    true
                } else false
            }.background(Color.White).border(2.dp, Ink).padding(horizontal = 10.dp, vertical = 10.dp),
            decorationBox = { inner -> Box { if (fieldValue.text.isEmpty() && placeholder.isNotEmpty()) Text(placeholder, color = InkMuted, style = LascoBody); inner() } },
        )
    }
}

private fun chooseTakeoutZips(): List<String> {
    val dialog = FileDialog(null as Frame?, "Choose Google Takeout ZIP", FileDialog.LOAD)
    dialog.isMultipleMode = true; dialog.isVisible = true
    return dialog.files.map { Path.of(it.toURI()).toString() }
}

private fun openPhotosPrivacySettings() {
    check(System.getProperty("os.name").lowercase().contains("mac")) { "Photos settings are available only on macOS." }
    check(Desktop.isDesktopSupported() && Desktop.getDesktop().isSupported(Desktop.Action.BROWSE)) {
        "This desktop cannot open System Settings."
    }
    Desktop.getDesktop().browse(URI("x-apple.systempreferences:com.apple.preference.security?Privacy_Photos"))
}

private fun formatBytes(value: Long): String = when {
    value >= 1024L * 1024 * 1024 -> "%.1f GB".format(value / (1024.0 * 1024 * 1024))
    value >= 1024L * 1024 -> "%.1f MB".format(value / (1024.0 * 1024))
    else -> "$value B"
}

private fun formatRate(value: Long): String = "${formatBytes(value)}/s"
