package com.monokulo.fob.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.layout.Layout
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import uniffi.fob.QrCode
import kotlin.math.PI
import kotlin.math.cos
import uniffi.fob.addressPattern
import kotlin.math.roundToInt
import kotlin.math.sin

/** The stroke icons from the designs, as SVG path data on a 24 × 24 grid. */
object Icons {
    const val back = "M20 12H5M11 6l-6 6 6 6"
    const val close = "M6 6l12 12M18 6L6 18"
    /** Monokular's cog: eight teeth round a hole (drawn at stroke 1.8). */
    const val gear = "M9.92 5.21L10.5 2.52A9.6 9.6 0 0 1 13.5 2.52L14.08 5.21A7.1 7.1 0 0 1 15.33 5.73L17.64 4.23A9.6 9.6 0 0 1 19.77 6.36L18.27 8.67A7.1 7.1 0 0 1 18.79 9.92L21.48 10.5A9.6 9.6 0 0 1 21.48 13.5L18.79 14.08A7.1 7.1 0 0 1 18.27 15.33L19.77 17.64A9.6 9.6 0 0 1 17.64 19.77L15.33 18.27A7.1 7.1 0 0 1 14.08 18.79L13.5 21.48A9.6 9.6 0 0 1 10.5 21.48L9.92 18.79A7.1 7.1 0 0 1 8.67 18.27L6.36 19.77A9.6 9.6 0 0 1 4.23 17.64L5.73 15.33A7.1 7.1 0 0 1 5.21 14.08L2.52 13.5A9.6 9.6 0 0 1 2.52 10.5L5.21 9.92A7.1 7.1 0 0 1 5.73 8.67L4.23 6.36A9.6 9.6 0 0 1 6.36 4.23L8.67 5.73A7.1 7.1 0 0 1 9.92 5.21Z M12 8.9a3.1 3.1 0 1 0 0.01 0Z"
    const val scan = "M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3M4 12h16"
    const val down = "M12 4v13M6 11l6 6 6-6"
    const val up = "M12 20V7M6 13l6-6 6 6"
    const val receipt = "M6 3h12v18l-3-2-3 2-3-2-3 2V3Z M9 8h6 M9 12h6 M9 16h3"
    const val chevron = "M9 5l7 7-7 7"
    const val arrow = "M5 12h13 M13 6l6 6-6 6"
    const val lines = "M4 6h16 M4 10h16 M4 14h10 M4 18h7"
    const val eye = "M2.5 12s3.5-6.5 9.5-6.5S21.5 12 21.5 12s-3.5 6.5-9.5 6.5S2.5 12 2.5 12Z M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6"
    const val share = "M12 15V4M7 8l5-5 5 5M5 13v6a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-6"
    const val warning = "M12 3l9.5 17h-19L12 3Z M12 10v4.5 M12 17.5v.5"
    const val check = "M5 12.5l4.5 4.5L19 7.5"
    const val expand = "M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7"
    const val plus = "M12 5v14M5 12h14"
    /** A circling arrow: check the node again. */
    const val refresh = "M20 11a8 8 0 1 0-2.3 5.7 M20 4v7h-7"
    /** Three dots: a node check still running. */
    const val dots = "M6 12h.5 M12 12h.5 M18 12h.5"
    /** An exclamation mark: a node check that failed. */
    const val alert = "M12 7v6 M12 17v.5"
    /** A dash: a node check not run. */
    const val dash = "M8 12h8"
    const val search = "M10.5 4a6.5 6.5 0 1 0 0.01 0Z M15.5 15.5L20 20"
    /** A camera: Take a photo, and the badge on a contact's photo. */
    const val camera = "M4 8h3l2-3h6l2 3h3v11H4Z M12 10.5a3 3 0 1 0 0.01 0Z"
    /** A picture: Choose from your photos. */
    const val picture = "M4 5h16v14H4Z M4 16l5-5 4 4 2-2 5 5 M15.5 8.5a1.5 1.5 0 1 0 0.01 0Z"
    const val trash = "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"
    /** A person with a plus: Save as a contact. */
    const val personPlus = "M6.5 8a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0Z M3.5 19.5c0.6-3.4 3.3-5.5 6.5-5.5s5.9 2.1 6.5 5.5 M19 8v6 M16 11h6"
    /** A person: a share of your profile in Activity, and Receive's line naming you. */
    const val person = "M8.5 8a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0Z M5.5 19.5c0.6-3.4 3.3-5.5 6.5-5.5s5.9 2.1 6.5 5.5"
    /** Two links of a chain: a share's link copied or shared. */
    const val link = "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1 M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"
    /** A phone with waves (drawn at stroke 1.8): touch phones, and a share made that way. */
    const val touch = "M7 3h7a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z M18.5 8.5a5 5 0 0 1 0 7 M20.8 6.2a8.2 8.2 0 0 1 0 11.6"
    /** A payment code, drawn loose: a request in Activity. */
    const val request = "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h2 M18 14h2 M14 18h2 M18 18h2"
    /** Two people (drawn at stroke 1.8): Contacts in Settings. */
    const val people = "M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z M2 21c0-4 3-7 7-7s7 3 7 7 M16 3.5a4 4 0 0 1 0 7.5 M18 14.5c2.4.8 4 3.2 4 6.5"
    /** Two columns of words (drawn at stroke 1.8): Seed phrase in Backup and keys. */
    const val seedWords = "M4 6h7 M4 10h7 M4 14h7 M4 18h7 M14 6h6 M14 10h6 M14 14h6 M14 18h6"
    /** Two sheets, one over the other (drawn at stroke 1.8): Copy. */
    const val copy = "M9 9h11v11H9z M5 15H4V4h11v1"
    /** A key (drawn at stroke 1.8): Backup and keys. */
    const val key = "M8 15a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z M12 11h9 M18 11v3 M15 11v2"
    /** A globe (drawn at stroke 1.8): a price provider that answers over the web. */
    const val globe = "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z M3 12h18 M12 3c3 3 3 15 0 18 M12 3c-3 3-3 15 0 18"
    /** An onion (drawn at stroke 1.6): one that answers over Tor. */
    const val onion = "M12 3c-1 2-1 3 0 4 M12 7c-4 0-7 3.5-7 7.5S8 21 12 21s7-2.5 7-6.5S16 7 12 7Z M12 10c-2.5 0-4 2-4 4.5S9.5 18 12 18s4-1 4-3.5S14.5 10 12 10Z M12 13c-1 0-1.5.7-1.5 1.5S11 16 12 16s1.5-.7 1.5-1.5S13 13 12 13Z"
    /** Two coins, one behind the other (drawn at stroke 1.8). */
    const val coins = "M2.5 13a6.5 6.5 0 1 0 13 0a6.5 6.5 0 1 0 -13 0Z M6.5 13a2.5 2.5 0 1 0 5 0a2.5 2.5 0 1 0 -5 0Z M13.2 4.3A6.5 6.5 0 1 1 17.6 16.4"
    /** A four-pointed sparkle, drawn filled: Split up's Auto. */
    const val sparkle = "M12 3l1.9 5.6L19.5 10.5l-5.6 1.9L12 18l-1.9-5.6L4.5 10.5l5.6-1.9z"
    /** One coin splitting into three (drawn at stroke 1.8). */
    const val split = "M2.5 12a4.5 4.5 0 1 0 9 0a4.5 4.5 0 1 0 -9 0Z M16.8 5a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0Z M16.8 12a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0Z M16.8 19a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0Z M11.5 12h5.3 M10.6 9.6l6.5-3.6 M10.6 14.4l6.5 3.6"
}

