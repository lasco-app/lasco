package com.lasco.lasco.data

import androidx.paging.PagingSource
import androidx.paging.PagingState

/** Paging source for FFI collections whose range API uses offset/limit. */
class OffsetPagingSource<T : Any>(
    private val count: suspend () -> Int,
    private val range: suspend (offset: Int, limit: Int) -> List<T>,
) : PagingSource<Int, T>() {
    override suspend fun load(params: LoadParams<Int>): LoadResult<Int, T> = try {
        val total = count().coerceAtLeast(0)
        val offset = (params.key ?: 0).coerceIn(0, total)
        val data = if (offset >= total) emptyList() else range(offset, minOf(params.loadSize, total - offset))
        val loaded = data.size
        LoadResult.Page(
            data = data,
            prevKey = if (offset == 0) null else (offset - params.loadSize).coerceAtLeast(0),
            nextKey = if (loaded == 0 || offset + loaded >= total) null else offset + loaded,
            itemsBefore = offset,
            itemsAfter = (total - offset - loaded).coerceAtLeast(0),
        )
    } catch (t: Throwable) {
        LoadResult.Error(t)
    }

    override fun getRefreshKey(state: PagingState<Int, T>): Int? =
        state.anchorPosition?.let { anchor ->
            state.closestPageToPosition(anchor)?.let { page ->
                page.prevKey?.plus(page.data.size) ?: page.nextKey?.minus(page.data.size)
            } ?: anchor
        }
}
