package com.lasco.lasco

import android.app.Application
import com.lasco.lasco.data.LascoRepository
import com.lasco.lasco.data.LibraryRepository

/**
 * Application entry point. Builds the single LascoRepository with the app
 * private directory, which is the app data dir injected into every FFI call.
 *
 * We keep the repository here rather than pulling in a dependency injection
 * framework. ViewModels reach it through LascoRepository.from(application).
 *
 * librarySession is the session scoped repository for whichever library is
 * currently open, null before any library has been opened. The onboarding
 * and library list flows set it once FfiLibrary.open or a create call
 * succeeds. ViewModels reach it through LibraryRepository.from(application).
 */
class LascoApp : Application() {
    lateinit var repository: LascoRepository
        private set

    var librarySession: LibraryRepository? = null

    override fun onCreate() {
        super.onCreate()
        repository = LascoRepository(appDir = filesDir.path)
    }
}
