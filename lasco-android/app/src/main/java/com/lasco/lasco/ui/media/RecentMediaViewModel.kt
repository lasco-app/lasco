package com.lasco.lasco.ui.media

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.data.Change
import com.lasco.lasco.data.LibraryRepository
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import uniffi.lasco_ffi.FfiMediaItem

/**
 * Recent media grid, the Android equivalent of the Swift media list screen.
 * Subscribes to MediaList (and, for free, to the All wildcard on sync).
 */
class RecentMediaViewModel(
    repo: LibraryRepository,
) : ViewModel() {
    val media: StateFlow<List<FfiMediaItem>> =
        repo.watch(Change.MediaList) { repo.mediaByDate() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                RecentMediaViewModel(LibraryRepository.from(app))
            }
        }
    }
}
