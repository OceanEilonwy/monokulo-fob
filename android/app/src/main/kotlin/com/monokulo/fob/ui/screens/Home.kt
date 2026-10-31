package com.monokulo.fob.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.FobMark
import com.monokulo.fob.ui.IconBadge
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.Manrope
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.ProgressBar
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.SyncChip
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.display
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.fob.ActivityRow
import uniffi.fob.Network
import uniffi.fob.NotReady
import uniffi.fob.NotReadyPiece
import uniffi.fob.SyncStatus
import java.text.DateFormat
import java.util.Date

fun syncLine(status: SyncStatus): String {
    val upTo = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(status.upTo.toLong() * 1000))
    val left = status.secondsLeft?.toLong()?.let { s -> if (s < 90) "about $s s left" else "about ${(s + 30) / 60} min left" }
    return listOfNotNull(if (status.upTo > 0u) "Up to $upTo" else null, left).joinToString(" · ")
}

@Composable
fun HomeScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val balance by model.balance.collectAsState()
    val activity by model.activity.collectAsState()
    val status by model.status.collectAsState()
    val notReady by model.notReady.collectAsState()
    val prices by model.prices.collectAsState()
    val network by model.network.collectAsState()
    val synced = status?.synced == true
    val error = status?.error
    val canSpend = wallet?.canSpend() != false
    val scope = rememberCoroutineScope()
    var refreshing by remember { mutableStateOf(false) }
    // Ready times count down between blocks: Rust writes them again each half minute.
    LaunchedEffect(notReady != null) {
        while (notReady != null) {
            delay(30_000)
            model.refresh()
        }
    }

    Page(refreshing = refreshing, onRefresh = {
        refreshing = true
        scope.launch {
            model.syncNow()
            refreshing = false
        }
    }) {
        Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp).heightIn(min = 64.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            FobMark(26.dp)
            Text("Fob", style = display(22))
            if (network == Network.STAGENET) Text("Stagenet", color = Fob.orangeText, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.clip(RoundedCornerShape(10.dp)).background(Fob.highlight).padding(horizontal = 8.dp, vertical = 2.dp))
            Spacer(Modifier.weight(1f))
            val chip = when {
                status == null -> "Starting"
                error != null && !synced -> "Can't reach node"
                synced -> "Synced"
                else -> "Catching up · ${((status?.progress ?: 0f) * 100).toInt()}%"
            }
            SyncChip(chip, synced, status?.progress ?: 0f, error != null && !synced) { nav.go(Screen.Node) }
            IconButton(onClick = { nav.go(Screen.Settings) }, modifier = Modifier.semantics { contentDescription = "Settings" }) { StrokeIcon(Icons.gear, width = 1.8f) }
        }

        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 28.dp, bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(if (synced) "Balance" else "Balance so far", color = Fob.muted, fontSize = 14.sp)
            // Tapping swaps which comes first, kept as Settings' "Show amounts in".
            val b = balance
            val fiat = b?.fiatText
            Column(
                Modifier.clip(RoundedCornerShape(12.dp))
                    .clickable(enabled = fiat != null, role = Role.Button, onClickLabel = "Swap XMR and ${prices?.currency ?: "money"}") {
                        scope.launch { runCatching { model.setFiatFirst(b?.fiatFirst != true) } }
                    },
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                if (fiat != null && b.fiatFirst) {
                    Text(fiat, style = display(56))
                    Text("${b.totalText} XMR", color = Fob.muted, fontSize = 17.sp, fontWeight = FontWeight.Medium)
                } else {
                    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(b?.totalText ?: "—", style = display(56))
                        Text("XMR", fontFamily = Manrope, fontWeight = FontWeight.Bold, fontSize = 22.sp, color = Fob.muted, modifier = Modifier.padding(bottom = 10.dp))
                    }
                    fiat?.let { Text(it, color = Fob.muted, fontSize = 17.sp, fontWeight = FontWeight.Medium) }
                }
            }
            balance?.readyText?.let { Text(it, color = Fob.muted, fontSize = 15.sp) }
            notReady?.let { NotReadyCard(it, onSplit = if (canSpend) ({ nav.go(Screen.Split) }) else null) }
            val s = status
            if (s != null && !s.synced && s.tip > 0u) {
                Column(Modifier.padding(top = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    ProgressBar(s.progress)
                    Text(syncLine(s), color = Fob.muted, fontSize = 14.sp)
                }
            }
            if (error != null && !synced) Text(error, color = Fob.red, fontSize = 13.sp, modifier = Modifier.padding(top = 8.dp))
        }

        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (canSpend) PrimaryButton("Scan to pay", { nav.go(Screen.ScanPay) }, height = 60.dp, leading = Icons.scan)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlineButton("Receive", { nav.go(Screen.Receive) }, Modifier.weight(1f), height = 52.dp, leading = Icons.down)
                if (canSpend) OutlineButton("Send", { nav.go(Screen.Send()) }, Modifier.weight(1f), height = 52.dp, leading = Icons.up)
            }
            if (!canSpend) Text("This wallet only watches: it can see payments but not send them.", color = Fob.muted, fontSize = 13.sp)
        }

        Card(Modifier.padding(start = 16.dp, end = 16.dp, top = 24.dp, bottom = 24.dp), padding = PaddingValues(vertical = 8.dp)) {
            Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 16.dp, top = 10.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Activity", fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                if (!synced && activity.isNotEmpty()) Text("Still finding older ones", color = Fob.muted, fontSize = 13.sp)
                if (canSpend) TextButton("Split up", { nav.go(Screen.Split) })
            }
            if (activity.isEmpty()) {
                Text(
                    if (synced) "Nothing yet. Tap Receive to get your address." else "Payments appear here as Fob finds them.",
                    color = Fob.muted, fontSize = 15.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 16.dp),
                )
            }
            activity.forEach { day ->
                DayLabel(day.label)
                day.rows.forEach { row ->
                    PaymentRow(row) { nav.go(if (row.activity.receipt != null) Screen.Receipt(row.activity) else Screen.Payment(row.activity)) }
                }
            }
        }
    }
}

