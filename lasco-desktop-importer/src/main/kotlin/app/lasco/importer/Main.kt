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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.platform.Font
import androidx.compose.ui.text.input.TextFieldValue
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
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.collect
import java.awt.FileDialog
import java.awt.Frame
import java.nio.file.Path
import uniffi.lasco_ffi.listLibraries

private enum class Page(val stage: Int) {
    DESTINATION(0), CLOUD(0), REMOTE_TYPE(0), S3(0), SMB(0), LOCAL(0),
    SOURCE(1), TAKEOUT(1), PHOTOS(1), REVIEW(2), IMPORT(3),
}

private enum class RemoteType { CLOUD, S3, SMB, LOCAL }
private enum class SourceType { TAKEOUT, PHOTOS }

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
private val LascoHeading = TextStyle(fontFamily = Jersey10, fontSize = 21.sp)
private val LascoBody = TextStyle(fontFamily = SpaceGrotesk, fontSize = 13.sp)
private val LascoLabel = TextStyle(fontFamily = Jersey10, fontSize = 10.sp, letterSpacing = 1.sp)
private val LascoPixel = TextStyle(fontFamily = VT323, fontSize = 12.sp)
private val LascoMono = TextStyle(fontFamily = JetBrainsMono, fontSize = 11.sp)

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
    var page by remember { mutableStateOf(Page.DESTINATION) }
    var forward by remember { mutableStateOf(true) }
    var remoteType by remember { mutableStateOf<RemoteType?>(null) }
    var sourceType by remember { mutableStateOf<SourceType?>(null) }
    var nickname by remember { mutableStateOf("") }
    var libraryUser by remember { mutableStateOf("") }
    var libraryPassword by remember { mutableStateOf("") }
    var remoteName by remember { mutableStateOf("") }
    var cloudUrl by remember { mutableStateOf("https://api.lasco.app") }
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
    var localPath by remember { mutableStateOf("") }
    var remoteNames by remember { mutableStateOf(emptyList<String>()) }
    var gateway by remember { mutableStateOf<LascoGateway?>(null) }
    var connecting by remember { mutableStateOf(false) }
    var connectionError by remember { mutableStateOf<String?>(null) }
    var archives by remember { mutableStateOf(emptyList<String>()) }
    var photosAllowed by remember { mutableStateOf(false) }
    var requestingPhotos by remember { mutableStateOf(false) }
    var photosError by remember { mutableStateOf<String?>(null) }
    var coordinator by remember { mutableStateOf<ImportCoordinator?>(null) }
    var sourceReader by remember { mutableStateOf<ImportSourceReader?>(null) }
    var importJobId by remember { mutableStateOf<String?>(null) }
    var importPlan by remember { mutableStateOf<ImportPlan?>(null) }
    var benchmarks by remember { mutableStateOf(emptyList<RemoteBenchmark>()) }
    var importProgress by remember { mutableStateOf(ImportProgress(ImportRunState.READY, 0, 0, 0, 0)) }
    var discovering by remember { mutableStateOf(false) }
    var importRunning by remember { mutableStateOf(false) }
    var importError by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    fun go(to: Page) {
        forward = to.stage >= page.stage
        page = to
    }

    fun connect() {
        val kind = remoteType ?: return
        connecting = true
        connectionError = null
        scope.launch {
            try {
                val remote = when (kind) {
                    RemoteType.CLOUD -> ExistingRemote.LascoCloud(remoteName.ifBlank { "Lasco Cloud" }, cloudUrl, cloudEmail, cloudPassword)
                    RemoteType.S3 -> ExistingRemote.S3(remoteName, endpoint, bucket, region, prefix, accessKey, secretKey)
                    RemoteType.SMB -> ExistingRemote.Smb(remoteName, server, port.toIntOrNull() ?: 0, share, prefix, smbUser, smbPassword, domain.ifBlank { null })
                    RemoteType.LOCAL -> ExistingRemote.FixedPath(remoteName, localPath)
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
                connectionError = failure.message?.ifBlank { null } ?: "Could not connect to this remote. Check the values and try again."
            } finally {
                connecting = false
            }
        }
    }

    fun discover() {
        val connectedGateway = gateway ?: return
        val selectedSource = sourceType ?: return
        discovering = true
        importError = null
        scope.launch {
            try {
                val prepared = withContext(Dispatchers.IO) {
                    val appData = Path.of(System.getProperty("user.home"), ".lasco-desktop-importer")
                    val reader = when (selectedSource) {
                        SourceType.TAKEOUT -> GoogleTakeoutReader(archives.map(Path::of))
                        SourceType.PHOTOS -> ApplePhotosReader()
                    }
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
                importError = failure.message?.ifBlank { null } ?: "Could not scan this source."
            } finally { discovering = false }
        }
    }

    fun startImport() {
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
                        Page.DESTINATION -> DestinationPicker(
                            onCloud = { remoteType = RemoteType.CLOUD; remoteName = "Lasco Cloud"; go(Page.CLOUD) },
                            onStorage = { go(Page.REMOTE_TYPE) },
                        )
                        Page.REMOTE_TYPE -> RemotePicker(
                            onS3 = { remoteType = RemoteType.S3; go(Page.S3) },
                            onSmb = { remoteType = RemoteType.SMB; go(Page.SMB) },
                            onLocal = { remoteType = RemoteType.LOCAL; go(Page.LOCAL) },
                        )
                        Page.CLOUD, Page.S3, Page.SMB, Page.LOCAL -> ConnectionForm(
                            type = remoteType ?: RemoteType.CLOUD,
                            nickname = nickname, setNickname = { nickname = it }, libraryUser = libraryUser, setLibraryUser = { libraryUser = it }, libraryPassword = libraryPassword, setLibraryPassword = { libraryPassword = it },
                            remoteName = remoteName, setRemoteName = { remoteName = it }, cloudUrl = cloudUrl, setCloudUrl = { cloudUrl = it }, cloudEmail = cloudEmail, setCloudEmail = { cloudEmail = it }, cloudPassword = cloudPassword, setCloudPassword = { cloudPassword = it },
                            endpoint = endpoint, setEndpoint = { endpoint = it }, bucket = bucket, setBucket = { bucket = it }, region = region, setRegion = { region = it }, prefix = prefix, setPrefix = { prefix = it }, accessKey = accessKey, setAccessKey = { accessKey = it }, secretKey = secretKey, setSecretKey = { secretKey = it },
                            server = server, setServer = { server = it }, port = port, setPort = { port = it }, share = share, setShare = { share = it }, smbUser = smbUser, setSmbUser = { smbUser = it }, smbPassword = smbPassword, setSmbPassword = { smbPassword = it }, domain = domain, setDomain = { domain = it }, localPath = localPath, setLocalPath = { localPath = it }, error = connectionError,
                        )
                        Page.SOURCE -> SourcePicker(
                            onTakeout = { sourceType = SourceType.TAKEOUT; go(Page.TAKEOUT) },
                            onPhotos = { sourceType = SourceType.PHOTOS; go(Page.PHOTOS) },
                        )
                        Page.TAKEOUT -> TakeoutPage(archives) { archives = chooseTakeoutZips() }
                        Page.PHOTOS -> PhotosPage(photosAllowed, requestingPhotos, photosError) {
                            photosError = null
                            requestingPhotos = true
                            scope.launch {
                                try {
                                    photosAllowed = withContext(Dispatchers.IO) {
                                        ApplePhotosReader().let { if (it.hasPermission()) true else it.requestPermission() }
                                    }
                                    if (!photosAllowed) photosError = "Photos access was not granted. Allow Lasco in System Settings, then try again."
                                } catch (failure: Throwable) {
                                    photosError = failure.message?.ifBlank { null } ?: "Could not request Photos access."
                                } finally { requestingPhotos = false }
                            }
                        }
                        Page.REVIEW -> ReviewPage(sourceType, archives, remoteNames, importPlan, benchmarks, importError)
                        Page.IMPORT -> ImportPage(importProgress, importRunning, importError, onStart = ::startImport, onPause = { importJobId?.let { coordinator?.requestPause(it) } })
                    }
                }
            }
        }
        Spacer(Modifier.height(16.dp))
        WizardFooter(
            page = page, connecting = connecting, discovering = discovering, archivesReady = archives.isNotEmpty(), photosReady = photosAllowed,
            onBack = {
                when (page) {
                    Page.CLOUD -> go(Page.DESTINATION)
                    Page.REMOTE_TYPE -> go(Page.DESTINATION)
                    Page.S3, Page.SMB, Page.LOCAL -> go(Page.REMOTE_TYPE)
                    Page.SOURCE -> go(Page.DESTINATION)
                    Page.TAKEOUT, Page.PHOTOS -> go(Page.SOURCE)
                    Page.REVIEW -> go(if (sourceType == SourceType.PHOTOS) Page.PHOTOS else Page.TAKEOUT)
                    Page.IMPORT -> go(Page.REVIEW)
                    Page.DESTINATION -> Unit
                }
            },
            onConnect = ::connect,
            onContinue = { if (page == Page.REVIEW) startImport() else discover() },
        )
    }
}

