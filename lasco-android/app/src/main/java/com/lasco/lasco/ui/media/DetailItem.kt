package com.lasco.lasco.ui.media

import androidx.navigation3.runtime.NavKey
import kotlinx.serialization.Serializable
import uniffi.lasco_ffi.FfiGroup
import uniffi.lasco_ffi.FfiMediaItem

/**
 * One page in the Media Detail pager, mirrors Swift's AlbumItem. Home only
 * ever produces Media entries, Albums produces both.
 */
sealed interface DetailItem {
    data class Media(val item: FfiMediaItem) : DetailItem
    data class Group(val group: FfiGroup) : DetailItem
}

val DetailItem.id: String
    get() = when (this) {
        is DetailItem.Media -> item.mediaId
        is DetailItem.Group -> group.groupId
    }

@Serializable
sealed interface MediaDetailSource {
    @Serializable data object HomeByDate : MediaDetailSource
    @Serializable data class AlbumByDate(val albumId: String, val ascending: Boolean) : MediaDetailSource
}

@Serializable
data class MediaDetailKey(val source: MediaDetailSource, val startPosition: Int) : NavKey
