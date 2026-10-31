package com.monokulo.fob.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.monokulo.fob.R

/** The Monokulo family's colours, as in the Fob designs. */
object Fob {
    val paper = Color(0xFFF4F1EA)
    val card = Color(0xFFFFFFFF)
    val sunken = Color(0xFFEFEAE0)
    val line = Color(0xFFE2DCCF)
    val ink = Color(0xFF1A1917)
    val muted = Color(0xFF5A564C)
    val outline = Color(0xFF8C8577)
    val orange = Color(0xFFFF6600)
    val onOrange = Color(0xFF111111)
    val orangeText = Color(0xFFB34700)
    val green = Color(0xFF1F7A45)
    val greenDot = Color(0xFF1F8A4C)
    val greenFill = Color(0xFF4CC27A)
    val camera = Color(0xFF2B2925)
    val highlight = Color(0xFFFBF3E8)
    val red = Color(0xFFB3261E)
}

/** Manrope is one variable font file: each weight is a variation of it. */
private fun manrope(weight: FontWeight) = Font(R.font.manrope, weight, variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)))

val Manrope = FontFamily(manrope(FontWeight.Medium), manrope(FontWeight.Bold), manrope(FontWeight.ExtraBold))

/** Big amounts and titles: Manrope, extra bold. */
fun display(size: Int) = TextStyle(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold, fontSize = size.sp, letterSpacing = (-size / 36.0).sp)

@Composable
fun FobTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = lightColorScheme(
            primary = Fob.orange,
            onPrimary = Fob.onOrange,
            background = Fob.paper,
            onBackground = Fob.ink,
            surface = Fob.card,
            onSurface = Fob.ink,
            surfaceVariant = Fob.sunken,
            onSurfaceVariant = Fob.muted,
            outline = Fob.outline,
            error = Fob.red,
        ),
        content = content,
    )
}
