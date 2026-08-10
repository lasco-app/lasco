package com.lasco.lasco.data

import uniffi.lasco_ffi.FfiAlbumUuid
import uniffi.lasco_ffi.FfiMediaUuid

/**
 * The scopes a mutation can invalidate. All is the wildcard used whenever we
 * pulled a new version of the library and do not know precisely what changed.
 */
sealed interface Change {
    data object All : Change
    data object MediaList : Change
    data object AlbumList : Change
    data class Album(val id: FfiAlbumUuid) : Change
    data class Media(val id: FfiMediaUuid) : Change
}