@Composable
fun icon(path: String, width: Float = 2f): ImageVector = remember(path, width) {
    ImageVector.Builder(defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
        .addPath(
            pathData = PathParser().parsePathString(path).toNodes(),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = width,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        )
        .build()
}

/** The shape [path] filled with [tint], on the same 24 × 24 grid as [Icons]. */
@Composable
fun FilledIcon(path: String, tint: Color = Fob.ink, size: Dp = 22.dp) {
    val vector = remember(path) {
        ImageVector.Builder(defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
            .addPath(pathData = PathParser().parsePathString(path).toNodes(), fill = SolidColor(Color.Black))
            .build()
    }
    Icon(vector, contentDescription = null, tint = tint, modifier = Modifier.size(size))
}

@Composable
fun StrokeIcon(path: String, tint: Color = Fob.ink, size: Dp = 22.dp, width: Float = 2f) {
    Icon(icon(path, width), contentDescription = null, tint = tint, modifier = Modifier.size(size))
}

/**
 * A whole screen: paper, clear of the system bars, scrolling if it must.
 * With [onRefresh], pulling down from the top calls it; the spinner shows
 * while [refreshing].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Page(
    background: Color = Fob.paper,
    scroll: Boolean = true,
    refreshing: Boolean = false,
    onRefresh: (() -> Unit)? = null,
    bottom: (@Composable ColumnScope.() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(Modifier.fillMaxSize().background(background).safeDrawingPadding()) {
        val scrolling = Modifier.fillMaxWidth().let { if (scroll) it.verticalScroll(rememberScrollState()) else it }
        if (onRefresh == null) {
            Column(Modifier.weight(1f).then(scrolling), content = content)
        } else {
            val state = rememberPullToRefreshState()
            PullToRefreshBox(
                isRefreshing = refreshing, onRefresh = onRefresh, state = state, modifier = Modifier.weight(1f).fillMaxWidth(),
                indicator = {
                    PullToRefreshDefaults.Indicator(
                        state = state, isRefreshing = refreshing, containerColor = Fob.card, color = Fob.orangeText,
                        modifier = Modifier.align(Alignment.TopCenter),
                    )
                },
            ) {
                Column(Modifier.fillMaxSize().then(scrolling), content = content)
            }
        }
        if (bottom != null) {
            // The price toast rises above it.
            Column(
                Modifier.fillMaxWidth().bottomSpace().padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 20.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp), content = bottom,
            )
        }
    }
}

/**
 * The buttons at a screen's edges (round 8 › item 2): Back or Close, the top
 * bar's actions, Home's photo and Settings, a scanner's Close and torch. One
 * size and inset for all, so they sit in from the edge and are easy to hit:
 * an icon-only button is a [target] circle with an [icon] inside, its row
 * [inset] in from the screen's edge (a button's centre 44dp in), a photo is
 * [avatar] inside the target, a text action at least [textHeight] tall.
 */
object EdgeButton {
    val target = 56.dp
    val icon = 26.dp
    val avatar = 40.dp
    val inset = 16.dp
    val textHeight = 48.dp
}

/** An edge button's circle: [EdgeButton.target] wide, [content] centred; TalkBack reads [label]. */
@Composable
fun EdgeIconButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, content: @Composable () -> Unit) {
    Box(
        modifier.size(EdgeButton.target).clip(CircleShape).clickable(enabled = enabled, role = Role.Button, onClick = onClick).semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) { content() }
}

/** An edge button showing the icon [path] at [EdgeButton.icon]. */
@Composable
fun EdgeIconButton(path: String, label: String, onClick: () -> Unit, modifier: Modifier = Modifier, tint: Color = Fob.ink, width: Float = 2f) {
    EdgeIconButton(label, onClick, modifier) { StrokeIcon(path, tint = tint, size = EdgeButton.icon, width = width) }
}

/**
 * A screen's top bar: Back (or Close), the title, then [actions], the row
 * [EdgeButton.inset] in from both edges. A long title or a big font
 * ellipsizes the title; the buttons keep their size.
 */
@Composable
fun TopBar(title: String?, onBack: (() -> Unit)?, closeIcon: Boolean = false, tint: Color = Fob.ink, actions: @Composable RowScope.() -> Unit = {}) {
    Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically) {
        if (onBack != null) {
            EdgeIconButton(if (closeIcon) Icons.close else Icons.back, if (closeIcon) "Close" else "Back", onBack, tint = tint)
        } else {
            Spacer(Modifier.width(4.dp))
        }
        Text(title ?: "", color = tint, fontSize = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        actions()
    }
}

@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 56.dp, leading: String? = null, iconWidth: Float = 2f) {
    Row(
        modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2))
            .background(if (enabled) Fob.orange else Fob.sunken)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            // A long label ("Share with Alexandria …") ellipsizes inside the pill's curve, as OutlineButton's does.
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null) {
            StrokeIcon(leading, tint = if (enabled) Fob.onOrange else Fob.muted, width = iconWidth)
            Spacer(Modifier.width(10.dp))
        }
        Text(text, color = if (enabled) Fob.onOrange else Fob.muted, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * A warning that stops someone losing money or keys, one style everywhere
 * (Backup and keys, a new wallet's seed phrase and Home's backup reminder,
 * the mainnet and remove dialogs, a contact's replaced address): the
 * [Fob.warning] tint, an orange mark, ink words at body size. No edge or
 * outline. With [title] and [onClick] it is a card that opens something,
 * with a chevron. Over the camera it takes the camera's colours.
 */
@Composable
fun Warning(
    text: String, modifier: Modifier = Modifier, title: String? = null, icon: String = Icons.warning,
    surface: Color = Fob.warning, ink: Color = Fob.ink, mark: Color = Fob.orangeText, onClick: (() -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(surface)
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick).semantics(mergeDescendants = true) {} else Modifier)
            .padding(start = 16.dp, end = if (onClick != null) 12.dp else 16.dp, top = 14.dp, bottom = 14.dp),
        verticalAlignment = if (onClick != null) Alignment.CenterVertically else Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        StrokeIcon(icon, tint = mark, size = 24.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            title?.let { Text(it, color = ink, fontSize = 15.sp, lineHeight = 21.sp, fontWeight = FontWeight.SemiBold) }
            Text(text, color = ink, fontSize = 15.sp, lineHeight = 21.sp)
        }
        if (onClick != null) StrokeIcon(Icons.chevron, tint = ink, size = 16.dp)
    }
}

