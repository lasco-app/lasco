package app.lasco.importer

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
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
import androidx.compose.ui.window.application
import app.lasco.importer.ffi.ExistingLibraryConnector
import app.lasco.importer.ffi.ExistingRemote
import app.lasco.importer.ffi.LascoGateway
import app.lasco.importer.ffi.LibraryCredentials
import app.lasco.importer.engine.ImportCoordinator
import app.lasco.importer.model.ImportPlan
import app.lasco.importer.model.ImportProgress
import app.lasco.importer.model.ImportRunState
import app.lasco.importer.model.RemoteBenchmark
import app.lasco.importer.source.ApplePhotosReader
import app.lasco.importer.source.GoogleTakeoutReader
import app.lasco.importer.source.ImportSourceReader
import app.lasco.importer.persistence.ImportManifestStore
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
import uniffi.lasco_ffi.listLibraries

private enum class Page(val stage: Int) {
    WELCOME(0), DESTINATION(1), CLOUD(1), S3(1), SMB(1),
    SOURCE(2), TAKEOUT(2), PHOTOS(2), SCANNING(2), REVIEW(3), IMPORT(4),
}

private enum class RemoteType { CLOUD, S3, SMB }
private enum class SourceType { TAKEOUT, PHOTOS }
private data class ConnectionFailure(val remoteType: RemoteType, val message: String)
private data class DiscoveryFailure(val sourceType: SourceType, val message: String)
private data class DiscoveryProgress(val completed: Int = 0, val total: Int = 0)

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

