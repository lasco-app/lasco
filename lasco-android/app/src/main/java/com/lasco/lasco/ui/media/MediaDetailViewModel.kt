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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumItem
import uniffi.lasco_ffi.FfiMediaItem

private fun toDetailItem(item: FfiAlbumItem): DetailItem? =
    item.media?.let { DetailItem.Media(it) } ?: item.group?.let { DetailItem.Group(it) }

/**
 * Media Detail's pager state and per-page metadata, the Android equivalent
 * of the @State bag Swift's MediaDetailView keeps (groupMediaCache,
 * groupMediaIndex, showingLivePhotoVideo, livePhotoVideoItems). Moved here
 * rather than remember{} in the composable so it survives recomposition
 * and config changes the same way the rest of the app's per-screen state
 * does.
 *
 * items subscribes to the same live data source the calling screen already
 * subscribes to (mediaByDate for Home, albumItemsSorted for Albums), so an
 * edit elsewhere while detail is open updates the pager live instead of the
 * previous snapshot-forever behavior. startMediaId is resolved to a start
 * page once, the first time the list emits.
 *
 * currentDisplayItem is the single source of truth for "what media is on
 * screen right now": for a DetailItem.Media page that's the page's item,
 * for a DetailItem.Group page it's groupMediaCache[groupId][groupMediaIndex]
 * once the group's media has loaded. A rename refreshes through the same
 * Change bus everything else uses, so a still-open Albums screen underneath
 * reloads on its own.
 */
class MediaDetailViewModel(
    private val sourceAlbumId: String?,
    private val startMediaId: String,
    private val repo: LibraryRepository,
) : ViewModel() {
    val items: StateFlow<List<DetailItem>> = (
        if (sourceAlbumId == null) {
            repo.watch(Change.MediaList) { repo.mediaByDateAll().map { DetailItem.Media(it) } }
        } else {
            repo.watch(Change.Album(sourceAlbumId), Change.AlbumList) {
                repo.albumItemsSorted(sourceAlbumId, false).mapNotNull(::toDetailItem)
            }
        }
    ).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _currentIndex = MutableStateFlow<Int?>(null)
    val currentIndex: StateFlow<Int?> = _currentIndex.asStateFlow()

    private val _groupMediaCache = MutableStateFlow<Map<String, List<FfiMediaItem>>>(emptyMap())
    val groupMediaCache: StateFlow<Map<String, List<FfiMediaItem>>> = _groupMediaCache.asStateFlow()

    private val _groupMediaIndex = MutableStateFlow(0)
    val groupMediaIndex: StateFlow<Int> = _groupMediaIndex.asStateFlow()

    private val _showingLivePhotoVideo = MutableStateFlow(false)
    val showingLivePhotoVideo: StateFlow<Boolean> = _showingLivePhotoVideo.asStateFlow()

    private val _livePhotoVideoItems = MutableStateFlow<Map<String, FfiMediaItem>>(emptyMap())

    init {
        viewModelScope.launch {
            val list = items.filter { it.isNotEmpty() }.first()
            val idx = list.indexOfFirst { it.id == startMediaId }
            _currentIndex.value = if (idx >= 0) idx else 0
        }
    }

    fun setCurrentIndex(index: Int) {
        if (_currentIndex.value == index) return
        _currentIndex.value = index
        _groupMediaIndex.value = 0
        _showingLivePhotoVideo.value = false
    }

    fun setGroupMediaIndex(index: Int) {
        _groupMediaIndex.value = index
        _showingLivePhotoVideo.value = false
    }

    fun toggleLivePhotoVideo() {
        _showingLivePhotoVideo.value = !_showingLivePhotoVideo.value
    }

    fun loadGroupMediaIfNeeded(groupId: String) {
        if (_groupMediaCache.value.containsKey(groupId)) return
        viewModelScope.launch {
            val media = repo.groupMedia(groupId)
            _groupMediaCache.value = _groupMediaCache.value + (groupId to media)
        }
    }

    fun loadLivePhotoVideoIfNeeded(item: FfiMediaItem) {
        val videoId = item.appleLivePhotoMediaId ?: return
        if (_livePhotoVideoItems.value.containsKey(item.mediaId)) return
        viewModelScope.launch {
            val video = repo.showMedia(videoId)
            _livePhotoVideoItems.value = _livePhotoVideoItems.value + (item.mediaId to video)
        }
    }

    val currentGroupMedia: StateFlow<List<FfiMediaItem>> =
        combine(_currentIndex, items, _groupMediaCache) { idx, list, cache ->
            if (idx == null) return@combine emptyList()
            val entry = (list.getOrNull(idx) as? DetailItem.Group) ?: return@combine emptyList()
            cache[entry.group.groupId] ?: emptyList()
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val displayedMediaId: Flow<String?> =
        combine(_currentIndex, items, _groupMediaCache, _groupMediaIndex) { idx, list, cache, groupIdx ->
            if (idx == null) return@combine null
            when (val entry = list.getOrNull(idx)) {
                is DetailItem.Media -> entry.item.mediaId
                is DetailItem.Group -> cache[entry.group.groupId]?.getOrNull(groupIdx)?.mediaId
                null -> null
            }
        }

    @OptIn(ExperimentalCoroutinesApi::class)
    val currentDisplayItem: StateFlow<FfiMediaItem?> = displayedMediaId.flatMapLatest { mediaId ->
        if (mediaId == null) flowOf(null) else repo.watch(Change.Media(mediaId)) { repo.showMedia(mediaId) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // Metadata display only. Rename, export, and album membership stay on
    // currentDisplayItem, since the paired live-photo video isn't a
    // renamable/exportable library item on its own.
    val infoDisplayItem: StateFlow<FfiMediaItem?> =
        combine(_showingLivePhotoVideo, currentDisplayItem, _livePhotoVideoItems) { showing, display, liveMap ->
            if (showing) liveMap[display?.mediaId] ?: display else display
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    @OptIn(ExperimentalCoroutinesApi::class)
    val containingAlbums: StateFlow<List<FfiAlbum>> = currentDisplayItem.flatMapLatest { item ->
        val mediaId = item?.mediaId
        if (mediaId == null) {
            flowOf(emptyList())
        } else {
            repo.watch(Change.AlbumList, Change.Media(mediaId)) {
                repo.containingAlbums(mediaId, sourceAlbumId)
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun rename(mediaId: String, name: String?) {
        viewModelScope.launch { repo.renameMedia(mediaId, name) }
    }

    companion object {
        fun factory(sourceAlbumId: String?, startMediaId: String): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY]!!
                MediaDetailViewModel(sourceAlbumId, startMediaId, LibraryRepository.from(app))
            }
        }
    }
}
