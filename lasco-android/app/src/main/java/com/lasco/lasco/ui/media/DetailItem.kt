package com.lasco.lasco.ui.media

import androidx.navigation3.runtime.NavKey
import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.serialization.Serializable
import uniffi.lasco_ffi.FfiGroup
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiMediaUuid

/** Non-persisted bitmap passed only from a tapped grid cell into Media Detail. */
data class MediaDetailInitialThumbnail(
    val mediaId: FfiMediaUuid,
    val bitmap: ImageBitmap,
)

/** Holds one launch preview outside serialized navigation state, then releases it. */
class MediaDetailThumbnailHandoff {
    private var thumbnail: MediaDetailInitialThumbnail? = null

    fun offer(value: MediaDetailInitialThumbnail?) {
        thumbnail = value
    }

    fun take(): MediaDetailInitialThumbnail? = thumbnail.also { thumbnail = null }
}

/**
 * One page in the Media Detail pager, mirrors Swift's AlbumItem. Home only
 * ever produces Media entries, Albums produces both.
 */
sealed interface DetailItem {
    data class Media(val item: FfiMediaItem) : DetailItem
    data class Group(val group: FfiGroup) : DetailItem
}

/**
 * Stable identity for a detail cursor. Positions are deliberately retained for
 * efficient neighbor navigation, but must be checked against this identity
 * after a library refresh because the sorted list may have moved.
 */
@Serializable
sealed interface DetailTarget {
    @Serializable data class Media(val mediaId: String) : DetailTarget
    @Serializable data class Group(val groupId: String) : DetailTarget
}

val DetailItem.target: DetailTarget
    get() = when (this) {
        is DetailItem.Media -> DetailTarget.Media(item.mediaId.value)
        is DetailItem.Group -> DetailTarget.Group(group.groupId.value)
    }

@Serializable
sealed interface MediaDetailSource {
    @Serializable data object HomeByDate : MediaDetailSource
    @Serializable data object OrphansByDate : MediaDetailSource
    @Serializable data class AlbumByDate(val albumId: String, val ascending: Boolean) : MediaDetailSource
}

@Serializable
data class MediaDetailKey(
    val source: MediaDetailSource,
    val startPosition: Int,
    val expectedTarget: DetailTarget,
) : NavKey