@Composable
private fun ProgressHeader(stage: Int, remotes: List<String>) {
    Text("LASCO", color = Ink, style = LascoHeading, fontWeight = FontWeight.Black, letterSpacing = 2.sp)
    Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        listOf("1. DESTINATION", "2. SOURCE", "3. REVIEW", "4. IMPORT").forEachIndexed { index, label ->
            Text(label, color = if (index == stage) Accent else InkMuted, style = LascoLabel, fontWeight = FontWeight.Bold)
        }
    }
    Text(if (remotes.isEmpty()) "NO REMOTE CONNECTED" else "CONNECTED: ${remotes.joinToString()}", color = if (remotes.isEmpty()) InkMuted else Good, style = LascoLabel, fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 7.dp))
}

@Composable
private fun DestinationPicker(onCloud: () -> Unit, onStorage: () -> Unit) {
    PageTitle("Where should your photos go?", "Connect the destination first. There is no default remote: the importer connects one explicitly before it reads a source.")
    Spacer(Modifier.height(24.dp))
    Column(Modifier.widthIn(max = 460.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        LascoButton("LASCO CLOUD", onCloud)
        LascoButton("S3, SMB, OR LOCAL STORAGE", onStorage, primary = false)
    }
}

@Composable
private fun RemotePicker(onS3: () -> Unit, onSmb: () -> Unit, onLocal: () -> Unit) {
    PageTitle("Add a storage remote", "Choose the type of remote that contains the existing Lasco library.")
    Spacer(Modifier.height(24.dp))
    Column(Modifier.widthIn(max = 460.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        LascoButton("S3-COMPATIBLE STORAGE", onS3)
        LascoButton("SMB NETWORK SHARE", onSmb, primary = false)
        LascoButton("LOCAL FOLDER", onLocal, primary = false)
    }
}

@Composable
private fun SourcePicker(onTakeout: () -> Unit, onPhotos: () -> Unit) {
    PageTitle("Where are your photos now?", "Choose a source to open its dedicated setup. This selection continues immediately, with no Continue button on this screen.")
    Spacer(Modifier.height(24.dp))
    Column(Modifier.widthIn(max = 460.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        LascoButton("GOOGLE TAKEOUT", onTakeout)
        if (System.getProperty("os.name").lowercase().contains("mac")) LascoButton("APPLE PHOTOS / ICLOUD", onPhotos, primary = false)
    }
}

@Composable
private fun ConnectionForm(
    type: RemoteType,
    nickname: String, setNickname: (String) -> Unit, libraryUser: String, setLibraryUser: (String) -> Unit, libraryPassword: String, setLibraryPassword: (String) -> Unit,
    remoteName: String, setRemoteName: (String) -> Unit, cloudUrl: String, setCloudUrl: (String) -> Unit, cloudEmail: String, setCloudEmail: (String) -> Unit, cloudPassword: String, setCloudPassword: (String) -> Unit,
    endpoint: String, setEndpoint: (String) -> Unit, bucket: String, setBucket: (String) -> Unit, region: String, setRegion: (String) -> Unit, prefix: String, setPrefix: (String) -> Unit, accessKey: String, setAccessKey: (String) -> Unit, secretKey: String, setSecretKey: (String) -> Unit,
    server: String, setServer: (String) -> Unit, port: String, setPort: (String) -> Unit, share: String, setShare: (String) -> Unit, smbUser: String, setSmbUser: (String) -> Unit, smbPassword: String, setSmbPassword: (String) -> Unit, domain: String, setDomain: (String) -> Unit,
    localPath: String, setLocalPath: (String) -> Unit, error: String?,
) {
    PageTitle("Connect ${if (type == RemoteType.CLOUD) "Lasco Cloud" else "an existing library"}", "All passwords use the standard text field, as requested. Connect adds the remote through the existing Lasco FFI.")
    Spacer(Modifier.height(20.dp))
    Column(Modifier.widthIn(max = 620.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        LascoField("Library nickname", nickname, setNickname, "family-library")
        LascoField("Library username", libraryUser, setLibraryUser)
        LascoField("Library password", libraryPassword, setLibraryPassword)
        when (type) {
            RemoteType.CLOUD -> { LascoField("Cloud URL", cloudUrl, setCloudUrl); LascoField("Cloud email", cloudEmail, setCloudEmail); LascoField("Cloud password", cloudPassword, setCloudPassword) }
            RemoteType.S3 -> { LascoField("Remote name", remoteName, setRemoteName); LascoField("Endpoint URL", endpoint, setEndpoint); LascoField("Bucket", bucket, setBucket); LascoField("Region", region, setRegion); LascoField("Path prefix", prefix, setPrefix); LascoField("Access key", accessKey, setAccessKey); LascoField("Secret key", secretKey, setSecretKey) }
            RemoteType.SMB -> { LascoField("Remote name", remoteName, setRemoteName); LascoField("Server address", server, setServer); LascoField("Port", port, setPort); LascoField("Shared folder", share, setShare); LascoField("Path prefix", prefix, setPrefix); LascoField("SMB username", smbUser, setSmbUser); LascoField("SMB password", smbPassword, setSmbPassword); LascoField("Domain or workgroup", domain, setDomain) }
            RemoteType.LOCAL -> { LascoField("Remote name", remoteName, setRemoteName); LascoField("Library folder", localPath, setLocalPath, "/Volumes/Archive/Lasco") }
        }
        error?.let { ErrorMessage(it) }
    }
}

@Composable
private fun TakeoutPage(archives: List<String>, chooseArchives: () -> Unit) {
    PageTitle("Import Google Takeout", "Select one or more Takeout ZIP archives. They remain in place and are read lazily in import chunks.")
    Spacer(Modifier.height(24.dp)); LascoButton("CHOOSE ZIP ARCHIVES", chooseArchives, modifier = Modifier.widthIn(max = 320.dp))
    archives.forEach { Text(it, color = InkSub, style = LascoMono, modifier = Modifier.padding(top = 12.dp)) }
}

@Composable
private fun PhotosPage(granted: Boolean, requesting: Boolean, error: String?, request: () -> Unit) {
    PageTitle("Import Apple Photos", "Allow Photos access to discover your library. iCloud-only originals download only into the staging directory when their chunk is imported.")
    Spacer(Modifier.height(24.dp))
    if (granted) Text("PHOTOS ACCESS GRANTED", color = Good, style = LascoPixel, fontWeight = FontWeight.Bold)
    else { LascoButton(if (requesting) "REQUESTING PHOTOS ACCESS…" else "ALLOW PHOTOS ACCESS", request, enabled = !requesting, modifier = Modifier.widthIn(max = 320.dp)); error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) } }
}

@Composable
private fun ReviewPage(source: SourceType?, archives: List<String>, remotes: List<String>, plan: ImportPlan?, benchmarks: List<RemoteBenchmark>, error: String?) {
    PageTitle("Ready to review", "The importer will scan once, preserve source metadata and albums, then fan each chunk out to every connected remote.")
    Spacer(Modifier.height(20.dp))
    Detail("DESTINATION", remotes.joinToString().ifBlank { "No remote connected" })
    Detail("SOURCE", if (source == SourceType.PHOTOS) "Apple Photos / iCloud" else "Google Takeout")
    if (source == SourceType.TAKEOUT) Detail("ARCHIVES", archives.size.toString())
    plan?.let {
        Detail("CANDIDATES", "${it.candidates} · ${formatBytes(it.candidatesBytes)}")
        Detail("CHUNK SIZE", "${it.chunkSize} items")
    }
    benchmarks.forEach { benchmark ->
        Detail("${benchmark.remoteName.uppercase()} UPLOAD", "${formatRate(benchmark.isolatedBytesPerSecond)} at ${benchmark.selectedParallelism} parallel uploads")
    }
    Text("Exact duplicates are determined by the Lasco core after staging and hashing, never by filename. The import manifest records completed source IDs, so closing and resuming does not rescan Apple Photos.", color = InkSub, style = LascoBody, modifier = Modifier.padding(top = 20.dp))
    error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) }
}