fun main() = application {
    Window(onCloseRequest = ::exitApplication, title = "Lasco Desktop Importer") {
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
                    listLibraries(Path.of(System.getProperty("user.home"), ".lasco-desktop-importer").toString())
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
    var page by remember { mutableStateOf(Page.WELCOME) }
    var forward by remember { mutableStateOf(true) }
    var remoteType by remember { mutableStateOf<RemoteType?>(null) }
    var sourceType by remember { mutableStateOf<SourceType?>(null) }
    var nickname by remember { mutableStateOf("") }
    var libraryUser by remember { mutableStateOf("") }
    var libraryPassword by remember { mutableStateOf("") }
    var remoteName by remember { mutableStateOf("") }
    var cloudUrl by remember { mutableStateOf("https://cloud.getlasco.app") }
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
    var remoteNames by remember { mutableStateOf(emptyList<String>()) }
    var gateway by remember { mutableStateOf<LascoGateway?>(null) }
    var connecting by remember { mutableStateOf(false) }
    var connectionFailure by remember { mutableStateOf<ConnectionFailure?>(null) }
    var archives by remember { mutableStateOf(emptyList<String>()) }
    var photosAllowed by remember { mutableStateOf(false) }
    var requestingPhotos by remember { mutableStateOf(false) }
    var photosError by remember { mutableStateOf<String?>(null) }
    var photosPermissionDenied by remember { mutableStateOf(false) }
    var coordinator by remember { mutableStateOf<ImportCoordinator?>(null) }
    var sourceReader by remember { mutableStateOf<ImportSourceReader?>(null) }
    var importJobId by remember { mutableStateOf<String?>(null) }
    var importPlan by remember { mutableStateOf<ImportPlan?>(null) }
    var benchmarks by remember { mutableStateOf(emptyList<RemoteBenchmark>()) }
    var importProgress by remember { mutableStateOf(ImportProgress(ImportRunState.READY, 0, 0, 0, 0)) }
    var discovering by remember { mutableStateOf(false) }
    var discoveryFailure by remember { mutableStateOf<DiscoveryFailure?>(null) }
    var discoveryProgress by remember { mutableStateOf(DiscoveryProgress()) }
    var importRunning by remember { mutableStateOf(false) }
    var importError by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    fun go(to: Page) {
        forward = to.ordinal > page.ordinal
        page = to
    }

    fun connect() {
        val kind = remoteType ?: return
        connecting = true
        connectionFailure = null
        scope.launch {
            try {
                val remote = when (kind) {
                    RemoteType.CLOUD -> ExistingRemote.LascoCloud(remoteName.ifBlank { "Lasco Cloud" }, cloudUrl, cloudEmail, cloudPassword)
                    RemoteType.S3 -> ExistingRemote.S3(remoteName, endpoint, bucket, region, prefix, accessKey, secretKey)
                    RemoteType.SMB -> ExistingRemote.Smb(remoteName, server, port.toIntOrNull() ?: 0, share, prefix, smbUser, smbPassword, domain.ifBlank { null })
                }
                val connected = withContext(Dispatchers.IO) {
                    ExistingLibraryConnector.connect(
                        LibraryCredentials(nickname, libraryUser, libraryPassword), remote,
                        Path.of(System.getProperty("user.home"), ".lasco-desktop-importer"),
                    )
                }
                gateway?.close()
                gateway = connected
                remoteNames = withContext(Dispatchers.IO) { connected.remotes().map { it.name } }
                go(Page.SOURCE)
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
                    val appData = Path.of(System.getProperty("user.home"), ".lasco-desktop-importer")
                    val newCoordinator = ImportCoordinator(
                        ImportManifestStore(appData.resolve("imports")),
                        connectedGateway,
                        appData.resolve("staging"),
                    )
                    val (jobId, plan) = newCoordinator.discover(reader)
                    Triple(reader, newCoordinator, Triple(jobId, plan, newCoordinator.benchmark(connectedGateway.remotes())))
                }
                sourceReader = prepared.first
                coordinator = prepared.second
                importJobId = prepared.third.first
                importPlan = prepared.third.second
                benchmarks = prepared.third.third
                go(Page.REVIEW)
            } catch (failure: Throwable) {
                discoveryFailure = DiscoveryFailure(
                    sourceType = selectedSource,
                    message = failure.message?.ifBlank { null } ?: "Could not scan this source.",
                )
                go(if (selectedSource == SourceType.PHOTOS) Page.PHOTOS else Page.TAKEOUT)
            } finally {
                progressPolling?.cancel()
                discovering = false
            }
        }
    }

    fun startImport() {
        if (importProgress.state == ImportRunState.COMPLETE) return
        val activeCoordinator = coordinator ?: return
        val jobId = importJobId ?: return
        val reader = sourceReader ?: return
        importRunning = true
        importError = null
        go(Page.IMPORT)
        scope.launch {
            try {
                withContext(Dispatchers.IO) { activeCoordinator.startOrResume(jobId, reader, benchmarks) }
            } catch (failure: Throwable) {
                importError = failure.message?.ifBlank { null } ?: "The import stopped unexpectedly. You can fix the issue and resume."
            } finally { importRunning = false }
        }
    }

    LaunchedEffect(coordinator) {
        coordinator?.progress?.collect { importProgress = it }
    }

    Column(Modifier.fillMaxSize().background(Plaster).padding(horizontal = 32.dp, vertical = 24.dp)) {
        ProgressHeader(page.stage, remoteNames)
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
                Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(28.dp)) {
                    when (current) {
                        Page.WELCOME -> WelcomePage()
                        Page.DESTINATION -> DestinationPicker(
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
                            showCloudUrl = System.getProperty("lasco.importer.release") == "false",
                            nickname = nickname, setNickname = { nickname = it }, libraryUser = libraryUser, setLibraryUser = { libraryUser = it }, libraryPassword = libraryPassword, setLibraryPassword = { libraryPassword = it },
                            remoteName = remoteName, setRemoteName = { remoteName = it }, cloudUrl = cloudUrl, setCloudUrl = { cloudUrl = it }, cloudEmail = cloudEmail, setCloudEmail = { cloudEmail = it }, cloudPassword = cloudPassword, setCloudPassword = { cloudPassword = it },
                            endpoint = endpoint, setEndpoint = { endpoint = it }, bucket = bucket, setBucket = { bucket = it }, region = region, setRegion = { region = it }, prefix = prefix, setPrefix = { prefix = it }, accessKey = accessKey, setAccessKey = { accessKey = it }, secretKey = secretKey, setSecretKey = { secretKey = it },
                            server = server, setServer = { server = it }, port = port, setPort = { port = it }, share = share, setShare = { share = it }, smbUser = smbUser, setSmbUser = { smbUser = it }, smbPassword = smbPassword, setSmbPassword = { smbPassword = it }, domain = domain, setDomain = { domain = it },
                            error = connectionFailure?.takeIf { it.remoteType == (remoteType ?: RemoteType.CLOUD) }?.message,
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
                                        if (!photosAllowed) {
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
                        Page.SCANNING -> ScanningPage(sourceType, discoveryProgress)
                        Page.REVIEW -> ReviewPage(sourceType, archives, remoteNames, importPlan, benchmarks, importError)
                        Page.IMPORT -> ImportPage(importProgress, importRunning, importError, onStart = ::startImport, onPause = { importJobId?.let { coordinator?.requestPause(it) } })
                    }
                }
            }
        }
        Spacer(Modifier.height(16.dp))
        WizardFooter(
            page = page, source = sourceType, connecting = connecting, discovering = discovering, archivesReady = archives.isNotEmpty(), photosReady = photosAllowed,
            onBack = {
                when (page) {
                    Page.DESTINATION -> go(Page.WELCOME)
                    Page.CLOUD, Page.S3, Page.SMB -> {
                        connectionFailure = null
                        go(Page.DESTINATION)
                    }
                    Page.SOURCE -> go(Page.DESTINATION)
                    Page.TAKEOUT, Page.PHOTOS -> go(Page.SOURCE)
                    Page.SCANNING -> Unit
                    Page.REVIEW -> go(if (sourceType == SourceType.PHOTOS) Page.PHOTOS else Page.TAKEOUT)
                    Page.IMPORT -> go(Page.REVIEW)
                    Page.WELCOME -> Unit
                }
            },
            onConnect = ::connect,
            onContinue = { when (page) {
                Page.WELCOME -> go(Page.DESTINATION)
                Page.REVIEW -> startImport()
                else -> discover()
            } },
        )
    }
}

