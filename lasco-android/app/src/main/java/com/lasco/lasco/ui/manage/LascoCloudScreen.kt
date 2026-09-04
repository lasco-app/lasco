package com.lasco.lasco.ui.manage

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lasco.lasco.data.CloudAccount
import com.lasco.lasco.data.CloudSubscription
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import com.lasco.lasco.ui.components.LascoPrimaryButton
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlinx.coroutines.launch

@Composable
fun LascoCloudScreen(
    modifier: Modifier = Modifier,
    onBack: () -> Unit,
    manageViewModel: ManageViewModel,
    onSignedOut: () -> Unit,
) {
    val colors = LascoTheme.colors
    val session by manageViewModel.sessionState.collectAsStateWithLifecycle()
    var account by remember { mutableStateOf<CloudAccount?>(null) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var signingOut by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    LaunchedEffect(session.libraryId) {
        loading = true
        error = null
        try {
            account = manageViewModel.lascoCloudSubscription()
        } catch (exception: Exception) {
            error = exception.message ?: "Could not load Lasco Cloud information."
        }
        loading = false
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg)
            .padding(horizontal = 16.dp),
    ) {
        Text(
            text = "← Manage",
            style = LascoTheme.type.body(),
            color = colors.inkSub,
            modifier = Modifier.clickable { onBack() }.padding(top = 20.dp, bottom = 20.dp),
        )
        Text(text = "LASCO CLOUD", style = LascoTheme.type.categoryLarge(), color = colors.ink)
        Text(
            text = session.nickname,
            style = LascoTheme.type.subtitle(),
            color = colors.inkMuted,
            modifier = Modifier.padding(top = 4.dp, bottom = 16.dp),
        )

        Column(modifier = Modifier.fillMaxWidth().lascoPanel().padding(horizontal = 16.dp, vertical = 14.dp)) {
            when {
                loading -> Text("Loading subscription…", style = LascoTheme.type.body(), color = colors.inkMuted)
                error != null -> Text(error!!, style = LascoTheme.type.body(), color = colors.error)
                account == null -> Text("Could not load Lasco Cloud information.", style = LascoTheme.type.body(), color = colors.error)
                else -> CloudAccountDetails(account = account!!)
            }
        }

        Text(
            text = "To delete your account, sign in from a browser at getlasco.app.",
            style = LascoTheme.type.body(),
            color = colors.inkMuted,
            modifier = Modifier.padding(top = 16.dp),
        )

        LascoPrimaryButton(
            text = if (signingOut) "Signing out…" else "Sign out",
            enabled = !signingOut,
            onClick = {
                signingOut = true
                error = null
                scope.launch {
                    try {
                        manageViewModel.signOutLascoCloud()
                        onSignedOut()
                    } catch (exception: Exception) {
                        error = exception.message ?: "Could not sign out of Lasco Cloud."
                    } finally {
                        signingOut = false
                    }
                }
            },
            modifier = Modifier.fillMaxWidth().padding(top = 16.dp),
        )
    }
}

@Composable
private fun CloudAccountDetails(account: CloudAccount) {
    CloudInfoRow("Email", account.email)
    if (account.subscription == null) {
        CloudInfoRow("Plan", "No active plan")
    } else {
        CloudSubscriptionDetails(account.subscription)
    }
}

@Composable
private fun CloudSubscriptionDetails(subscription: CloudSubscription) {
    CloudInfoRow("Plan", subscription.planName)
    CloudInfoRow("Status", subscription.status.replaceFirstChar { it.uppercase() })
    CloudInfoRow("Storage", formatStorageQuota(subscription.storageQuotaBytes))
    CloudInfoRow("Renews", formatRenewalDate(subscription.renewsAt))
}

@Composable
private fun CloudInfoRow(label: String, value: String) {
    val colors = LascoTheme.colors
    Row(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
        Text(label, style = LascoTheme.type.body(), color = colors.inkSub)
        Spacer(modifier = Modifier.weight(1f))
        Text(value, style = LascoTheme.type.mono(), color = colors.ink)
    }
}

private fun formatStorageQuota(bytes: Long): String = "%.0f GB".format(Locale.US, bytes / 1_000_000_000.0)

private fun formatRenewalDate(value: String): String = runCatching {
    DateTimeFormatter.ofPattern("d MMM yyyy").withZone(ZoneId.systemDefault()).format(Instant.parse(value))
}.getOrElse { value }