@Composable
private fun ImportPage(progress: ImportProgress, running: Boolean, error: String?, onStart: () -> Unit, onPause: () -> Unit) {
    PageTitle("Import", "The importer runs in durable 32-item chunks. Pause finishes the active chunk and its fan-out, writes a checkpoint, and lets you close the app safely.")
    Spacer(Modifier.height(24.dp))
    Detail("STATUS", progress.detail.ifBlank { progress.state.name.lowercase().replaceFirstChar(Char::uppercase) })
    Detail("PROGRESS", "${progress.completedAssets} / ${progress.totalAssets} items")
    progress.activeChunk?.let { Detail("ACTIVE CHUNK", it.toString()) }
    Row(Modifier.padding(top = 20.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        LascoButton(if (running) "IMPORTING…" else "START OR RESUME IMPORT", onStart, enabled = !running, fillWidth = false)
        LascoButton("PAUSE AFTER THIS CHUNK", onPause, primary = false, enabled = running, fillWidth = false)
    }
    error?.let { Spacer(Modifier.height(12.dp)); ErrorMessage(it) }
}

@Composable
private fun WizardFooter(page: Page, connecting: Boolean, discovering: Boolean, archivesReady: Boolean, photosReady: Boolean, onBack: () -> Unit, onConnect: () -> Unit, onContinue: () -> Unit) {
    val picker = page == Page.DESTINATION || page == Page.REMOTE_TYPE || page == Page.SOURCE
    val connectPage = page in setOf(Page.CLOUD, Page.S3, Page.SMB, Page.LOCAL)
    val continueEnabled = (page == Page.TAKEOUT && archivesReady) || (page == Page.PHOTOS && photosReady) || page == Page.REVIEW
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        if (page != Page.DESTINATION) LascoButton("BACK", onBack, primary = false, fillWidth = false)
        Spacer(Modifier.weight(1f))
        if (connectPage) LascoButton(if (connecting) "CONNECTING…" else "CONNECT REMOTE", onConnect, enabled = !connecting, fillWidth = false)
        if (!picker && !connectPage && page != Page.IMPORT) LascoButton(if (discovering) "DISCOVERING…" else if (page == Page.REVIEW) "START IMPORT" else "CONTINUE", onContinue, enabled = continueEnabled && !discovering, fillWidth = false)
    }
}

