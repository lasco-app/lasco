package com.lasco.lasco

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.lasco.lasco.ui.LascoRoot
import com.lasco.lasco.ui.theme.LascoTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            var libraryOpen by remember { mutableStateOf(false) }
            LascoTheme(darkTheme = libraryOpen) {
                Scaffold(modifier = Modifier.fillMaxSize()) { innerPadding ->
                    LascoRoot(
                        modifier = Modifier.padding(innerPadding),
                        onLibraryOpenChanged = { libraryOpen = it },
                    )
                }
            }
        }
    }
}
