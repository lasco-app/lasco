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
                assets = listOf(
                    NativePhotoAsset(
                        ticket = "9A9B9C9D",
                        cloudAssetID = "cloud-asset",
                        resources = listOf(
                    NativePhotoResource(
                        ticket = "8A8B8C8D",
                        assetTicket = "9A9B9C9D",
                        role = "primary",
                        type = "PHOTO",
                        filename = "IMG_0001.HEIC",
                        byteCount = 0,
                    ),
                        ),
                    ),
                ),
                collections = listOf(
                    NativePhotoCollection(
                        cloudCollectionID = "cloud-album",
                        kind = "album",
                        name = "Trip",
                    ),
                ),
            ),
        )

        assertTrue(payload.contains("cloudAssetID"))
        assertTrue(payload.contains("ticket"))
        assertTrue(payload.contains("cloudCollectionID"))
        assertFalse(payload.contains("localIdentifier"))
        assertFalse(payload.contains("resourceId"))
        assertFalse(payload.contains("assetId"))
    }
}
