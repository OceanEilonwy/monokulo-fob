package com.monokulo.fob.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.monokulo.fob.R

/** One theme's colour roles. Screens read them through [Fob]. */
@Immutable
class FobColors(
    val dark: Boolean,
    val paper: Color,
    val card: Color,
    val sunken: Color,
    val line: Color,
    val ink: Color,
    val muted: Color,
    val outline: Color,
    val orangeText: Color,
    val green: Color,
    val greenDot: Color,
    val greenFill: Color,
    /** What sits on [greenFill]: the tick on Paid. */
    val onGreen: Color,
    val highlight: Color,
    /**
     * Behind a warning that stops someone losing money or keys: a stronger
     * orange tint than [highlight], with ink words and an [orangeText] mark.
     */
    val warning: Color,
    /** Behind a choice that's on, with an [orangeText] edge: Split up's Auto while the coins are its mix. */
    val selectedFill: Color,
    /** The first, middle and last groups of an address on Receive. */
    val addressMark: Color,
    val onAddressMark: Color,
    val red: Color,
    /** What sits on [red] filled: Delete's words. */
    val onRed: Color,
    /** The empty part of the sync ring. */
    val track: Color,
    /** Over the screen behind a sheet that asks something. */
    val scrim: Color,
    /** The price toast: dark on a light page, light on a dark one, with its words, button and marks. */
    val inverse: Color,
    val onInverse: Color,
    val inverseMuted: Color,
    val inverseAccent: Color,
    val inverseGreen: Color,
)

/** The Monokulo family's colours, as in the Fob designs. */
val LightColors = FobColors(
    dark = false,
    paper = Color(0xFFF4F1EA),
    card = Color(0xFFFFFFFF),
    sunken = Color(0xFFEFEAE0),
    line = Color(0xFFE2DCCF),
    ink = Color(0xFF1A1917),
    muted = Color(0xFF5A564C),
    outline = Color(0xFF8C8577),
    orangeText = Color(0xFFB34700),
    green = Color(0xFF1F7A45),
    greenDot = Color(0xFF1F8A4C),
    greenFill = Color(0xFF4CC27A),
    onGreen = Color(0xFF1A1917),
    highlight = Color(0xFFFBF3E8),
    warning = Color(0xFFF7D6B4),
    selectedFill = Color(0xFFFBE3CF),
    addressMark = Color(0xFFFBE3CC),
    onAddressMark = Color(0xFF7A2F00),
    red = Color(0xFFB3261E),
    onRed = Color(0xFFFFFFFF),
    track = Color(0xFFD8D1C3),
    scrim = Color(0x731A1917),
    inverse = Color(0xFF1A1917),
    onInverse = Color(0xFFF4F1EA),
    inverseMuted = Color(0xFFB0A999),
    inverseAccent = Color(0xFFFF8F45),
    inverseGreen = Color(0xFF5CC88A),
)

/** The same roles at night: warm greys, no black, the same orange. */
val DarkColors = FobColors(
    dark = true,
    paper = Color(0xFF141311),
    card = Color(0xFF1F1D1A),
    sunken = Color(0xFF2A2824),
    line = Color(0xFF3A3731),
    ink = Color(0xFFF2EEE6),
    muted = Color(0xFFB0A999),
    outline = Color(0xFF7A7366),
    orangeText = Color(0xFFFF8F45),
    green = Color(0xFF5CC88A),
    greenDot = Color(0xFF5CC88A),
    greenFill = Color(0xFF5CC88A),
    onGreen = Color(0xFF111111),
    highlight = Color(0xFF2E2418),
    warning = Color(0xFF46301A),
    selectedFill = Color(0xFF452812),
    addressMark = Color(0xFF4A2A12),
    onAddressMark = Color(0xFFFFC9A0),
    red = Color(0xFFF2B8B5),
    onRed = Color(0xFF601410),
    track = Color(0xFF3A3731),
    scrim = Color(0x99080706),
    inverse = Color(0xFFF2EEE6),
    onInverse = Color(0xFF1A1917),
    inverseMuted = Color(0xFF5A564C),
    inverseAccent = Color(0xFFB34700),
    inverseGreen = Color(0xFF1F7A45),
)

val LocalFobColors = staticCompositionLocalOf { LightColors }

