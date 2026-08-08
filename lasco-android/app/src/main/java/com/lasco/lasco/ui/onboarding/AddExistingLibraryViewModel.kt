package com.lasco.lasco.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.lasco.lasco.LascoApp
import com.lasco.lasco.data.LascoRepository
import com.lasco.lasco.data.LibraryRepository
import com.lasco.lasco.data.Prefs
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class AddExistingLibraryUiState(
    val loading: Boolean = false,
    val error: String? = null,
    val opened: Boolean = false,
)

/**
 * Adds an existing S3 backed library. Unlike create, ffiAddExistingLibraryS3
 * already returns an opened FfiLibrary, mirrors AddExistingLibraryView.
 */
class AddExistingLibraryViewModel(
    private val app: LascoApp,
    private val repository: LascoRepository,
    private val prefs: Prefs,
) : ViewModel() {
    private val _uiState = MutableStateFlow(AddExistingLibraryUiState())
    val uiState: StateFlow<AddExistingLibraryUiState> = _uiState.asStateFlow()

    fun add(
        nickname: String,
        username: String,
        password: String,
        newUsername: String?,
        newPassword: String?,
        remoteName: String,
        endpoint: String,
        bucket: String,
        region: String,
        pathPrefix: String,
        accessKey: String,
        secretKey: String,
    ) {
        _uiState.value = AddExistingLibraryUiState(loading = true)
        viewModelScope.launch {
            try {
                val lib = repository.addExistingLibraryS3(
                    nickname = nickname,
                    username = username,
                    password = password,
                    newUsername = newUsername,
                    newPassword = newPassword,
                    remoteName = remoteName,
                    endpoint = endpoint,
                    bucket = bucket,
                    region = region,
                    pathPrefix = pathPrefix,
                    accessKey = accessKey,
                    secretKey = secretKey,
                )
                val sessionUsername = newUsername ?: username
                app.librarySession =
                    LibraryRepository(lib, nickname = nickname, username = sessionUsername, appDir = repository.appDir, context = app, prefs = prefs)
                _uiState.value = AddExistingLibraryUiState(opened = true)
            } catch (e: Throwable) {
                _uiState.value = AddExistingLibraryUiState(error = e.message ?: "Could not add library")
            }
        }
    }

    companion object {
        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[APPLICATION_KEY] as LascoApp
                AddExistingLibraryViewModel(app, LascoRepository.from(app), Prefs.from(app))
            }
        }
    }
}