@Composable
private fun ProgressHeader(stage: Int, remotes: List<String>) {
    Text("LASCO", color = Ink, style = LascoHeading.copy(fontSize = 30.sp), fontWeight = FontWeight.Black, letterSpacing = 2.sp)
    Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        listOf("1. HOW IT WORKS", "2. DESTINATION", "3. SOURCE", "4. REVIEW", "5. IMPORT").forEachIndexed { index, label ->
            Text(label, color = if (index == stage) Pink else InkMuted, style = LascoLabel.copy(fontSize = 16.sp), fontWeight = FontWeight.Bold)
        }
    }
    if (remotes.isNotEmpty()) {
        Text("CONNECTED: ${remotes.joinToString()}", color = Good, style = LascoLabel.copy(fontSize = 12.sp), fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 7.dp))
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
            HowItWorksItem("4", "Review the import, then start. You can safely pause and resume it.")
        }
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
private fun DestinationPicker(onCloud: () -> Unit, onS3: () -> Unit, onSmb: () -> Unit) {
    PageTitle("Import photos into Lasco", "Connect an existing Lasco library. Imports run in resumable chunks and send each chunk to every connected remote.")
    Spacer(Modifier.height(24.dp))
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        Column(Modifier.widthIn(max = 460.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            LascoButton("LASCO CLOUD", onCloud)
            LascoButton("S3-COMPATIBLE STORAGE", onS3)
            LascoButton("SMB NETWORK SHARE", onSmb, primary = false)
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
            if (System.getProperty("os.name").lowercase().contains("mac")) LascoButton("APPLE PHOTOS / ICLOUD", onPhotos, primary = false)
        }
    }
}

