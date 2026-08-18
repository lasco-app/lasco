package com.lasco.lasco.ui.manage

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.ui.theme.LascoTheme
import com.lasco.lasco.ui.theme.lascoPanel
import java.text.SimpleDateFormat
import java.util.Locale
import uniffi.lasco_ffi.FfiOperation
import uniffi.lasco_ffi.FfiCrdtOperation

/**
 * Ported from Swift's OperationsView, gated behind expert mode the same way
 * as the Swift Manage screen. Shows individual CRDT operations, newest first.
 */
@Composable
fun OperationsScreen(onBack: () -> Unit, modifier: Modifier = Modifier) {
    val colors = LascoTheme.colors
    val context = LocalContext.current
    val repo = remember { LibraryRepository.from(context) }
    var operations by remember { mutableStateOf<List<FfiCrdtOperation>>(emptyList()) }
    var nextStartPos by remember { mutableStateOf(0uL) }
    var hasMore by remember { mutableStateOf(true) }
    var isLoading by remember { mutableStateOf(false) }

    suspend fun loadMore() {
        if (isLoading || !hasMore) return
        isLoading = true
        try {
            val endPosExclusive = nextStartPos + 50uL
            val page = repo.listOperations(nextStartPos, endPosExclusive)
            operations = operations + page
            nextStartPos = endPosExclusive
            hasMore = page.size == 50
        } finally {
            isLoading = false
        }
    }

    LaunchedEffect(Unit) {
        loadMore()
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(colors.bg)
            .padding(horizontal = 16.dp),
    ) {
        Row(modifier = Modifier.fillMaxWidth().padding(top = 20.dp, bottom = 12.dp)) {
            Text(
                text = "← Manage",
                style = LascoTheme.type.body(),
                color = colors.inkMuted,
                modifier = Modifier.clickable { onBack() },
            )
        }
        Text(text = "OPERATIONS", style = LascoTheme.type.categoryLarge(), color = colors.ink)
        Spacer(modifier = Modifier.height(16.dp))

        if (operations.isEmpty() && !isLoading) {
            Column(
                modifier = Modifier.fillMaxWidth().weight(1f),
                verticalArrangement = Arrangement.Center,
            ) {
                Text(
                    text = "No operations yet.",
                    style = LascoTheme.type.body(),
                    color = colors.inkMuted,
                    modifier = Modifier.align(Alignment.CenterHorizontally),
                )
            }
        } else {
            LazyColumn(
                modifier = Modifier.fillMaxWidth().weight(1f),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                itemsIndexed(operations, key = { _, operation -> "${operation.dot.deviceId}:${operation.dot.lamportCounter}" }) { index, operation ->
                    OperationCard(operation)
                    if (index == operations.lastIndex) {
                        LaunchedEffect(operation.dot) { loadMore() }
                    }
                }
                if (isLoading) item { androidx.compose.material3.CircularProgressIndicator(color = colors.ink) }
            }
        }
    }
}

@Composable
private fun OperationCard(operation: FfiCrdtOperation) {
    val colors = LascoTheme.colors
    Column(
        modifier = Modifier.fillMaxWidth().lascoPanel().padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.weight(1f)) {
                Text(text = "${operation.dot.deviceId.take(8)}:${operation.dot.lamportCounter}", style = LascoTheme.type.mono(), color = colors.ink)
            }
            Text(text = operation.author, style = LascoTheme.type.mono(), color = colors.inkMuted)
        }

        OperationRow(operation.operation)
    }
}

@Composable
private fun OperationRow(operation: FfiOperation) {
    val colors = LascoTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(
                text = operation.kind,
                style = LascoTheme.type.mono(),
                color = colors.ink,
                modifier = Modifier.weight(1f),
            )
            Text(
                text = formattedTimestamp(operation.timestamp),
                style = LascoTheme.type.mono(),
                color = colors.inkMuted,
                maxLines = 1,
            )
        }
        for (arg in operation.args) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    text = arg.key,
                    style = LascoTheme.type.mono(),
                    color = colors.inkMuted,
                )
                Text(
                    text = arg.value.ifEmpty { "—" },
                    style = LascoTheme.type.mono(),
                    color = colors.inkSub,
                    maxLines = 2,
                )
            }
        }
    }
}

private fun formattedTimestamp(timestamp: String): String {
    val patterns = listOf("yyyy-MM-dd'T'HH:mm:ss.SSSXXX", "yyyy-MM-dd'T'HH:mm:ssXXX")
    for (pattern in patterns) {
        try {
            val date = SimpleDateFormat(pattern, Locale.US).parse(timestamp) ?: continue
            return SimpleDateFormat("MMM d, HH:mm:ss", Locale.getDefault()).format(date)
        } catch (e: Exception) {
            // try the next pattern
        }
    }
    return timestamp
}
