package com.lasco.lasco.ui.theme

import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.lasco.lasco.R

/**
 * Font families ported from the Swift app. Same four typefaces, so the two
 * apps render the same text. The files live in res/font.
 */
val Jersey10 = FontFamily(Font(R.font.jersey10_regular))
val VT323 = FontFamily(Font(R.font.vt323_regular))
val SpaceGrotesk = FontFamily(
    Font(R.font.space_grotesk_regular, FontWeight.Normal),
    Font(R.font.space_grotesk_bold, FontWeight.Bold),
)
val JetBrainsMono = FontFamily(Font(R.font.jetbrains_mono_regular))

/**
 * Text style helpers mirroring the Swift LascoFont enum. Compose has no direct
 * equivalent of a font that carries only a family, so each helper takes a size
 * and returns a full TextStyle. Defaults match the Swift defaults.
 */
object LascoType {
    // Jersey 10, all caps pixel titles.
    fun categoryLarge(size: Int = 36) = TextStyle(fontFamily = Jersey10, fontSize = size.sp)
    fun categorySmall(size: Int = 22) = TextStyle(fontFamily = Jersey10, fontSize = size.sp)

    // VT323, pixel subtitle, metadata, overlays.
    fun subtitle(size: Int = 18) = TextStyle(fontFamily = VT323, fontSize = size.sp)
    fun pixel(size: Int = 15) = TextStyle(fontFamily = VT323, fontSize = size.sp)

    // Space Grotesk, statement titles and body.
    fun title(size: Int = 22) =
        TextStyle(fontFamily = SpaceGrotesk, fontWeight = FontWeight.Bold, fontSize = size.sp)

    fun body(size: Int = 15) =
        TextStyle(fontFamily = SpaceGrotesk, fontWeight = FontWeight.Normal, fontSize = size.sp)

    val button = TextStyle(fontFamily = SpaceGrotesk, fontWeight = FontWeight.Bold, fontSize = 14.sp)

    // JetBrains Mono, paths, sizes, timestamps.
    fun mono(size: Int = 12) = TextStyle(fontFamily = JetBrainsMono, fontSize = size.sp)
}