@Composable
fun OutlineButton(
    text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 56.dp, leading: String? = null,
    tint: Color = Fob.ink, edge: Color = Fob.outline,
) {
    Row(
        modifier.height(height).clip(RoundedCornerShape(height / 2))
            .border(1.dp, if (enabled) edge else Fob.line, RoundedCornerShape(height / 2))
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null) {
            StrokeIcon(leading, tint = tint, size = 18.dp)
            Spacer(Modifier.width(8.dp))
        }
        Text(text, color = if (enabled) tint else Fob.muted, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
fun TextButton(text: String, onClick: () -> Unit, color: Color = Fob.orangeText) {
    Box(
        Modifier.heightIn(min = EdgeButton.textHeight).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 10.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) { Text(text, color = color, fontSize = 15.sp, fontWeight = FontWeight.SemiBold) }
}

@Composable
fun Card(modifier: Modifier = Modifier, padding: PaddingValues = PaddingValues(vertical = 4.dp), content: @Composable ColumnScope.() -> Unit) {
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(Fob.card).padding(padding), content = content)
}

@Composable
fun Divider() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(Fob.line))
}

/** A label and value in a card: "Network fee  0.00003 XMR". */
@Composable
fun InfoRow(label: String, value: String, bold: Boolean = false, mono: Boolean = false) {
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = if (bold) Fob.ink else Fob.muted, fontSize = 15.sp, fontWeight = if (bold) FontWeight.SemiBold else FontWeight.Normal, modifier = Modifier.weight(1f))
        Text(
            value, fontSize = if (mono) 14.sp else 15.sp, fontWeight = if (bold) FontWeight.SemiBold else FontWeight.Normal,
            fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default, textAlign = TextAlign.End,
        )
    }
}

