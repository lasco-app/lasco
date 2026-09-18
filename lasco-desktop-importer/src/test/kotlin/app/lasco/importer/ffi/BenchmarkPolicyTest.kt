package app.lasco.importer.ffi

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class BenchmarkPolicyTest {
    @Test
    fun `each benchmark upload is two MiB`() {
        assertEquals(2L * 1024L * 1024L, BENCHMARK_BYTES_PER_UPLOAD)
    }

    @Test
    fun `parallel benchmarking is skipped below one MiB per second`() {
        assertTrue(shouldRestrictToOneUpload(MINIMUM_SPEED_FOR_PARALLEL_UPLOADS - 1))
        assertFalse(shouldRestrictToOneUpload(MINIMUM_SPEED_FOR_PARALLEL_UPLOADS))
    }
}
