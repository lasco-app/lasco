package com.lasco.lasco.data

import uniffi.lasco_ffi.FfiRemote

/**
 * The only slice of the opened library that is genuinely shared across
 * screens, identity, users, remotes and defaults. Media lists and album
 * contents are never held here, each screen pulls its own snapshot through
 * LibraryRepository.watch instead.
 */
data class SessionState(
    val libraryId: String,
    val nickname: String,
    val username: String?,
    val users: List<String>,
    val remotes: List<FfiRemote>,
    val defaultFetchRemoteId: String?,
    val autoImportDeviceMedia: Boolean,
)
