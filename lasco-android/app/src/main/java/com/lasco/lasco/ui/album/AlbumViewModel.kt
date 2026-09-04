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
import androidx.paging.cachedIn
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.OffsetPagingSource
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem
import uniffi.lasco_ffi.FfiAlbumUuid
import com.lasco.lasco.ui.media.DetailTarget

sealed interface AlbumEntry {
    data class Item(val item: FfiAlbumItem, val position: Int) : AlbumEntry
}

fun FfiAlbumItem.toDetailTarget(): DetailTarget =
    media?.let { DetailTarget.Media(it.mediaId.value) }
        ?: group?.let { DetailTarget.Group(it.groupId.value) }
        ?: error("FFI album item contained neither media nor group")

class AlbumViewModel(
    private val albumId: FfiAlbumUuid?,
    private val repo: LibraryRepository,
) : ViewModel() {
    private val _sortAscending = MutableStateFlow(false)
    val sortAscending: StateFlow<Boolean> = _sortAscending.asStateFlow()
    private val albumsRevision = MutableStateFlow(0)
    private val mediaRevision = MutableStateFlow(0)
    private val pagingConfig = PagingConfig(
        pageSize = PAGE_SIZE,
        prefetchDistance = PREFETCH_DISTANCE,
        enablePlaceholders = true,
    )

    @OptIn(ExperimentalCoroutinesApi::class)
    val albums: Flow<PagingData<FfiAlbum>> = albumsRevision.flatMapLatest {
        Pager(pagingConfig) {
            OffsetPagingSource(
                count = { repo.albumChildrenCount(albumId) },
                range = { offset, limit -> repo.albumChildren(albumId, offset, limit) },
            )
        }.flow
    }.cachedIn(viewModelScope)

    @OptIn(ExperimentalCoroutinesApi::class)
    val disconnectedAlbums: Flow<PagingData<FfiAlbum>> = albumsRevision.flatMapLatest {
        if (albumId == null) {
            Pager(pagingConfig) {
                OffsetPagingSource(repo::disconnectedAlbumsCount, repo::disconnectedAlbums)
            }.flow
        } else {
            flowOf(PagingData.empty())
        }
    }.cachedIn(viewModelScope)

    @OptIn(ExperimentalCoroutinesApi::class)
    val items: Flow<PagingData<AlbumEntry.Item>> = sortAscending.flatMapLatest { ascending ->
        mediaRevision.flatMapLatest {
            albumId?.let { id ->
            Pager(PagingConfig(pageSize = PAGE_SIZE, prefetchDistance = PREFETCH_DISTANCE, enablePlaceholders = true)) {
                    OffsetPagingSource(
                        count = { repo.albumItemsCount(id) },
                        range = { offset, limit ->
                            repo.albumItemsByDate(id, ascending, offset, limit)
                                .mapIndexed { index, item -> AlbumEntry.Item(item, offset + index) }
                        },
                    )
            }.flow
            } ?: flowOf(PagingData.empty())
        }
    }.cachedIn(viewModelScope)

    init {
        viewModelScope.launch {
            repo.watch(Change.AlbumList) { Unit }.collect { albumsRevision.value++ }
        }
        viewModelScope.launch {
            repo.watch(Change.MediaList, *albumId?.let { arrayOf(Change.Album(it)) }.orEmpty()) { Unit }
                .collect { mediaRevision.value++ }
        }
    }

    fun setSortAscending(value: Boolean) { _sortAscending.value = value }

    companion object {
        private const val PAGE_SIZE = 100
        private const val PREFETCH_DISTANCE = 30

        fun factory(albumId: FfiAlbumUuid?): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                AlbumViewModel(albumId, LibraryRepository.from(app))
            }
        }
    }
}