/** A setting or link row: title, subtitle, and a chevron or a switch. */
@Composable
fun SettingRow(
    title: String, subtitle: String? = null, onClick: (() -> Unit)? = null, checked: Boolean? = null, onChecked: ((Boolean) -> Unit)? = null,
    titleColor: Color = Fob.ink, subtitleColor: Color = Fob.muted,
) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp)
            .let { if (onClick != null) it.clickable(role = Role.Button, onClick = onClick) else it }
            .padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontSize = 16.sp, color = titleColor)
            if (subtitle != null) Text(subtitle, fontSize = 13.sp, color = subtitleColor)
        }
        when {
            checked != null -> Switch(
                checked = checked, onCheckedChange = onChecked,
                colors = SwitchDefaults.colors(checkedTrackColor = Fob.ink, checkedThumbColor = Fob.paper, uncheckedTrackColor = Fob.sunken, uncheckedBorderColor = Fob.outline, uncheckedThumbColor = Fob.outline),
            )
            onClick != null -> StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
        }
    }
}

/**
 * A search on a sunken pill: the magnifier, what's typed (or [placeholder]),
 * and a clear button once something is. [description] is TalkBack's name
 * for the field; the keyboard's Search key just closes the keyboard.
 */
@Composable
fun SearchField(value: String, onChange: (String) -> Unit, placeholder: String, description: String, modifier: Modifier = Modifier, height: Dp = 48.dp, onSearch: () -> Unit = {}) {
    val style = TextStyle(fontSize = 16.sp, color = Fob.ink)
    Row(
        modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2)).background(Fob.sunken).padding(start = 16.dp, end = if (value.isEmpty()) 16.dp else 0.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        StrokeIcon(Icons.search, tint = Fob.muted, size = 18.dp)
        BasicTextField(
            value = value, onValueChange = onChange, singleLine = true, textStyle = style, cursorBrush = SolidColor(Fob.ink),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = { onSearch() }),
            modifier = Modifier.weight(1f).semantics { contentDescription = description },
            decorationBox = { field ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (value.isEmpty()) Text(placeholder, style = style.copy(color = Fob.muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    field()
                }
            },
        )
        if (value.isNotEmpty()) {
            IconButton(onClick = { onChange("") }, modifier = Modifier.semantics { contentDescription = "Clear search" }) {
                StrokeIcon(Icons.close, tint = Fob.muted, size = 18.dp)
            }
        }
    }
}

