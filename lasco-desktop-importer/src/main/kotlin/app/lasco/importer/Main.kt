package app.lasco.importer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import java.awt.FileDialog
import java.awt.Frame
import java.nio.file.Path

private enum class WizardStep(val title: String) { LIBRARY("1. Library"), SOURCE("2. Source"), REVIEW("3. Review"), IMPORT("4. Import") }

fun main() = application {
    Window(onCloseRequest = ::exitApplication, title = "Lasco Desktop Importer") {
        MaterialTheme { ImporterWizard() }
    }
}

@Composable
private fun ImporterWizard() {
    var step by remember { mutableIntStateOf(0) }
    var nickname by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var source by remember { mutableStateOf("Google Takeout") }
    var archivePaths by remember { mutableStateOf(emptyList<String>()) }
    var remoteSummary by remember { mutableStateOf("No remote connected yet") }

    Column(Modifier.fillMaxSize().padding(32.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            WizardStep.entries.forEachIndexed { index, item -> Text(if (index == step) "• ${item.title}" else item.title) }
        }
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                when (step) {
                    0 -> LibraryPage(nickname, username, password, remoteSummary, { nickname = it }, { username = it }, { password = it })
                    1 -> SourcePage(source, archivePaths, { source = it }, { archivePaths = chooseTakeoutZips() })
                    2 -> ReviewPage(source, archivePaths, remoteSummary)
                    3 -> ImportPage()
                }
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = step > 0, onClick = { step-- }) { Text("Back") }
            Button(onClick = {
                // Connection and discovery are deliberately explicit actions in the view model:
                // no remote is touched before credentials/source consent are provided.
                if (step < WizardStep.entries.lastIndex) step++
            }) { Text(if (step == WizardStep.entries.lastIndex) "Close" else "Continue") }
        }
    }
}

@Composable
private fun LibraryPage(nickname: String, username: String, password: String, remoteSummary: String, setNickname: (String) -> Unit, setUsername: (String) -> Unit, setPassword: (String) -> Unit) {
    Text("Connect an existing library")
    Text("Use Lasco Cloud, S3-compatible storage, SMB, or a configured fixed path. USB is intentionally unavailable in this importer.")
    OutlinedTextField(nickname, setNickname, label = { Text("Library nickname") })
    OutlinedTextField(username, setUsername, label = { Text("Library username") })
    OutlinedTextField(password, setPassword, label = { Text("Library password") })
    Text(remoteSummary)
}

@Composable
private fun SourcePage(source: String, archivePaths: List<String>, setSource: (String) -> Unit, chooseArchives: () -> Unit) {
    Text("Choose a source")
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(onClick = { setSource("Google Takeout") }) { Text("Google Takeout") }
        if (System.getProperty("os.name").lowercase().contains("mac")) Button(onClick = { setSource("Apple Photos / iCloud") }) { Text("Apple Photos / iCloud") }
    }
    if (source == "Google Takeout") {
        Text("Select one or more Takeout ZIP archives. They remain in place and are read lazily.")
        Button(onClick = chooseArchives) { Text("Choose ZIP archives") }
        archivePaths.forEach { Text(it) }
    } else Text("The app requests Photos access, discovers albums/resources once, and downloads iCloud originals only as each chunk is imported.")
}

@Composable
private fun ReviewPage(source: String, archivePaths: List<String>, remoteSummary: String) {
    Text("Review before import")
    Text("Source: $source")
    Text("Remote status: $remoteSummary")
    Text("The recap will show candidates, bytes, albums, selected upload parallelism, and an ETA after discovery and 4 MiB remote benchmarks.")
    Text("Exact existing-library duplicates are confirmed during Rust import by content hash; they are never guessed from filenames.")
    archivePaths.forEach { Text("Archive: $it") }
}

@Composable
private fun ImportPage() {
    Text("Import")
    Text("Start imports in 32-item chunks. Pause finishes the active chunk and its remote fan-out, then checkpoints safely. Resume uses the SQLite manifest without rescanning Apple Photos.")
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(onClick = {}) { Text("Start import") }
        Button(onClick = {}) { Text("Pause after current chunk") }
    }
}

private fun chooseTakeoutZips(): List<String> {
    val dialog = FileDialog(null as Frame?, "Choose Google Takeout ZIP", FileDialog.LOAD)
    dialog.isMultipleMode = true
    dialog.isVisible = true
    return dialog.files.map { Path.of(it.toURI()).toString() }
}
