package com.lasco.lasco.data

import uniffi.lasco_ffi.FfiRemote
import uniffi.lasco_ffi.FfiLibraryId
import uniffi.lasco_ffi.FfiRemoteUuid

/**
 * The only slice of the opened library that is genuinely shared across
 * screens, identity, users, remotes and defaults. Media lists and album
 * contents are never held here, each screen pulls its own snapshot through
 * LibraryRepository.watch instead.
 */
data class SessionState(
    val libraryId: FfiLibraryId,
    val nickname: String,
    val username: String?,
    val users: List<String>,
    val remotes: List<FfiRemote>,
    val mediaSourceOrder: List<FfiRemoteUuid>,
    val defaultFetchRemoteId: FfiRemoteUuid?,
    val autoImportDeviceMedia: Boolean,
)