/**
 * A radio: a ring, filled in the middle when [chosen]; paler while it
 * can't be [enabled]. Drawn only: the row around it is the control.
 */
@Composable
fun Radio(chosen: Boolean, enabled: Boolean = true) {
    val ring = when {
        !enabled -> Fob.track
        chosen -> Fob.ink
        else -> Fob.outline
    }
    Box(Modifier.size(20.dp).border(2.dp, ring, CircleShape), contentAlignment = Alignment.Center) {
        if (chosen) Box(Modifier.size(10.dp).clip(CircleShape).background(if (enabled) Fob.ink else Fob.outline))
    }
}

@Composable
fun SectionLabel(text: String) {
    Text(text, color = Fob.muted, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(start = 8.dp, bottom = 8.dp))
}

/**
 * A round sunken badge with a stroke icon, as in Activity rows; with
 * [receipt], a small receipt on its corner. Everything scales with [size].
 */
@Composable
fun IconBadge(path: String, tint: Color, size: Dp = 40.dp, receipt: Boolean = false) {
    Box {
        Box(Modifier.size(size).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) {
            StrokeIcon(path, tint = tint, size = size / 2)
        }
        if (receipt) {
            val badge = size / 2
            Box(Modifier.align(Alignment.BottomEnd).offset(size / 10, size / 10).size(badge).clip(CircleShape).background(Fob.card), contentAlignment = Alignment.Center) {
                StrokeIcon(Icons.receipt, tint = Fob.orangeText, size = badge * 0.7f, width = 2.2f)
            }
        }
    }
}

/**
 * A QR code from the Rust library's module grid, with a quiet zone of
 * [quiet] modules: dark on white in both themes. A payment code's pattern
 * (of the address in the code itself) is painted where Rust placed it, in
 * the light square in its middle; TalkBack reads only [description].
 */
@Composable
fun QrImage(code: QrCode, size: Dp, modifier: Modifier = Modifier, description: String = "Payment code", quiet: Int = 2) {
    Canvas(modifier.size(size).semantics { contentDescription = description }) {
        val n = code.size.toInt()
        // Each module a whole number of pixels, the code centred, so a dense code (a profile's) stays crisp
        // rather than blurring at its edges; a code with more modules than pixels falls back to fractions.
        val whole = kotlin.math.floor(this.size.width / (n + quiet * 2))
        val cell = if (whole >= 1f) whole else this.size.width / (n + quiet * 2)
        val start = (this.size.width - cell * n) / 2
        val origin = if (whole >= 1f) kotlin.math.floor(start) else quiet * cell
        val bleed = if (whole >= 1f) 0f else 0.5f
        drawRect(Fob.qrLight)
        for (y in 0 until n) for (x in 0 until n) {
            if (code.dark[y * n + x]) {
                drawRect(Fob.qrDark, topLeft = Offset(origin + x * cell, origin + y * cell), size = Size(cell + bleed, cell + bleed))
            }
        }
        code.pattern?.let { p ->
            val colour = Fob.patternColours[p.colour.toInt() % Fob.patternColours.size]
            p.dots.forEach { drawCircle(colour, radius = p.radius * cell, center = Offset(origin + it.x * cell, origin + it.y * cell)) }
        }
    }
}