@Composable private fun PageTitle(title: String, text: String) { Text(title, color = Ink, style = LascoHeading, fontWeight = FontWeight.Bold); Spacer(Modifier.height(8.dp)); Text(text, color = InkSub, style = LascoBody.copy(fontSize = 14.sp, lineHeight = 20.sp), modifier = Modifier.widthIn(max = 680.dp)) }
@Composable private fun Detail(label: String, value: String) { Row(Modifier.padding(vertical = 5.dp)) { Text(label, color = InkMuted, style = LascoLabel, fontWeight = FontWeight.Bold, modifier = Modifier.widthIn(min = 120.dp)); Text(value, color = Ink, style = LascoBody.copy(fontSize = 12.sp)) } }
@Composable private fun ErrorMessage(text: String) { Text(text, color = Error, style = LascoBody.copy(fontSize = 12.sp), modifier = Modifier.fillMaxWidth().background(Error.copy(alpha = .08f)).border(1.dp, Error).padding(10.dp)) }

@Composable
private fun LascoButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, primary: Boolean = true, enabled: Boolean = true, fillWidth: Boolean = true) {
    Box(modifier.then(if (fillWidth) Modifier.fillMaxWidth() else Modifier).background(if (primary) Accent else PlasterDeep).border(2.dp, Ink).clickable(enabled = enabled, onClick = onClick).padding(horizontal = 20.dp, vertical = 12.dp), contentAlignment = Alignment.Center) {
        Text(label, color = if (primary) Color.White else Ink, style = LascoBody.copy(fontSize = 14.sp), fontWeight = FontWeight.Bold)
    }
}

/** Tab is data in importer fields; it must not move focus to the next field. */
@Composable
private fun LascoField(label: String, value: String, onValueChange: (String) -> Unit, placeholder: String = "") {
    var fieldValue by remember { mutableStateOf(TextFieldValue(value)) }
    LaunchedEffect(value) { if (value != fieldValue.text) fieldValue = TextFieldValue(value, TextRange(value.length)) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label.uppercase(), color = InkSub, style = LascoLabel, fontWeight = FontWeight.Bold)
        BasicTextField(
            value = fieldValue, onValueChange = { fieldValue = it; onValueChange(it.text) }, textStyle = LascoBody.copy(color = Ink), cursorBrush = SolidColor(Pink), visualTransformation = VisualTransformation.None,
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

private fun formatBytes(value: Long): String = when {
    value >= 1024L * 1024 * 1024 -> "%.1f GB".format(value / (1024.0 * 1024 * 1024))
    value >= 1024L * 1024 -> "%.1f MB".format(value / (1024.0 * 1024))
    else -> "$value B"
}

private fun formatRate(value: Long): String = "${formatBytes(value)}/s"