/**
 * The current theme's colours, by role: `Fob.ink`, `Fob.card`. Roles that
 * read the theme are composable; the rest are the same in both themes.
 */
object Fob {
    val paper: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.paper
    val card: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.card
    val sunken: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.sunken
    val line: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.line
    val ink: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.ink
    val muted: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.muted
    val outline: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.outline
    val orangeText: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.orangeText
    val green: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.green
    val greenDot: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.greenDot
    val greenFill: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.greenFill
    val onGreen: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.onGreen
    val highlight: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.highlight
    val warning: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.warning
    val selectedFill: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.selectedFill
    val addressMark: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.addressMark
    val onAddressMark: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.onAddressMark
    val red: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.red
    val track: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.track
    val onRed: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.onRed
    val scrim: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.scrim
    val inverse: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.inverse
    val onInverse: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.onInverse
    val inverseMuted: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.inverseMuted
    val inverseAccent: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.inverseAccent
    val inverseGreen: Color @Composable @ReadOnlyComposable get() = LocalFobColors.current.inverseGreen

    val orange = Color(0xFFFF6600)
    val onOrange = Color(0xFF111111)

    /** The camera's backdrop and what's drawn over it, in both themes. */
    val camera = Color(0xFF2B2925)
    val onCamera = Color(0xFFF4F1EA)
    val cameraHint = Color(0xFFE2DCCF)
    val cameraCard = Color(0xFF3A3732)
    val cameraWarning = Color(0xFFFF8A3D)
    /** Over the camera's picture: a light shade, deeper under the top bar, so what's drawn over it reads on a white page too. */
    val cameraShade = Color(0x402B2925)
    val cameraShadeTop = Color(0xB32B2925)

    /**
     * The address pattern: dots in one of eight colours (Rust picks the
     * index) on a white badge, the same in both themes so two phones match.
     */
    val patternBadge = Color(0xFFFFFFFF)
    val patternColours = listOf(
        Color(0xFFB34700), Color(0xFF1F7A45), Color(0xFF2F5AA8), Color(0xFF7A3FA0),
        Color(0xFFA3324A), Color(0xFF1D6F7A), Color(0xFF5B5F17), Color(0xFF3D3A8C),
    )

    /** QR codes stay dark on white in both themes, so every reader reads them. */
    val qrDark = Color(0xFF1A1917)
    val qrLight = Color(0xFFFFFFFF)
    /** Quieter words on the white of the expanded code, in both themes. */
    val qrMuted = Color(0xFF5A564C)
    /** Touch phones' strip on the white of Share my profile, in both themes, with its orange phone. */
    val qrStrip = Color(0xFFEFEAE0)
    val qrAccent = Color(0xFFB34700)
}

/** Which theme Fob draws in; kept on the phone (see [WalletModel.theme]). */
enum class ThemeChoice(val label: String) {
    PHONE("Same as the phone"),
    LIGHT("Light"),
    DARK("Dark"),
}

/** Manrope is one variable font file: each weight is a variation of it. */
private fun manrope(weight: FontWeight) = Font(R.font.manrope, weight, variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)))

val Manrope = FontFamily(manrope(FontWeight.Medium), manrope(FontWeight.Bold), manrope(FontWeight.ExtraBold))

/** Big amounts and titles: Manrope, extra bold. */
fun display(size: Int) = TextStyle(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold, fontSize = size.sp, letterSpacing = (-size / 36.0).sp)

@Composable
fun FobTheme(choice: ThemeChoice = ThemeChoice.PHONE, content: @Composable () -> Unit) {
    val dark = when (choice) {
        ThemeChoice.PHONE -> isSystemInDarkTheme()
        ThemeChoice.LIGHT -> false
        ThemeChoice.DARK -> true
    }
    val c = if (dark) DarkColors else LightColors
    val roles = if (dark) darkColorScheme() else lightColorScheme()
    MaterialTheme(
        colorScheme = roles.copy(
            primary = Fob.orange,
            onPrimary = Fob.onOrange,
            background = c.paper,
            onBackground = c.ink,
            surface = c.card,
            onSurface = c.ink,
            surfaceContainerHigh = c.card,
            surfaceVariant = c.sunken,
            onSurfaceVariant = c.muted,
            outline = c.outline,
            error = c.red,
        ),
    ) {
        CompositionLocalProvider(LocalFobColors provides c, LocalContentColor provides c.ink, content = content)
    }
}