/**
 * An address's pattern, as Rust derives it: a round white badge with dots
 * on a 5 × 5 grid (66% of the badge, centred, so the corners stay inside),
 * each dot 84% of its cell. Only Fob draws it; it has no words.
 */
@Composable
fun AddressPattern(address: String, size: Dp, modifier: Modifier = Modifier) {
    val pattern = remember(address) { addressPattern(address) }
    val (badge, edge) = Fob.patternBadge to Fob.line
    Canvas(modifier.size(size).semantics { contentDescription = "Address pattern" }) {
        val side = this.size.width
        drawCircle(badge)
        drawCircle(edge, style = Stroke(1.dp.toPx()), radius = side / 2 - 0.5.dp.toPx())
        patternDots(pattern, center, side * 0.66f)
    }
}

/** A pattern's dots on a 5 × 5 grid [grid] wide around [centre], each dot 84% of its cell, in the pattern's colour. */
private fun DrawScope.patternDots(pattern: uniffi.fob.AddressPattern, centre: Offset, grid: Float) {
    val colour = Fob.patternColours[pattern.colour.toInt() % Fob.patternColours.size]
    val cell = grid / 5
    val start = Offset(centre.x - grid / 2, centre.y - grid / 2)
    for (row in 0 until 5) for (column in 0 until 5) {
        if (pattern.dots[row * 5 + column]) {
            drawCircle(colour, radius = cell * 0.84f / 2, center = Offset(start.x + (column + 0.5f) * cell, start.y + (row + 0.5f) * cell))
        }
    }
}

/** The Fob seal's wax edge (the Calm pour); docs/brand/seal.py draws the icons from the same formula. */
private fun waxRadius(t: Float) = 39.5f + 1.7f * sin(5f * t + 0.4f) + 0.6f * sin(9f * t + 1f)

/** The Fob mark: the Monokulo iris pressed into a disc of orange wax. */
@Composable
fun FobMark(size: Dp) {
    Canvas(Modifier.size(size).semantics { contentDescription = "Fob" }) {
        val steps = 120
        val radii = FloatArray(steps) { waxRadius(it * 2f * PI.toFloat() / steps) }
        val u = this.size.width / 2f / radii.max()
        val c = center
        val wax = Path()
        radii.forEachIndexed { i, r ->
            val t = i * 2f * PI.toFloat() / steps
            val x = c.x + cos(t) * r * u
            val y = c.y + sin(t) * r * u
            if (i == 0) wax.moveTo(x, y) else wax.lineTo(x, y)
        }
        wax.close()
        drawPath(wax, Fob.orange)
        val deep = Color(0xFFC24E00)
        // The iris stays dark on the wax in both themes, like the launcher icon.
        val iris = Color(0xFF1A1917)
        drawCircle(deep, radius = 29f * u, center = c, style = Stroke(3.2f * u))
        val k = 2.05f * u
        fun p(x: Float, y: Float) = Offset(c.x + (x - 30f) * k, c.y + (y - 28f) * k)
        fun poly(color: Color, vararg pts: Pair<Float, Float>) {
            val path = Path()
            pts.forEachIndexed { i, (x, y) -> val o = p(x, y); if (i == 0) path.moveTo(o.x, o.y) else path.lineTo(o.x, o.y) }
            path.close()
            drawPath(path, color)
        }
        poly(deep, 30f to 23f, 34.33f to 25.5f, 35.5f to 18.47f)
        poly(deep, 34.33f to 25.5f, 34.33f to 30.5f, 41f to 28f)
        poly(deep, 34.33f to 30.5f, 30f to 33f, 35.5f to 37.53f)
        poly(deep, 30f to 33f, 25.67f to 30.5f, 24.5f to 37.53f)
        poly(deep, 25.67f to 30.5f, 25.67f to 25.5f, 19f to 28f)
        poly(deep, 25.67f to 25.5f, 30f to 23f, 24.5f to 18.47f)
        poly(iris, 30f to 23f, 34.33f to 25.5f, 34.33f to 30.5f, 30f to 33f, 25.67f to 30.5f, 25.67f to 25.5f)
    }
}