@Composable
private fun ConnectionForm(
    type: RemoteType,
    showCloudUrl: Boolean,
    nickname: String, setNickname: (String) -> Unit, libraryUser: String, setLibraryUser: (String) -> Unit, libraryPassword: String, setLibraryPassword: (String) -> Unit,
    remoteName: String, setRemoteName: (String) -> Unit, cloudUrl: String, setCloudUrl: (String) -> Unit, cloudEmail: String, setCloudEmail: (String) -> Unit, cloudPassword: String, setCloudPassword: (String) -> Unit,
    endpoint: String, setEndpoint: (String) -> Unit, bucket: String, setBucket: (String) -> Unit, region: String, setRegion: (String) -> Unit, prefix: String, setPrefix: (String) -> Unit, accessKey: String, setAccessKey: (String) -> Unit, secretKey: String, setSecretKey: (String) -> Unit,
    server: String, setServer: (String) -> Unit, port: String, setPort: (String) -> Unit, share: String, setShare: (String) -> Unit, smbUser: String, setSmbUser: (String) -> Unit, smbPassword: String, setSmbPassword: (String) -> Unit, domain: String, setDomain: (String) -> Unit, error: String?,
) {
    val title = when (type) {
        RemoteType.CLOUD -> "Connect Lasco Cloud"
        RemoteType.S3 -> "Connect an S3 remote"
        RemoteType.SMB -> "Connect an SMB remote"
    }
    PageTitle(title, "Password and secret-key fields are protected while typing. Connect adds the remote through the existing Lasco FFI.")
    Spacer(Modifier.height(20.dp))
    Column(Modifier.widthIn(max = 620.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        LascoField("Library nickname", nickname, setNickname, "family-library")
        LascoField("Library username", libraryUser, setLibraryUser)
        LascoField("Library password", libraryPassword, setLibraryPassword, secure = true)
        when (type) {
            RemoteType.CLOUD -> {
                if (showCloudUrl) LascoField("Cloud URL", cloudUrl, setCloudUrl)
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
private fun ScanningPage(source: SourceType?, progress: DiscoveryProgress) {
    val isPhotos = source == SourceType.PHOTOS
    PageTitle(
        if (isPhotos) "Scanning Apple Photos" else "Scanning Google Takeout",
        if (isPhotos) "Reading your library structure and metadata. iCloud originals are not downloaded until import starts."
        else "Reading archive contents and metadata. Your ZIP files remain unchanged.",
    )
    Spacer(Modifier.height(28.dp))
    if (isPhotos && progress.total > 0) {
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
private fun ReviewPage(source: SourceType?, archives: List<String>, remotes: List<String>, plan: ImportPlan?, benchmarks: List<RemoteBenchmark>, error: String?) {
    PageTitle("Library summary", "Review the media and destinations before importing.")
    Spacer(Modifier.height(20.dp))
    Detail("DESTINATION", remotes.joinToString().ifBlank { "No remote connected" })
    Detail("SOURCE", if (source == SourceType.PHOTOS) "Apple Photos / iCloud" else "Google Takeout")
    if (source == SourceType.TAKEOUT) Detail("ARCHIVES", archives.size.toString())
    plan?.let {
        Detail("MEDIA TO UPLOAD", "${it.candidates} · ${formatBytes(it.candidatesBytes)}")
    }
    benchmarks.forEach { benchmark ->
        Detail("${benchmark.remoteName.uppercase()} UPLOAD", "${formatRate(benchmark.isolatedBytesPerSecond)} at ${benchmark.selectedParallelism} parallel uploads")
    }
    error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) }
}

@Composable
private fun ImportPage(progress: ImportProgress, running: Boolean, error: String?, onStart: () -> Unit, onPause: () -> Unit) {
    PageTitle("Import", "Your selected media will be imported. You can pause after the current batch and close the app safely.")
    Spacer(Modifier.height(24.dp))
    Detail("STATUS", progress.detail.ifBlank { progress.state.name.lowercase().replaceFirstChar(Char::uppercase) })
    Detail("PROGRESS", "${progress.completedAssets} / ${progress.totalAssets} items")
    if (progress.state == ImportRunState.COMPLETE) {
        Text("IMPORT COMPLETE", color = Good, style = LascoPixel, fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 20.dp))
    } else {
        Row(Modifier.padding(top = 20.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            LascoButton(if (running) "IMPORTING…" else "START OR RESUME IMPORT", onStart, enabled = !running, fillWidth = false)
            LascoButton("PAUSE AFTER CURRENT BATCH", onPause, primary = false, enabled = running, fillWidth = false)
        }
    }
    error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) }
}

@Composable
private fun WizardFooter(page: Page, source: SourceType?, connecting: Boolean, discovering: Boolean, archivesReady: Boolean, photosReady: Boolean, onBack: () -> Unit, onConnect: () -> Unit, onContinue: () -> Unit) {
    val picker = page == Page.DESTINATION || page == Page.SOURCE
    val connectPage = page in setOf(Page.CLOUD, Page.S3, Page.SMB)
    val scanning = page == Page.SCANNING
    val continueEnabled = page == Page.WELCOME || (page == Page.TAKEOUT && archivesReady) || (page == Page.PHOTOS && photosReady) || page == Page.REVIEW
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        if (page != Page.WELCOME && !scanning) LascoButton("BACK", onBack, primary = false, fillWidth = false)
        Spacer(Modifier.weight(1f))
        if (connectPage) LascoButton(if (connecting) "CONNECTING…" else "CONNECT REMOTE", onConnect, enabled = !connecting, fillWidth = false)
        if (!picker && !connectPage && !scanning && page != Page.IMPORT) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                LascoButton(if (discovering) "DISCOVERING…" else if (page == Page.REVIEW) "START IMPORT" else "CONTINUE", onContinue, enabled = continueEnabled && !discovering, fillWidth = false)
                if (page == Page.REVIEW && source == SourceType.PHOTOS) {
                    Text("(It will not delete your iCloud files.)", color = InkSub, style = LascoBody.copy(fontSize = 12.sp), modifier = Modifier.padding(top = 6.dp))
                }
            }
        }
    }
}

