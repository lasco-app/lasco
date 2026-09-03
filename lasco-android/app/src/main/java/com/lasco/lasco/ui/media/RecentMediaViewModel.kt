package com.lasco.lasco.ui.media

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
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiMediaItem

class RecentMediaViewModel(
    private val repo: LibraryRepository,
) : ViewModel() {
    private val _showingOrphans = MutableStateFlow(false)
    val showingOrphans: StateFlow<Boolean> = _showingOrphans.asStateFlow()

    private val mediaRevision = MutableStateFlow(0)

    private val config = PagingConfig(pageSize = PAGE_SIZE, prefetchDistance = PREFETCH_DISTANCE, enablePlaceholders = true)
    @OptIn(ExperimentalCoroutinesApi::class)
    private val allMedia = mediaRevision.flatMapLatest {
        Pager(config) { OffsetPagingSource(repo::mediaByDateCount, repo::mediaByDate) }.flow
    }
    @OptIn(ExperimentalCoroutinesApi::class)
    private val orphanMedia = mediaRevision.flatMapLatest {
        Pager(config) { OffsetPagingSource(repo::orphanMediaByDateCount, repo::orphanMediaByDate) }.flow
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    val media: Flow<PagingData<FfiMediaItem>> = showingOrphans
        .flatMapLatest { if (it) orphanMedia else allMedia }
        .cachedIn(viewModelScope)

    init {
        viewModelScope.launch {
            repo.watch(Change.MediaList) { Unit }.collect { mediaRevision.value++ }
        }
    }

    fun setShowingOrphans(value: Boolean) {
        _showingOrphans.value = value
    }

    companion object {
        private const val PAGE_SIZE = 100
        private const val PREFETCH_DISTANCE = 30

        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                RecentMediaViewModel(LibraryRepository.from(app))
            }
        }
    }
}