/** A bar on the line colour, filled as far as [progress] (0 to 1); TalkBack reads it as a progress bar. */
@Composable
fun ProgressBar(progress: Float, height: Dp = 6.dp, fill: Color = Fob.orange) {
    val done = progress.coerceIn(0f, 1f)
    Box(
        Modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2)).background(Fob.line)
            .semantics { progressBarRangeInfo = ProgressBarRangeInfo(done, 0f..1f) },
    ) {
        Box(Modifier.fillMaxWidth(done).height(height).clip(RoundedCornerShape(height / 2)).background(fill))
    }
}

/**
 * Swipe to send: drag the orange knob all the way across. Let go early and
 * it springs back, so a stray tap never pays. TalkBack gets a plain action.
 */
@Composable
fun SwipeToSend(label: String, enabled: Boolean, onSwiped: () -> Unit) {
    val scope = rememberCoroutineScope()
    val offset = remember { Animatable(0f) }
    BoxWithConstraints(
        Modifier.fillMaxWidth().height(64.dp).clip(RoundedCornerShape(32.dp)).background(Fob.sunken)
            .semantics(mergeDescendants = true) { contentDescription = label }
            .clickable(enabled = enabled, onClickLabel = label, role = Role.Button) {
                // Only accessibility services click; a finger has to swipe.
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        val knob = 56.dp
        val max = with(LocalDensity.current) { (maxWidth - knob - 8.dp).toPx() }
        Text(
            label, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, color = if (enabled) Fob.ink else Fob.muted,
            textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(start = 48.dp),
        )
        Box(
            Modifier.padding(4.dp).offset { IntOffset(offset.value.roundToInt(), 0) }.size(knob).clip(CircleShape)
                .background(if (enabled) Fob.orange else Fob.outline)
                .draggable(
                    enabled = enabled,
                    orientation = Orientation.Horizontal,
                    state = rememberDraggableState { delta -> scope.launch { offset.snapTo((offset.value + delta).coerceIn(0f, max)) } },
                    onDragStopped = {
                        if (offset.value >= max * 0.92f) {
                            offset.snapTo(max)
                            onSwiped()
                        } else {
                            offset.animateTo(0f, spring())
                        }
                    },
                ),
            contentAlignment = Alignment.Center,
        ) {
            StrokeIcon(Icons.arrow, tint = Fob.onOrange, size = 24.dp, width = 2.2f)
        }
    }
}

@Composable
fun ErrorText(text: String?) {
    if (text != null) Text(text, color = Fob.red, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 4.dp))
}

/** The message to show for anything the Rust library throws. */
fun Throwable.reason(): String = when (this) {
    is uniffi.fob.FobException.Invalid -> reason
    is uniffi.fob.FobException.Node -> "Couldn't reach the node: $reason"
    is uniffi.fob.FobException.NotEnoughFunds -> reason.replaceFirstChar { it.uppercase() }
    is uniffi.fob.FobException.WrongKey -> reason
    is uniffi.fob.FobException.ViewOnly -> reason
    is uniffi.fob.FobException.Other -> reason
    else -> message ?: toString()
}.replaceFirstChar { it.uppercase() }

@Composable
fun Spacing(h: Dp) = Spacer(Modifier.height(h))

@Composable
fun PagePadding(content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp), content = content)
}

/** Groups of four characters, as addresses are shown: "86Ux 3m9P … 7kQa". */
fun short(address: String, groups: Int = 3): String {
    val head = address.take(4 * groups).chunked(4).joinToString(" ")
    val tail = address.takeLast(4 * groups).chunked(4).joinToString(" ")
    return "$head … $tail"
}

val small = 13.sp

/** Keeps secrets on screen out of screenshots, recordings and the app switcher. */
@Composable
fun NoScreenshots() {
    val activity = androidx.activity.compose.LocalActivity.current
    androidx.compose.runtime.DisposableEffect(activity) {
        val window = activity?.window
        window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
        onDispose { window?.clearFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE) }
    }
}

/**
 * The system bars' icons dark ([darkIcons]) or light while this is shown,
 * whatever the theme: light over the camera, dark on a white code screen.
 * The navigation bar's too unless [statusOnly]. Set again after a turn or
 * a theme change, which set them afresh; put back as they were after.
 */
