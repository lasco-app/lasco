package app.lasco.importer.source

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ApplePhotosBridgeWireFormatTest {
    @Test
    fun `bridge discovery JSON exposes only cloud identities and opaque handles`() {
        val payload = Json.encodeToString(
            NativePhotoDiscovery(
                resources = listOf(
                    NativePhotoResource(
                        sessionHandle = "8A8B8C8D",
                        assetSessionHandle = "9A9B9C9D",
                        type = "primary",
                        filename = "IMG_0001.HEIC",
                        byteCount = 0,
                        cloudAssetId = "cloud-asset",
                        resourceType = "PHOTO",
                    ),
                ),
                collections = listOf(
                    NativePhotoCollection(
                        cloudCollectionId = "cloud-album",
                        kind = "ALBUM",
                        name = "Trip",
                    ),
                ),
            ),
        )

        assertTrue(payload.contains("cloudAssetId"))
        assertTrue(payload.contains("sessionHandle"))
        assertTrue(payload.contains("cloudCollectionId"))
        assertFalse(payload.contains("localIdentifier"))
        assertFalse(payload.contains("resourceId"))
        assertFalse(payload.contains("assetId"))
    }
}
