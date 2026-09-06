package com.lasco.lasco.ui.album

import org.junit.Assert.assertEquals
import org.junit.Test

class AlbumBreadcrumbTitleTest {
    @Test
    fun displaysTheFullPathForTwoAlbums() {
        assertEquals("A / B", albumBreadcrumbTitle(listOf("a", "b")))
    }

    @Test
    fun collapsesPathsDeeperThanTwoAlbums() {
        assertEquals("... / B / C", albumBreadcrumbTitle(listOf("a", "b", "c")))
        assertEquals("... / C / D", albumBreadcrumbTitle(listOf("a", "b", "c", "d")))
    }
}
