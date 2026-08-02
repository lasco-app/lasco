package com.lasco.lasco.ui.album

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.paging.Pager
import androidx.paging.PagingConfig
import androidx.paging.PagingData
import androidx.paging.PagingSource
import androidx.paging.PagingState
import androidx.paging.cachedIn
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem

sealed interface AlbumEntry {
    val key: String
    data class ChildAlbum(val album: FfiAlbum) : AlbumEntry { override val key = "album:${album.albumId}" }
    data class Item(val item: FfiAlbumItem) : AlbumEntry {
        override val key = item.media?.mediaId?.let { "media:$it" } ?: "group:${item.group!!.groupId}"
    }
    data object DisconnectedHeader : AlbumEntry { override val key = "header:disconnected" }
}

private class AlbumEntriesPagingSource(
    private val repo: LibraryRepository,
    private val albumId: String?,
    private val ascending: Boolean,
) : PagingSource<Int, AlbumEntry>() {
    override suspend fun load(params: LoadParams<Int>): LoadResult<Int, AlbumEntry> = try {
        val children = repo.albumChildrenCount(albumId)
        val disconnected = if (albumId == null) repo.disconnectedAlbumsCount() else 0
        val albumItems = if (albumId != null) repo.albumItemsCount(albumId) else 0
        val total = children + disconnected + albumItems + if (disconnected > 0) 1 else 0
        val offset = (params.key ?: 0).coerceIn(0, total)
        val end = minOf(total, offset + params.loadSize)
        val result = mutableListOf<AlbumEntry>()
        var position = offset
        while (position < end) {
            when {
                position < children -> {
                    val size = minOf(end - position, children - position)
                    result += repo.albumChildren(albumId, position, size).map(AlbumEntry::ChildAlbum)
                    position += size
                }
                disconnected > 0 && position == children -> {
                    result += AlbumEntry.DisconnectedHeader
                    position++
                }
                disconnected > 0 && position < children + 1 + disconnected -> {
                    val localOffset = position - children - 1
                    val size = minOf(end - position, disconnected - localOffset)
                    result += repo.disconnectedAlbums(localOffset, size).map(AlbumEntry::ChildAlbum)
                    position += size
                }
                else -> {
                    val itemOffset = position - children - disconnected - if (disconnected > 0) 1 else 0
                    val size = minOf(end - position, albumItems - itemOffset)
                    result += repo.albumItemsByDate(albumId!!, ascending, itemOffset, size).map(AlbumEntry::Item)
                    position += size
                }
            }
        }
        LoadResult.Page(
            data = result,
            prevKey = if (offset == 0) null else (offset - params.loadSize).coerceAtLeast(0),
            nextKey = if (position >= total || result.isEmpty()) null else position,
            itemsBefore = offset,
            itemsAfter = (total - position).coerceAtLeast(0),
        )
    } catch (t: Throwable) {
        LoadResult.Error(t)
    }

    override fun getRefreshKey(state: PagingState<Int, AlbumEntry>): Int? =
        state.anchorPosition?.let { anchor ->
            state.closestPageToPosition(anchor)?.let { page ->
                page.prevKey?.plus(page.data.size) ?: page.nextKey?.minus(page.data.size)
            } ?: anchor
        }
}

class AlbumViewModel(
    private val albumId: String?,
    private val repo: LibraryRepository,
) : ViewModel() {
    private val _sortAscending = MutableStateFlow(false)
    val sortAscending: StateFlow<Boolean> = _sortAscending.asStateFlow()
    private var currentSource: AlbumEntriesPagingSource? = null

    @OptIn(ExperimentalCoroutinesApi::class)
    val entries: Flow<PagingData<AlbumEntry>> = sortAscending.flatMapLatest { ascending ->
        Pager(PagingConfig(pageSize = PAGE_SIZE, prefetchDistance = PREFETCH_DISTANCE, enablePlaceholders = true)) {
            AlbumEntriesPagingSource(repo, albumId, ascending).also { currentSource = it }
        }.flow
    }.cachedIn(viewModelScope)

    init {
        viewModelScope.launch {
            repo.watch(Change.AlbumList, Change.MediaList, *albumId?.let { arrayOf(Change.Album(it)) }.orEmpty()) { Unit }
                .collect { currentSource?.invalidate() }
        }
    }

    fun setSortAscending(value: Boolean) { _sortAscending.value = value }

    companion object {
        private const val PAGE_SIZE = 100
        private const val PREFETCH_DISTANCE = 30

        fun factory(albumId: String?): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                AlbumViewModel(albumId, LibraryRepository.from(app))
            }
        }
    }
}
