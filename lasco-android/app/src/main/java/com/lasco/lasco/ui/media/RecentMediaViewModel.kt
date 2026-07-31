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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.stateIn
import uniffi.lasco_ffi.FfiMediaItem

/**
 * Recent media grid, the Android equivalent of the Swift media list screen.
 * Subscribes to MediaList (and, for free, to the All wildcard on sync).
 */
class RecentMediaViewModel(
    repo: LibraryRepository,
) : ViewModel() {
    private val _showingOrphans = MutableStateFlow(false)
    val showingOrphans: StateFlow<Boolean> = _showingOrphans.asStateFlow()

    @OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
    val media: StateFlow<List<FfiMediaItem>> =
        showingOrphans.flatMapLatest { showingOrphans ->
            repo.watch(Change.MediaList) {
                if (showingOrphans) repo.orphanMediaByDate() else repo.mediaByDate()
            }
        }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun setShowingOrphans(value: Boolean) {
        _showingOrphans.value = value
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                RecentMediaViewModel(LibraryRepository.from(app))
            }
        }
    }
}
