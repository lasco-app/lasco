package com.lasco.lasco.media

import android.content.Context
import android.os.Build
import android.provider.MediaStore
import android.provider.MediaStore.Files.FileColumns
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// Camera folder only. Albums and other DCIM subfolders on the device are not
// scanned, matching the "camera folder only" decision for initial import.
private const val PATH_PREFIX_Q = "DCIM/%"
private const val PATH_PREFIX_LEGACY = "%/DCIM/%"

data class DeviceMediaRow(
    val id: Long,
    val displayName: String?,
    val size: Long,
    val dateAdded: Long,
    val mimeType: String?,
    val isVideo: Boolean,
)

data class DeviceScan(
    val photoCount: Int,
    val videoCount: Int,
    val ignoredCount: Int,
    val totalBytes: Long,
    val maxDateAdded: Long,
    val rows: List<DeviceMediaRow>,
)

/**
 * Pure MediaStore access, no Lasco types, kept separate from
 * DeviceImportController so it is testable on its own.
 */
class DeviceMediaStore(private val context: Context) {
    // null scans everything (initial import). A value restricts to rows added
    // after the watermark, serving the incremental path later with the same query.
    suspend fun scan(sinceDateAdded: Long? = null): DeviceScan = withContext(Dispatchers.IO) {
        val uri = MediaStore.Files.getContentUri("external")
        val projection = arrayOf(
            FileColumns._ID,
            FileColumns.DISPLAY_NAME,
            FileColumns.SIZE,
            FileColumns.DATE_ADDED,
            FileColumns.MIME_TYPE,
            FileColumns.MEDIA_TYPE,
        )

        val selectionParts = mutableListOf(
            "(${FileColumns.MEDIA_TYPE} = ? OR ${FileColumns.MEDIA_TYPE} = ?)",
        )
        val selectionArgs = mutableListOf(
            FileColumns.MEDIA_TYPE_IMAGE.toString(),
            FileColumns.MEDIA_TYPE_VIDEO.toString(),
        )

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            selectionParts += "${FileColumns.RELATIVE_PATH} LIKE ?"
            selectionArgs += PATH_PREFIX_Q
        } else {
            @Suppress("DEPRECATION")
            selectionParts += "${FileColumns.DATA} LIKE ?"
            selectionArgs += PATH_PREFIX_LEGACY
        }

        if (sinceDateAdded != null) {
            selectionParts += "${FileColumns.DATE_ADDED} > ?"
            selectionArgs += sinceDateAdded.toString()
        }

        var photoCount = 0
        var videoCount = 0
        var ignoredCount = 0
        var totalBytes = 0L
        var maxDateAdded = sinceDateAdded ?: 0L
        val rows = mutableListOf<DeviceMediaRow>()

        context.contentResolver.query(
            uri,
            projection,
            selectionParts.joinToString(" AND "),
            selectionArgs.toTypedArray(),
            null,
        )?.use { cursor ->
            val idIdx = cursor.getColumnIndexOrThrow(FileColumns._ID)
            val nameIdx = cursor.getColumnIndexOrThrow(FileColumns.DISPLAY_NAME)
            val sizeIdx = cursor.getColumnIndexOrThrow(FileColumns.SIZE)
            val dateAddedIdx = cursor.getColumnIndexOrThrow(FileColumns.DATE_ADDED)
            val mimeIdx = cursor.getColumnIndexOrThrow(FileColumns.MIME_TYPE)
            val mediaTypeIdx = cursor.getColumnIndexOrThrow(FileColumns.MEDIA_TYPE)

            // Trashed and pending rows are left out by MediaStore itself
            // unless the query asks for them, so there is nothing to filter
            // here beyond rows with no bytes yet.
            while (cursor.moveToNext()) {
                val size = cursor.getLong(sizeIdx)
                val dateAdded = cursor.getLong(dateAddedIdx)

                if (size <= 0) {
                    ignoredCount++
                    continue
                }

                val isVideo = cursor.getInt(mediaTypeIdx) == FileColumns.MEDIA_TYPE_VIDEO
                if (isVideo) videoCount++ else photoCount++
                totalBytes += size
                if (dateAdded > maxDateAdded) maxDateAdded = dateAdded

                rows += DeviceMediaRow(
                    id = cursor.getLong(idIdx),
                    displayName = cursor.getString(nameIdx),
                    size = size,
                    dateAdded = dateAdded,
                    mimeType = cursor.getString(mimeIdx),
                    isVideo = isVideo,
                )
            }
        }

        DeviceScan(
            photoCount = photoCount,
            videoCount = videoCount,
            ignoredCount = ignoredCount,
            totalBytes = totalBytes,
            maxDateAdded = maxDateAdded,
            rows = rows,
        )
    }

    fun contentUriFor(row: DeviceMediaRow) =
        android.content.ContentUris.withAppendedId(
            if (row.isVideo) MediaStore.Video.Media.EXTERNAL_CONTENT_URI else MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
            row.id,
        )
}
