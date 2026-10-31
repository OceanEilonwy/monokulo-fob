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
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import uniffi.fob.QrCode
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

/** The stroke icons from the designs, as SVG path data on a 24 × 24 grid. */
object Icons {
    const val back = "M20 12H5M11 6l-6 6 6 6"
    const val close = "M6 6l12 12M18 6L6 18"
    /** Monokular's cog: a ring and eight spokes. */
    const val gear = "M12 8.8a3.2 3.2 0 1 0 0.01 0Z M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"
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
            Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = bottom)
        }
    }
}

@Composable
fun TopBar(title: String?, onBack: (() -> Unit)?, closeIcon: Boolean = false, tint: Color = Fob.ink, actions: @Composable RowScope.() -> Unit = {}) {
    Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        if (onBack != null) {
            IconButton(onClick = onBack, modifier = Modifier.semantics { contentDescription = if (closeIcon) "Close" else "Back" }) {
                StrokeIcon(if (closeIcon) Icons.close else Icons.back, tint = tint, size = 20.dp)
            }
        } else {
            Spacer(Modifier.width(16.dp))
        }
        Text(title ?: "", color = tint, fontSize = 20.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
        actions()
    }
}

@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 56.dp, leading: String? = null) {
    Row(
        modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2))
            .background(if (enabled) Fob.orange else Fob.sunken)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null) {
            StrokeIcon(leading, tint = Fob.onOrange)
            Spacer(Modifier.width(10.dp))
        }
        Text(text, color = if (enabled) Fob.onOrange else Fob.muted, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
fun OutlineButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 56.dp, leading: String? = null, tint: Color = Fob.ink) {
    Row(
        modifier.height(height).clip(RoundedCornerShape(height / 2))
            .border(1.dp, if (enabled) Fob.outline else Fob.line, RoundedCornerShape(height / 2))
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null) {
            StrokeIcon(leading, tint = tint, size = 18.dp)
            Spacer(Modifier.width(8.dp))
        }
        Text(text, color = if (enabled) tint else Fob.muted, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
fun TextButton(text: String, onClick: () -> Unit, color: Color = Fob.orangeText) {
    Text(
        text, color = color, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
        modifier = Modifier.clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 10.dp, vertical = 12.dp),
    )
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
fun SettingRow(title: String, subtitle: String? = null, onClick: (() -> Unit)? = null, checked: Boolean? = null, onChecked: ((Boolean) -> Unit)? = null, titleColor: Color = Fob.ink) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp)
            .let { if (onClick != null) it.clickable(role = Role.Button, onClick = onClick) else it }
            .padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontSize = 16.sp, color = titleColor)
            if (subtitle != null) Text(subtitle, fontSize = 13.sp, color = Fob.muted)
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

/** A QR code from the Rust library's module grid, with a quiet zone: dark on white in both themes. */
@Composable
fun QrImage(code: QrCode, size: Dp, modifier: Modifier = Modifier, description: String = "Payment code") {
    Canvas(modifier.size(size).semantics { contentDescription = description }) {
        val n = code.size.toInt()
        val quiet = 2
        val cell = this.size.width / (n + quiet * 2)
        drawRect(Fob.qrLight)
        for (y in 0 until n) for (x in 0 until n) {
            if (code.dark[y * n + x]) {
                drawRect(Fob.qrDark, topLeft = Offset((x + quiet) * cell, (y + quiet) * cell), size = Size(cell + 0.5f, cell + 0.5f))
            }
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

/** The sync chip in the home bar: a green dot when up to date, a ring while catching up. */
@Composable
fun SyncChip(text: String, synced: Boolean, progress: Float, error: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.height(36.dp).clip(RoundedCornerShape(18.dp)).background(Fob.sunken).clickable(role = Role.Button, onClick = onClick).padding(start = 10.dp, end = 14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when {
            error -> Box(Modifier.size(8.dp).clip(CircleShape).background(Fob.red))
            synced -> Box(Modifier.size(8.dp).clip(CircleShape).background(Fob.greenDot))
            else -> {
                val (track, arc) = Fob.track to Fob.orangeText
                Canvas(Modifier.size(18.dp)) {
                    val stroke = Stroke(2.5.dp.toPx(), cap = StrokeCap.Round)
                    drawArc(track, 0f, 360f, false, style = stroke)
                    drawArc(arc, -90f, 360f * progress.coerceIn(0.02f, 1f), false, style = stroke)
                }
            }
        }
        Text(text, fontSize = 14.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
fun ProgressBar(progress: Float, height: Dp = 6.dp) {
    Box(Modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2)).background(Fob.line)) {
        Box(Modifier.fillMaxWidth(progress.coerceIn(0f, 1f)).height(height).clip(RoundedCornerShape(height / 2)).background(Fob.orange))
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
fun Mono(text: String, size: Int = 15) {
    Text(text, fontFamily = FontFamily.Monospace, fontSize = size.sp, lineHeight = (size + 7).sp, letterSpacing = 0.5.sp, textAlign = TextAlign.Center)
}

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

fun full(address: String): String = address.chunked(4).joinToString(" ")

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
