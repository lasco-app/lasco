package app.lasco.importer.source

/** Keeps PhotoKit mapping requests bounded and makes the batching contract independently testable. */
internal const val photoKitCloudMappingBatchSize = 250

internal fun <Value> batchedCloudMappings(
    localIdentifiers: List<String>,
    resolveBatch: (List<String>) -> Map<String, Value>,
): Map<String, Value> = buildMap {
    localIdentifiers.distinct().chunked(photoKitCloudMappingBatchSize).forEach { batch ->
        putAll(resolveBatch(batch))
    }
}
