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
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiMediaUuid

class TrashViewModel(
    private val repo: LibraryRepository,
) : ViewModel() {
    private val revision = MutableStateFlow(0)

    @OptIn(ExperimentalCoroutinesApi::class)
    val media: Flow<PagingData<FfiMediaItem>> = revision.flatMapLatest {
        Pager(PagingConfig(pageSize = PAGE_SIZE, prefetchDistance = PREFETCH_DISTANCE, enablePlaceholders = true)) {
            OffsetPagingSource(repo::trashedMediaByDateCount, repo::trashedMediaByDate)
        }.flow
    }.cachedIn(viewModelScope)

    init {
        viewModelScope.launch {
            repo.watch(Change.MediaList) { Unit }.collect { revision.value++ }
        }
    }

    fun restore(mediaId: FfiMediaUuid) {
        viewModelScope.launch { repo.restoreMedia(mediaId) }
    }

    companion object {
        private const val PAGE_SIZE = 100
        private const val PREFETCH_DISTANCE = 30

        fun factory(): ViewModelProvider.Factory = viewModelFactory {
            initializer { TrashViewModel(LibraryRepository.from(this[APPLICATION_KEY]!!)) }
        }
    }
}
