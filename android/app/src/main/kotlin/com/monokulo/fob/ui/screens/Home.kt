package com.monokulo.fob.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
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
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
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
import com.monokulo.fob.ui.display
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.fob.ActivityRow
import uniffi.fob.Network
import uniffi.fob.NotReady
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

        // The balance on one line: what "Show amounts in" puts first large on
        // the left, the other at the line's end. Tapping swaps them.
        val b = balance
        val fiat = b?.fiatText
        val fiatFirst = fiat != null && b.fiatFirst
        if (!synced) Text("Balance so far", color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(start = 20.dp, top = 12.dp))
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 12.dp).padding(top = if (synced) 16.dp else 0.dp, bottom = 12.dp).clip(RoundedCornerShape(12.dp))
                .clickable(enabled = fiat != null, role = Role.Button, onClickLabel = "Swap XMR and ${prices?.currency ?: "money"}") {
                    scope.launch { runCatching { model.setFiatFirst(b?.fiatFirst != true) } }
                }
                .semantics(mergeDescendants = true) {}
                .padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(Modifier.weight(1f, fill = false).alignByBaseline(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                BasicText(
                    if (fiatFirst) fiat!! else b?.totalText ?: "—",
                    style = display(48).copy(color = Fob.ink, lineHeight = 50.sp),
                    maxLines = 1, autoSize = TextAutoSize.StepBased(minFontSize = 24.sp, maxFontSize = 48.sp),
                    modifier = Modifier.weight(1f, fill = false).alignByBaseline(),
                )
                if (!fiatFirst) Text("XMR", fontFamily = Manrope, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = Fob.muted, modifier = Modifier.alignByBaseline())
            }
            val end = if (fiatFirst) "${b.totalText} XMR" else fiat
            end?.let { Text(it, color = Fob.muted, fontSize = 17.sp, maxLines = 1, modifier = Modifier.alignByBaseline()) }
        }

        Column(Modifier.padding(horizontal = 16.dp)) {
            notReady?.let { ReadyCard(it) }
            val s = status
            if (s != null && !s.synced && s.tip > 0u) {
                Column(Modifier.padding(start = 4.dp, end = 4.dp, top = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    ProgressBar(s.progress)
                    Text(syncLine(s), color = Fob.muted, fontSize = 14.sp)
                }
            }
            if (error != null && !synced) Text(error, color = Fob.red, fontSize = 13.sp, modifier = Modifier.padding(start = 4.dp, top = 8.dp))
        }

        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlineButton("Receive", { nav.go(Screen.Receive) }, Modifier.weight(1f), height = 52.dp, leading = Icons.down)
                if (canSpend) PrimaryButton("Send", { nav.go(Screen.Send()) }, Modifier.weight(1f), height = 52.dp, leading = Icons.up)
            }
            if (!canSpend) Text("This wallet only watches: it can see payments but not send them.", color = Fob.muted, fontSize = 13.sp)
        }

        Card(Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 24.dp), padding = PaddingValues(vertical = 8.dp)) {
            Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = 20.dp, end = 8.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Activity", fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                if (!synced && activity.isNotEmpty()) Text("Still finding older ones", color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(end = 12.dp))
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
 * What's ready beside what isn't yet, while some isn't: folded, one line
 * and the bar; opened, the sums and when all of it should be ready. Rust
 * wrote every word.
 */
@Composable
private fun ReadyCard(n: NotReady) {
    var open by rememberSaveable { mutableStateOf(false) }
    val turn by animateFloatAsState(if (open) -90f else 90f, label = "chevron")
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Fob.card)) {
        Column(
            Modifier.fillMaxWidth()
                .clickable(role = Role.Button, onClickLabel = if (open) "Hide the sums" else "Show the sums") { open = !open }
                .semantics {
                    contentDescription = n.description
                    stateDescription = if (open) "Expanded" else "Collapsed"
                    if (open) collapse { open = false; true } else expand { open = true; true }
                }
                .padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(Modifier.clearAndSetSemantics {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(n.readyText) }
                        append(" · ${n.notYetText}")
                    },
                    fontSize = 14.sp, modifier = Modifier.weight(1f),
                )
                Text(n.allReady, color = Fob.muted, fontSize = 14.sp)
                Box(Modifier.rotate(turn)) { StrokeIcon(Icons.chevron, tint = Fob.muted, size = 20.dp) }
            }
            Box(Modifier.clearAndSetSemantics {}) { CoinBar(n.ready, n.notReady, n.progress) }
        }
        AnimatedVisibility(open, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
            Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    val dots = listOf(Fob.ink, Fob.green)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        n.sums.forEachIndexed { i, sum ->
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                dots.getOrNull(i)?.let { Box(Modifier.size(10.dp).clip(CircleShape).background(it)) }
                                Text(sum.label, fontSize = 14.sp, fontWeight = if (i == n.sums.lastIndex) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        n.sums.forEachIndexed { i, sum -> Text(sum.amountText, fontSize = 14.sp, fontWeight = if (i == n.sums.lastIndex) FontWeight.SemiBold else FontWeight.Normal) }
                    }
                    if (n.sums.any { it.fiatText != null }) {
                        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            n.sums.forEach { Text(it.fiatText ?: "", color = Fob.muted, fontSize = 14.sp) }
                        }
                    }
                }
                Text(n.allReadyLine, color = Fob.muted, fontSize = 13.sp)
            }
        }
    }
}

/**
 * The thin bar under the balance: what's ready in ink, what isn't on the
 * track beside it, each as wide as its share, the not-ready part filled
 * green as far as its confirmations go (weighted by amount).
 */
@Composable
fun CoinBar(ready: ULong, notReady: ULong, progress: Float) {
    Row(
        Modifier.fillMaxWidth().height(6.dp).semantics { progressBarRangeInfo = ProgressBarRangeInfo(progress.coerceIn(0f, 1f), 0f..1f) },
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        if (ready > 0u) Box(Modifier.weight(ready.toFloat()).height(6.dp).clip(RoundedCornerShape(3.dp)).background(Fob.ink))
        if (notReady > 0u) {
            Box(Modifier.weight(notReady.toFloat()).height(6.dp).clip(RoundedCornerShape(3.dp)).background(Fob.track)) {
                Box(Modifier.fillMaxWidth(progress.coerceIn(0f, 1f)).height(6.dp).background(Fob.green))
            }
        }
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
