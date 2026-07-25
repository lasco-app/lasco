package com.lasco.lasco.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lasco.lasco.ui.components.ErrorBanner
import com.lasco.lasco.ui.components.LascoCheckbox
import com.lasco.lasco.ui.components.LascoField
import com.lasco.lasco.ui.components.LascoPrimaryButton
import com.lasco.lasco.ui.theme.LascoTheme

/**
 * Add an existing S3 backed library, ported from the Swift
 * AddExistingLibraryView. Calls ffiAddExistingLibraryS3 through
 * AddExistingLibraryViewModel, which already returns an opened FfiLibrary.
 */
@Composable
fun AddExistingLibraryScreen(
    onBack: () -> Unit,
    onLibraryOpened: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: AddExistingLibraryViewModel = viewModel(factory = AddExistingLibraryViewModel.Factory),
) {
    val colors = LascoTheme.colors
    val state by viewModel.uiState.collectAsStateWithLifecycle()

    var nickname by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var remoteName by remember { mutableStateOf("my s3 remote") }
    var endpoint by remember { mutableStateOf("") }
    var bucket by remember { mutableStateOf("") }
    var region by remember { mutableStateOf("") }
    var pathPrefix by remember { mutableStateOf("") }
    var accessKey by remember { mutableStateOf("") }
    var secretKey by remember { mutableStateOf("") }
    var uploadAcknowledged by remember { mutableStateOf(false) }

    LaunchedEffect(state.opened) {
        if (state.opened) onLibraryOpened()
    }

    val isValid = nickname.isNotEmpty() && username.isNotEmpty() && password.isNotEmpty() &&
        remoteName.isNotEmpty() && endpoint.isNotEmpty() && bucket.isNotEmpty() &&
        accessKey.isNotEmpty() && secretKey.isNotEmpty() && uploadAcknowledged && !state.loading

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg),
    ) {
        Text(
            text = "← Back",
            style = LascoTheme.type.body(14),
            color = colors.inkMuted,
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 32.dp, bottom = 16.dp)
                .clickable(interactionSource = null, indication = null) { onBack() },
        )

        Text(
            text = "Add an existing library",
            style = LascoTheme.type.title(26),
            color = colors.ink,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 32.dp, vertical = 8.dp),
        )

        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 32.dp)
                .padding(top = 12.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(
                text = "Point Lasco at an S3 remote that already holds a library. It downloads the library and syncs it to this device.",
                style = LascoTheme.type.body(16),
                color = colors.inkSub,
            )

            LascoField("Library name", nickname, { nickname = it }, placeholder = "my-library")
            LascoField("Username", username, { username = it }, placeholder = "an existing user")
            LascoField("Password", password, { password = it }, secure = true)

            LascoField("Remote name", remoteName, { remoteName = it }, placeholder = "my s3 remote")
            LascoField("Endpoint URL", endpoint, { endpoint = it }, placeholder = "https://region1.example-s3-server.com")
            LascoField("Bucket", bucket, { bucket = it }, placeholder = "my-photos-bucket")
            LascoField("Region", region, { region = it }, placeholder = "region1")
            LascoField("Path prefix (optional)", pathPrefix, { pathPrefix = it }, placeholder = "photos/")
            LascoField("Access key", accessKey, { accessKey = it })
            LascoField("Secret key", secretKey, { secretKey = it }, secure = true)

            LascoCheckbox(
                checked = uploadAcknowledged,
                onCheckedChange = { uploadAcknowledged = it },
                label = "I understand this app will upload my photos to the S3 bucket configured above.",
            )

            state.error?.let { ErrorBanner(it) }
        }

        Column(
            modifier = Modifier
                .padding(horizontal = 32.dp)
                .padding(top = 20.dp, bottom = 48.dp),
        ) {
            LascoPrimaryButton(
                text = if (state.loading) "Adding…" else "Add library",
                onClick = {
                    viewModel.add(
                        nickname = nickname,
                        username = username,
                        password = password,
                        remoteName = remoteName,
                        endpoint = endpoint,
                        bucket = bucket,
                        region = region,
                        pathPrefix = pathPrefix,
                        accessKey = accessKey,
                        secretKey = secretKey,
                    )
                },
                enabled = isValid,
            )
        }
    }
}
