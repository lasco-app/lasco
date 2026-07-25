package com.lasco.lasco.data

/**
 * The scopes a mutation can invalidate. All is the wildcard used whenever we
 * pulled a new version of the library and do not know precisely what changed.
 */
sealed interface Change {
    data object All : Change
    data object MediaList : Change
    data object AlbumList : Change
    data class Album(val id: String) : Change
    data class Media(val id: String) : Change
}
