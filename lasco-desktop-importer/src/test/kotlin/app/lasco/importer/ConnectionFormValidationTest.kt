package app.lasco.importer

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ConnectionFormValidationTest {
    @Test
    fun `cloud form requires library and cloud credentials`() {
        assertFalse(validForm(RemoteType.CLOUD, cloudPassword = ""))
        assertTrue(validForm(RemoteType.CLOUD))
    }

    @Test
    fun `s3 form requires its connection credentials`() {
        assertFalse(validForm(RemoteType.S3, secretKey = ""))
        assertTrue(validForm(RemoteType.S3))
    }

    @Test
    fun `smb form requires a valid port`() {
        assertFalse(validForm(RemoteType.SMB, port = "0"))
        assertFalse(validForm(RemoteType.SMB, port = "65536"))
        assertTrue(validForm(RemoteType.SMB))
    }

    private fun validForm(
        type: RemoteType,
        cloudPassword: String = "cloud-password",
        secretKey: String = "secret-key",
        port: String = "445",
    ) = isConnectionFormComplete(
        type = type,
        nickname = "family-library",
        libraryUser = "library-user",
        libraryPassword = "library-password",
        remoteName = "remote",
        cloudEmail = "person@example.com",
        cloudPassword = cloudPassword,
        endpoint = "https://s3.example.com",
        bucket = "bucket",
        region = "eu-west-3",
        accessKey = "access-key",
        secretKey = secretKey,
        server = "nas.local",
        port = port,
        share = "lasco",
        smbUser = "smb-user",
        smbPassword = "smb-password",
    )
}