/**
 * Money not ready to spend yet: folded, one line and a bar; opened, a row
 * per piece with its confirmations in a ring. Rust wrote every word.
 */
@Composable
private fun NotReadyCard(n: NotReady, onSplit: (() -> Unit)?) {
    var open by rememberSaveable { mutableStateOf(false) }
    val turn by animateFloatAsState(if (open) -90f else 90f, label = "chevron")
    val state = if (open) "Expanded" else "Collapsed"
    Card(Modifier.padding(top = 16.dp), padding = PaddingValues(0.dp)) {
        Column(
            Modifier.fillMaxWidth()
                .clickable(role = Role.Button, onClickLabel = if (open) "Hide coins" else "Show coins") { open = !open }
                .semantics {
                    stateDescription = state
                    if (open) collapse { open = false; true } else expand { open = true; true }
                }
                .padding(start = 16.dp, end = 12.dp, top = 14.dp, bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(n.title, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                    Text(n.line, color = Fob.muted, fontSize = 13.sp)
                }
                Text(n.percent, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                Box(Modifier.rotate(turn)) { StrokeIcon(Icons.chevron, tint = Fob.muted, size = 18.dp) }
            }
            ProgressBar(n.progress, fill = Fob.green)
        }
        AnimatedVisibility(open, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
            Column {
                Divider()
                Column(Modifier.padding(vertical = 6.dp)) { n.pieces.forEach { PieceRow(it) } }
                if (onSplit != null) {
                    Text(
                        "Split up your balance", color = Fob.orangeText, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.padding(start = 8.dp, bottom = 8.dp).clip(RoundedCornerShape(10.dp))
                            .clickable(role = Role.Button, onClick = onSplit).padding(horizontal = 8.dp, vertical = 10.dp),
                    )
                }
            }
        }
    }
}

/** One output not ready yet: its confirmations in a ring, where it came from, when it will be ready. */
@Composable
private fun PieceRow(p: NotReadyPiece) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).semantics(mergeDescendants = true) {}.padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        ConfirmationRing(p.confirmations, p.needed, p.progress)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text("${p.amountText} XMR" + (p.fiatText?.let { " · $it" } ?: ""), fontSize = 15.sp, fontWeight = FontWeight.Medium)
            Text(p.source, color = Fob.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(p.ready, fontSize = 15.sp, fontWeight = FontWeight.Medium)
            Text(p.eta, color = Fob.muted, fontSize = 12.sp)
        }
    }
}

/** A 34dp ring: green as far as the confirmations go, their number inside. */
@Composable
private fun ConfirmationRing(confirmations: ULong, needed: ULong, progress: Float) {
    val (track, arc) = Fob.track to Fob.green
    Box(
        Modifier.size(34.dp).semantics {
            contentDescription = "$confirmations of $needed confirmations"
            progressBarRangeInfo = ProgressBarRangeInfo(confirmations.toFloat(), 0f..needed.toFloat())
        },
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.size(34.dp)) {
            val width = 3.dp.toPx()
            val stroke = Stroke(width, cap = StrokeCap.Round)
            val inset = Offset(width / 2, width / 2)
            val ring = Size(size.width - width, size.height - width)
            drawArc(track, 0f, 360f, false, topLeft = inset, size = ring, style = stroke)
            if (progress > 0f) drawArc(arc, -90f, 360f * progress.coerceIn(0f, 1f), false, topLeft = inset, size = ring, style = stroke)
        }
        Text("$confirmations", fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
    }
}

/** "TODAY", "YESTERDAY", "27 OCT": a day's header in Activity. */
@Composable
private fun DayLabel(text: String) {
    Text(
        text, color = Fob.muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.8.sp,
        modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 14.dp, bottom = 4.dp).semantics { heading() },
    )
}

/** One payment: what Rust wrote for it, drawn. */
@Composable
fun PaymentRow(row: ActivityRow, onClick: () -> Unit) {
    val outgoing = row.activity.outgoing
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        IconBadge(if (outgoing) Icons.up else Icons.down, if (outgoing) Fob.ink else Fob.green, size = 28.dp, receipt = row.activity.receipt != null)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(row.title, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(row.line, color = Fob.muted, fontSize = 12.sp)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(row.amountText, fontSize = 15.sp, fontWeight = FontWeight.Medium, color = if (outgoing) Fob.ink else Fob.green)
            row.fiatText?.let { Text(it, color = Fob.muted, fontSize = 12.sp) }
        }
    }
}