@Composable private fun PageTitle(title: String, text: String) { Text(title, color = Ink, style = LascoHeading, fontWeight = FontWeight.Bold); Spacer(Modifier.height(8.dp)); Text(text, color = InkSub, style = LascoBody.copy(fontSize = 16.sp, lineHeight = 23.sp), modifier = Modifier.widthIn(max = 680.dp)) }
@Composable private fun Detail(label: String, value: String) { Row(Modifier.padding(vertical = 5.dp)) { Text(label, color = InkMuted, style = LascoLabel, fontWeight = FontWeight.Bold, modifier = Modifier.widthIn(min = 120.dp)); Text(value, color = Ink, style = LascoBody.copy(fontSize = 14.sp)) } }
@Composable private fun ErrorMessage(text: String) { Text(text, color = Error, style = LascoBody.copy(fontSize = 14.sp), modifier = Modifier.fillMaxWidth().background(Error.copy(alpha = .08f)).border(1.dp, Error).padding(10.dp)) }

@Composable
private fun LascoButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, primary: Boolean = true, enabled: Boolean = true, fillWidth: Boolean = true) {
    Box(modifier.then(if (fillWidth) Modifier.fillMaxWidth() else Modifier).background(if (primary) Accent else PlasterDeep).border(2.dp, Ink).clickable(enabled = enabled, onClick = onClick).padding(horizontal = 20.dp, vertical = 12.dp), contentAlignment = Alignment.Center) {
        Text(label, color = if (primary) Color.White else Ink, style = LascoBody.copy(fontSize = 14.sp), fontWeight = FontWeight.Bold)
    }
}

/** Tab is data in importer fields; it must not move focus to the next field. */
@Composable
private fun LascoField(label: String, value: String, onValueChange: (String) -> Unit, placeholder: String = "", secure: Boolean = false) {
    var fieldValue by remember { mutableStateOf(TextFieldValue(value)) }
    LaunchedEffect(value) { if (value != fieldValue.text) fieldValue = TextFieldValue(value, TextRange(value.length)) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label.uppercase(), color = InkSub, style = LascoLabel, fontWeight = FontWeight.Bold)
        BasicTextField(
            value = fieldValue, onValueChange = { fieldValue = it; onValueChange(it.text) }, textStyle = LascoBody.copy(color = Ink), cursorBrush = SolidColor(Pink), visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
            modifier = Modifier.fillMaxWidth().onPreviewKeyEvent { event ->
                if (event.key == Key.Tab && event.type == KeyEventType.KeyDown) {
                    val selection = fieldValue.selection
                    val updated = fieldValue.text.replaceRange(selection.start, selection.end, "\t")
                    fieldValue = TextFieldValue(updated, TextRange(selection.start + 1)); onValueChange(updated); true
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
