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

/**
 * Nav 3 key for Media Detail: pure ids, resolved to a live item list by
 * MediaDetailViewModel through the same repo.watch subscription the
 * calling screen already uses. sourceAlbumId null means Home's list
 * (mediaByDate), otherwise the given album's sorted items.
 */
@Serializable
data class MediaDetailKey(val sourceAlbumId: String?, val startMediaId: String) : NavKey
