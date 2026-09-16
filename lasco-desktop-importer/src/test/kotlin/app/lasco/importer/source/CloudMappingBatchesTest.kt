package app.lasco.importer.source

import kotlin.test.Test
import kotlin.test.assertEquals

class CloudMappingBatchesTest {
    @Test
    fun `mapping uses bounded distinct batches and preserves successful mappings`() {
        val identifiers = (1..501).map { "local-$it" } + listOf("local-1", "local-2")
        val requested = mutableListOf<List<String>>()

        val mappings = batchedCloudMappings(identifiers) { batch ->
            requested += batch
            batch.filter { it != "local-400" }.associateWith { "cloud-$it" }
        }

        assertEquals(listOf(250, 250, 1), requested.map { it.size })
        assertEquals(501, requested.flatten().toSet().size)
        assertEquals("cloud-local-1", mappings["local-1"])
        assertEquals(null, mappings["local-400"])
    }
}
