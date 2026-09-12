package com.lasco.lasco.ui.components

import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId

/** Exposes a stable Compose test tag as an Android accessibility resource id for Maestro. */
fun Modifier.maestroTag(value: String): Modifier =
    semantics { testTagsAsResourceId = true }.testTag(value)
