package com.lasco.lasco.ui.album

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.stateIn
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem

/**
 * One album level's data, root (albumId null) or a specific album. Child
 * albums and the disconnected-albums section are derived from allAlbums in
 * the screen rather than watched separately, one less FFI round trip.
 */
class AlbumViewModel(
    private val albumId: String?,
    repo: LibraryRepository,
) : ViewModel() {
    val allAlbums: StateFlow<List<FfiAlbum>> =
        repo.watch(Change.AlbumList) { repo.allAlbums() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _sortAscending = MutableStateFlow(false)
    val sortAscending: StateFlow<Boolean> = _sortAscending.asStateFlow()

    fun setSortAscending(value: Boolean) {
        _sortAscending.value = value
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    val items: StateFlow<List<FfiAlbumItem>> = if (albumId == null) {
        MutableStateFlow(emptyList<FfiAlbumItem>())
    } else {
        _sortAscending.flatMapLatest { ascending ->
            repo.watch(Change.Album(albumId), Change.AlbumList) { repo.albumItemsSorted(albumId, ascending) }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
    }

    companion object {
        fun factory(albumId: String?): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                AlbumViewModel(albumId, LibraryRepository.from(app))
            }
        }
    }
}
