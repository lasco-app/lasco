package com.lasco.lasco.ui.media

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiMediaItem

/**
 * Recent media grid, the Android equivalent of the Swift media list screen.
 * Subscribes to MediaList (and, for free, to the All wildcard on sync).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class RecentMediaViewModel(
    private val repo: LibraryRepository,
) : ViewModel() {
    private data class MediaPage(val items: List<FfiMediaItem>, val total: Int)

    private val _showingOrphans = MutableStateFlow(false)
    val showingOrphans: StateFlow<Boolean> = _showingOrphans.asStateFlow()

    private val _media = MutableStateFlow<List<FfiMediaItem>>(emptyList())
    val media: StateFlow<List<FfiMediaItem>> = _media.asStateFlow()

    private val _hasMore = MutableStateFlow(false)
    val hasMore: StateFlow<Boolean> = _hasMore.asStateFlow()

    private var total = 0
    private var loadingMore = false
    private var generation = 0

    init {
        viewModelScope.launch {
            showingOrphans.flatMapLatest { orphaned ->
                repo.watch(Change.MediaList) {
                    val count = if (orphaned) repo.orphanMediaByDateCount() else repo.mediaByDateCount()
                    val firstPage = if (orphaned) {
                        repo.orphanMediaByDate(offset = 0, limit = PAGE_SIZE)
                    } else {
                        repo.mediaByDate(offset = 0, limit = PAGE_SIZE)
                    }
                    MediaPage(firstPage, count)
                }
            }.collect { page ->
                generation += 1
                total = page.total
                _media.value = page.items
                _hasMore.value = page.items.size < page.total
            }
        }
    }

    fun setShowingOrphans(value: Boolean) {
        if (_showingOrphans.value == value) return
        generation += 1
        total = 0
        _media.value = emptyList()
        _hasMore.value = false
        _showingOrphans.value = value
    }

    fun loadMore() {
        if (!_hasMore.value || loadingMore) return
        loadingMore = true
        viewModelScope.launch {
            try {
                val requestGeneration = generation
                val orphaned = _showingOrphans.value
                val offset = _media.value.size
                val next = if (orphaned) {
                    repo.orphanMediaByDate(offset, PAGE_SIZE)
                } else {
                    repo.mediaByDate(offset, PAGE_SIZE)
                }
                if (requestGeneration != generation || orphaned != _showingOrphans.value) return@launch
                _media.value += next
                _hasMore.value = _media.value.size < total && next.isNotEmpty()
            } finally {
                loadingMore = false
            }
        }
    }

    companion object {
        private const val PAGE_SIZE = 100

        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                RecentMediaViewModel(LibraryRepository.from(app))
            }
        }
    }
}
