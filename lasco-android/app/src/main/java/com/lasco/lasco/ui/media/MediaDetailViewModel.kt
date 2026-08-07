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
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.lasco_ffi.FfiAlbum
import uniffi.lasco_ffi.FfiAlbumUuid
import uniffi.lasco_ffi.FfiAlbumItem
import uniffi.lasco_ffi.FfiMediaItem
import uniffi.lasco_ffi.FfiGroupUuid
import uniffi.lasco_ffi.FfiMediaUuid
import uniffi.lasco_ffi.LascoException

private fun FfiAlbumItem.toDetailItem(): DetailItem =
    media?.let(DetailItem::Media) ?: group?.let(DetailItem::Group)
        ?: error("FFI album item contained neither media nor group")

data class DetailNeighbors(
    val previous: DetailItem?,
    val current: DetailItem,
    val next: DetailItem?,
    val currentPosition: Int,
)

sealed interface MediaDetailState {
    data object Loading : MediaDetailState
    data class Content(val neighbors: DetailNeighbors, val navigationInFlight: Boolean = false) : MediaDetailState
    data class Error(val error: Throwable) : MediaDetailState
    data object Empty : MediaDetailState
}

class MediaDetailViewModel(
    private val source: MediaDetailSource,
    startPosition: Int,
    private val repo: LibraryRepository,
) : ViewModel() {
    private val _state = MutableStateFlow<MediaDetailState>(MediaDetailState.Loading)
    val state: StateFlow<MediaDetailState> = _state.asStateFlow()

    private val _groupMediaCache = MutableStateFlow<Map<FfiGroupUuid, List<FfiMediaItem>>>(emptyMap())
    val groupMediaCache: StateFlow<Map<FfiGroupUuid, List<FfiMediaItem>>> = _groupMediaCache.asStateFlow()
    private val _groupMediaIndex = MutableStateFlow(0)
    val groupMediaIndex: StateFlow<Int> = _groupMediaIndex.asStateFlow()
    private val _showingLivePhotoVideo = MutableStateFlow(false)
    val showingLivePhotoVideo: StateFlow<Boolean> = _showingLivePhotoVideo.asStateFlow()
    private val _livePhotoVideoItems = MutableStateFlow<Map<FfiMediaUuid, FfiMediaItem>>(emptyMap())
    private var neighborsJob: Job? = null

    init {
        loadNeighbors(startPosition)
        viewModelScope.launch {
            when (source) {
                MediaDetailSource.HomeByDate -> repo.watch(Change.MediaList) { Unit }
                is MediaDetailSource.AlbumByDate -> repo.watch(
                    Change.Album(FfiAlbumUuid(source.albumId)), Change.AlbumList, Change.MediaList,
                ) { Unit }
            }.collect {
                val position = (_state.value as? MediaDetailState.Content)?.neighbors?.currentPosition ?: startPosition
                loadNeighbors(position)
            }
        }
    }

    fun onPagerSettled(page: Int) {
        val content = _state.value as? MediaDetailState.Content ?: return
        val currentPage = if (content.neighbors.previous == null) 0 else 1
        val delta = page - currentPage
        if (delta !in -1..1 || delta == 0) return
        loadNeighbors(content.neighbors.currentPosition + delta)
    }

    private fun loadNeighbors(position: Int) {
        if (position < 0) return
        neighborsJob?.cancel()
        neighborsJob = viewModelScope.launch {
            _state.value = (_state.value as? MediaDetailState.Content)?.copy(navigationInFlight = true)
                ?: MediaDetailState.Loading
            try {
                val neighbors = when (source) {
                    MediaDetailSource.HomeByDate -> repo.mediaByDateNeighbors(position).let {
                        DetailNeighbors(it.previous?.let(DetailItem::Media), DetailItem.Media(it.current), it.next?.let(DetailItem::Media), position)
                    }
                    is MediaDetailSource.AlbumByDate -> repo.albumItemsByDateNeighbors(FfiAlbumUuid(source.albumId), source.ascending, position).let {
                        DetailNeighbors(it.previous?.toDetailItem(), it.current.toDetailItem(), it.next?.toDetailItem(), position)
                    }
                }
                pruneCaches(neighbors)
                _groupMediaIndex.value = 0
                _showingLivePhotoVideo.value = false
                _state.value = MediaDetailState.Content(neighbors)
            } catch (error: LascoException.NotFound) {
                _state.value = MediaDetailState.Empty
            } catch (error: Throwable) {
                _state.value = MediaDetailState.Error(error)
            }
        }
    }

    private fun pruneCaches(neighbors: DetailNeighbors) {
        val window = listOfNotNull(neighbors.previous, neighbors.current, neighbors.next)
        val groupIds = window.mapNotNull { (it as? DetailItem.Group)?.group?.groupId }.toSet()
        val mediaIds = window.mapNotNull { (it as? DetailItem.Media)?.item?.mediaId }.toSet()
        _groupMediaCache.value = _groupMediaCache.value.filterKeys(groupIds::contains)
        _livePhotoVideoItems.value = _livePhotoVideoItems.value.filterKeys(mediaIds::contains)
    }

    fun setGroupMediaIndex(index: Int) { _groupMediaIndex.value = index; _showingLivePhotoVideo.value = false }
    fun toggleLivePhotoVideo() { _showingLivePhotoVideo.value = !_showingLivePhotoVideo.value }
    fun loadGroupMediaIfNeeded(groupId: FfiGroupUuid) {
        if (_groupMediaCache.value.containsKey(groupId)) return
        viewModelScope.launch { _groupMediaCache.value += groupId to repo.groupMedia(groupId) }
    }
    fun loadLivePhotoVideoIfNeeded(item: FfiMediaItem) {
        val videoId = item.appleLivePhotoMediaId ?: return
        if (_livePhotoVideoItems.value.containsKey(item.mediaId)) return
        viewModelScope.launch { _livePhotoVideoItems.value += item.mediaId to repo.showMedia(videoId) }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private val currentEntry: Flow<DetailItem?> = state.flatMapLatest { value ->
        flowOf((value as? MediaDetailState.Content)?.neighbors?.current)
    }
    val currentGroupMedia: StateFlow<List<FfiMediaItem>> = combine(currentEntry, _groupMediaCache) { entry, cache ->
        (entry as? DetailItem.Group)?.let { cache[it.group.groupId] }.orEmpty()
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
    private val displayedMediaId: Flow<FfiMediaUuid?> = combine(currentEntry, _groupMediaCache, _groupMediaIndex) { entry, cache, index ->
        when (entry) {
            is DetailItem.Media -> entry.item.mediaId
            is DetailItem.Group -> cache[entry.group.groupId]?.getOrNull(index)?.mediaId
            null -> null
        }
    }
    @OptIn(ExperimentalCoroutinesApi::class)
    val currentDisplayItem: StateFlow<FfiMediaItem?> = displayedMediaId.flatMapLatest { id ->
        if (id == null) flowOf(null) else repo.watch(Change.Media(id)) { repo.showMedia(id) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)
    val infoDisplayItem: StateFlow<FfiMediaItem?> = combine(_showingLivePhotoVideo, currentDisplayItem, _livePhotoVideoItems) { showing, display, videos ->
        if (showing) videos[display?.mediaId] ?: display else display
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)
    @OptIn(ExperimentalCoroutinesApi::class)
    val containingAlbums: StateFlow<List<FfiAlbum>> = currentDisplayItem.flatMapLatest { item ->
        if (item == null) flowOf(emptyList()) else repo.watch(Change.AlbumList, Change.Media(item.mediaId)) { repo.containingAlbums(item.mediaId, null) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun rename(mediaId: FfiMediaUuid, name: String?) { viewModelScope.launch { repo.renameMedia(mediaId, name) } }

    companion object {
        fun factory(source: MediaDetailSource, startPosition: Int): ViewModelProvider.Factory = viewModelFactory {
            initializer { MediaDetailViewModel(source, startPosition, LibraryRepository.from(this[APPLICATION_KEY]!!)) }
        }
    }
}