@Composable
fun BarIcons(darkIcons: Boolean, statusOnly: Boolean = false) {
    val window = androidx.activity.compose.LocalActivity.current?.window
    val view = androidx.compose.ui.platform.LocalView.current
    val configuration = androidx.compose.ui.platform.LocalConfiguration.current
    androidx.compose.runtime.DisposableEffect(window, configuration) {
        val bars = window?.let { androidx.core.view.WindowCompat.getInsetsController(it, view) }
        val status = bars?.isAppearanceLightStatusBars
        val navigation = bars?.isAppearanceLightNavigationBars
        bars?.isAppearanceLightStatusBars = darkIcons
        if (!statusOnly) bars?.isAppearanceLightNavigationBars = darkIcons
        onDispose {
            if (status != null) bars.isAppearanceLightStatusBars = status
            if (navigation != null && !statusOnly) bars.isAppearanceLightNavigationBars = navigation
        }
    }
}

/**
 * A sheet over the screen, on a scrim: tapping the scrim or Back calls
 * [onDismiss]. Its paper reaches the bottom edge; what's on it stays above
 * the gesture bar and clear of the sides, and a tall one stops short of
 * the status bar. [title] names it for TalkBack.
 */
@Composable
fun BottomSheet(title: String, onDismiss: () -> Unit, background: Color = Fob.paper, content: @Composable ColumnScope.() -> Unit) {
    androidx.activity.compose.BackHandler(onBack = onDismiss)
    Box(Modifier.fillMaxSize()) {
        Box(Modifier.fillMaxSize().background(Fob.scrim).clickable(onClickLabel = "Close", onClick = onDismiss, indication = null, interactionSource = null))
        Column(
            Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top)).padding(top = 24.dp)
                .fillMaxWidth().clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(background)
                // Taps on the sheet stay on it.
                .clickable(onClick = {}, indication = null, interactionSource = null)
                .semantics { paneTitle = title }
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
                .padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 20.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            content = content,
        )
    }
}

/** A sheet's handle, as on Send's. */
@Composable
fun SheetHandle() {
    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Box(Modifier.width(36.dp).height(4.dp).clip(RoundedCornerShape(2.dp)).background(Fob.outline)) }
}

/**
 * A label at the start and a value at the end of a row (the sync card's
 * rows, Node's checks). At a big font the label wraps first, down to its
 * longest word, so the value keeps its whole line ("57 blocks/s") where
 * it can; then the value wraps between words; where even its longest word
 * ("2,240,749", "blocks/s") wouldn't fit beside the label, it goes under
 * it at the end, the label then taking the whole line. Nothing breaks
 * inside a word.
 * [centred] puts both on the row's middle, else on its top.
 */
@Composable
fun LabelAndValue(modifier: Modifier = Modifier, gap: Dp = 14.dp, centred: Boolean = false, label: @Composable () -> Unit, value: @Composable () -> Unit) {
    Layout(content = { Box { label() }; Box { value() } }, modifier = modifier.fillMaxWidth()) { (l, v), c ->
        val space = gap.roundToPx()
        val width = c.maxWidth
        val labelMin = minOf(l.minIntrinsicWidth(Constraints.Infinity), width)
        val labelMax = maxOf(labelMin, l.maxIntrinsicWidth(Constraints.Infinity))
        // The value's whole line if the label can wrap to make room for it, else as much as the label's longest word leaves.
        val labelWidth = (width - space - v.maxIntrinsicWidth(Constraints.Infinity)).coerceIn(labelMin, labelMax)
        // Where even that would break the value's longest word, it goes under the label at the end, and the label
        // has the whole line rather than breaking at every word.
        val under = width - space - labelWidth < v.minIntrinsicWidth(Constraints.Infinity)
        val lp = l.measure(Constraints(maxWidth = if (under) width else labelWidth))
        val room = width - space - lp.width
        val vp = v.measure(Constraints(maxWidth = if (under) width else room.coerceAtLeast(0)))
        val height = if (under) maxOf(lp.height + vp.height, c.minHeight) else maxOf(lp.height, vp.height, c.minHeight)
        layout(width, height) {
            if (under) {
                lp.placeRelative(0, 0)
                vp.placeRelative(width - vp.width, lp.height)
            } else {
                lp.placeRelative(0, if (centred) (height - lp.height) / 2 else 0)
                vp.placeRelative(width - vp.width, if (centred) (height - vp.height) / 2 else 0)
            }
        }
    }
}
