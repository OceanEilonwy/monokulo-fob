package com.monokulo.fob.ui

import android.os.SystemClock
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import uniffi.fob.ChipLook
import uniffi.fob.SyncDetails
import uniffi.fob.SyncStatus

/**
 * Home's sync chip (board SyncChips) with the card it opens (board
 * SyncHome): a ring filled as far as the progress with Rust's time left
 * while catching up, a green dot once synced, a red one when the node
 * doesn't answer. It's a button that opens and closes the card; tapping
 * outside the card or Back closes it too. [details] asks Rust for the
 * card's words, again whenever [status] changes while it's open.
 */
@Composable
fun SyncChip(status: SyncStatus?, open: Boolean, onOpen: (Boolean) -> Unit, details: () -> SyncDetails?, onNode: () -> Unit) {
    val look = status?.chipLook ?: ChipLook.RING
    val text = status?.chipText ?: "Starting"
    val label = status?.chipLabel ?: "Starting"
    // A tap on the chip while the card is open first reaches the card as a
    // tap outside it: that closes it, and the chip mustn't open it again.
    var closedAt by remember { mutableLongStateOf(0L) }
    val toggle: () -> Unit = {
        if (open) onOpen(false) else if (SystemClock.uptimeMillis() - closedAt > 300) onOpen(true)
    }
    // Where the chip ends, so the card under it fits above the gesture bar.
    var chipBottom by remember { mutableIntStateOf(0) }
    Box(Modifier.onGloballyPositioned { chipBottom = (it.positionInWindow().y + it.size.height).toInt() }) {
        Box(
            Modifier.heightIn(min = 48.dp).widthIn(min = 48.dp).clip(RoundedCornerShape(24.dp)).clickable(role = Role.Button, onClick = toggle)
                .clearAndSetSemantics {
                    contentDescription = label
                    role = Role.Button
                    onClick(label = if (open) "Hide sync details" else "Show sync details") { toggle(); true }
                    if (open) collapse { onOpen(false); true } else expand { onOpen(true); true }
                },
            contentAlignment = Alignment.Center,
        ) {
            // Where the top bar is too narrow for the words (a small phone, big
            // font), only the ring or dot shows; TalkBack still reads the label.
            Layout(
                content = {
                    when (look) {
                        ChipLook.OFFLINE -> Box(Modifier.size(8.dp).clip(CircleShape).background(Fob.red))
                        ChipLook.SYNCED -> Box(Modifier.size(8.dp).clip(CircleShape).background(Fob.greenDot))
                        ChipLook.RING -> {
                            val (track, arc) = Fob.track to Fob.orangeText
                            val progress = status?.progress ?: 0f
                            Canvas(Modifier.size(16.dp)) {
                                val stroke = Stroke(2.4.dp.toPx(), cap = StrokeCap.Round)
                                drawArc(track, 0f, 360f, false, style = stroke)
                                drawArc(arc, -90f, 360f * progress.coerceIn(0.02f, 1f), false, style = stroke)
                            }
                        }
                    }
                    Text(text, fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
                },
                modifier = Modifier.height(36.dp).clip(RoundedCornerShape(18.dp)).background(Fob.sunken).padding(start = 12.dp, end = 14.dp),
            ) { (mark, words), c ->
                val m = mark.measure(Constraints())
                val w = words.measure(Constraints())
                val gap = 8.dp.roundToPx()
                val fits = m.width + gap + w.width <= c.maxWidth
                val h = c.constrainHeight(maxOf(m.height, w.height))
                layout(if (fits) m.width + gap + w.width else m.width, h) {
                    m.placeRelative(0, (h - m.height) / 2)
                    if (fits) w.placeRelative(m.width + gap, (h - w.height) / 2)
                }
            }
        }
        if (open) {
            val d = remember(status) { details() }
            // Measured here, in Home's window: inside the popup it's the popup's own.
            val density = LocalDensity.current
            val windowHeight = LocalView.current.rootView.height
            val navBar = WindowInsets.navigationBars.getBottom(density)
            val room = with(density) { (windowHeight - chipBottom - navBar).coerceAtLeast(0).toDp() }
            if (d != null) {
                Popup(
                    popupPositionProvider = BelowAtEnd,
                    onDismissRequest = {
                        closedAt = SystemClock.uptimeMillis()
                        onOpen(false)
                    },
                    properties = PopupProperties(focusable = true),
                ) {
                    // Room around the card for its shadow, and off the screen's edge;
                    // at a big font it scrolls within the room under the chip rather than
                    // covering the chip or running under the gesture bar.
                    Box(Modifier.heightIn(max = room).padding(start = 12.dp, end = 12.dp, bottom = 16.dp)) {
                        SyncCard(d, Modifier.widthIn(max = 300.dp).fillMaxWidth()) {
                            onOpen(false)
                            onNode()
                        }
                    }
                }
            }
        }
    }
}

/** Under the chip, against the screen's end edge. */
private object BelowAtEnd : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val x = if (layoutDirection == LayoutDirection.Ltr) windowSize.width - popupContentSize.width else 0
        return IntOffset(x, anchorBounds.bottom)
    }
}

/** The details card: Rust's title, percent, rows and line, a bar while catching up, and a way to Node. */
@Composable
private fun SyncCard(d: SyncDetails, modifier: Modifier, onNode: () -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Column(
        modifier.shadow(8.dp, shape).clip(shape).background(Fob.card).semantics { paneTitle = d.title }
            .verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) { heading() }, verticalAlignment = Alignment.CenterVertically) {
            Text(d.title, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            d.percent?.let { Text(it, fontSize = 14.sp, color = Fob.muted) }
        }
        d.progress?.let { ProgressBar(it) }
        if (d.rows.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                d.rows.forEach { row ->
                    // TalkBack hears Rust's words, not "28 slash s".
                    LabelAndValue(Modifier.clearAndSetSemantics { contentDescription = row.spoken }, label = { Text(row.label, fontSize = 14.sp, color = Fob.muted) }) {
                        Text(row.value, fontSize = 14.sp, textAlign = TextAlign.End)
                    }
                }
            }
        }
        d.note?.let { Text(it, fontSize = 12.5.sp, color = Fob.muted) }
        d.error?.let { Text(it, fontSize = 13.sp, color = Fob.red) }
        Box(
            Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onNode).padding(horizontal = 4.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            Text("Node settings", color = Fob.orangeText, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
        }
    }
}
