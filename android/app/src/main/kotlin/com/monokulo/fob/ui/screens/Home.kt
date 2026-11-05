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
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
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
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layoutId
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Avatar
import com.monokulo.fob.ui.rememberPayReading
import com.monokulo.fob.ui.rememberTouchState
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.EdgeButton
import com.monokulo.fob.ui.EdgeIconButton
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.FobMark
import com.monokulo.fob.ui.IconBadge
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.Manrope
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.SyncChip
import com.monokulo.fob.ui.Warning
import com.monokulo.fob.ui.SyncButton
import com.monokulo.fob.ui.display
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.fob.ActivityRow
import uniffi.fob.BackupReminder
import uniffi.fob.Network
import uniffi.fob.NotReady

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun HomeScreen(model: WalletModel, nav: Nav, openLocked: (Screen) -> Unit, confirmIdentity: suspend (String) -> Boolean, identityPending: () -> Boolean) {
    val wallet by model.wallet.collectAsState()
    val balance by model.balance.collectAsState()
    val activity by model.activity.collectAsState()
    val status by model.status.collectAsState()
    val notReady by model.notReady.collectAsState()
    val prices by model.prices.collectAsState()
    val network by model.network.collectAsState()
    val current by model.current.collectAsState()
    val wallets by model.wallets.collectAsState()
    val synced = status?.synced == true
    val canSpend = wallet?.canSpend() != false
    val scope = rememberCoroutineScope()
    var refreshing by remember { mutableStateOf(false) }
    // A new wallet's words not yet written down (Fob closed during the backup step, say): asked again here.
    val reminder by model.reminder.collectAsState()
    // Ready times count down between blocks: Rust writes them again each half minute.
    LaunchedEffect(notReady != null) {
        while (notReady != null) {
            delay(30_000)
            model.refresh()
        }
    }

    // Touch to pay, the reader: a phone on Receive held to this one raises the pay drawer with what it asks.
    val touch = rememberTouchState(model)
    var payCode by remember { mutableStateOf<String?>(null) }
    var touchNotice by remember { mutableStateOf<String?>(null) }
    rememberPayReading(model, touch, enabled = canSpend && payCode == null, onProblem = { touchNotice = it }) { code -> touchNotice = null; payCode = code }
    Box(Modifier.fillMaxSize()) {
    val manual by model.manual.collectAsState()
    // Manual Sync can turn the gesture off altogether.
    val pullOff = manual?.on == true && manual?.pullMode == uniffi.fob.PullMode.DISABLED
    Page(refreshing = refreshing, onRefresh = if (pullOff) null else ({
        refreshing = true
        scope.launch {
            model.syncNow()
            refreshing = false
        }
    })) {
        // Tapping the chip opens Rust's details card under it, Node settings on it.
        var syncOpen by rememberSaveable { mutableStateOf(false) }
        // Meet takes the place your photo had, beside the cog; your profile is in Settings.
        // Fob's name until there's a second wallet to tell this one from.
        val title = current?.name?.takeIf { wallets.size > 1 }
        TopBar(network == Network.STAGENET, title, onWallets = { nav.go(Screen.Wallets) }, onMeet = { nav.go(Screen.Meet) }, onSettings = { nav.go(Screen.Settings) }) {
            val s = status
            // Manual Sync: a button in the chip's place, but when something's wrong.
            if (s?.manualSync == true && s.error == null) SyncButton(s, model::openScan)
            else SyncChip(status, syncOpen, { syncOpen = it }, model::syncDetails, onScanAgain = model::scanAgain) { nav.go(Screen.Node) }
        }

        // The balance on one line: what goes first large on the left, the
        // other at the line's end. Tapping swaps them, only when Rust says
        // it can (a default currency besides XMR, with its worth).
        val b = balance
        val fiat = b?.fiatText
        val canSwap = b?.canSwap == true
        val fiatFirst = fiat != null && b.fiatFirst
        // Nothing over it while syncing (the chip says it's catching up), so it never moves as syncing ends.
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 12.dp).padding(top = 8.dp, bottom = 12.dp).clip(RoundedCornerShape(12.dp))
                .then(
                    if (canSwap) {
                        Modifier.clickable(role = Role.Button, onClickLabel = "Swap XMR and ${prices?.currency ?: "the default currency"}") {
                            scope.launch { runCatching { model.setFiatFirst(!b.fiatFirst) } }
                        }
                    } else Modifier,
                )
                .semantics(mergeDescendants = true) {}
                .padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Filling the line, so the other amount sits at its end.
            Row(Modifier.weight(1f).alignByBaseline(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                BasicText(
                    if (fiatFirst) fiat else b?.totalText ?: "—",
                    style = display(48).copy(color = Fob.ink, lineHeight = 50.sp),
                    maxLines = 1, autoSize = TextAutoSize.StepBased(minFontSize = 24.sp, maxFontSize = 48.sp),
                    modifier = Modifier.weight(1f, fill = false).alignByBaseline(),
                )
                if (!fiatFirst) Text("XMR", fontFamily = Manrope, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = Fob.muted, modifier = Modifier.alignByBaseline())
                // Money first at a price over 10 minutes old: how old, beside it.
                else b.fiatAge?.let { Text(it, fontSize = 15.sp, color = Fob.muted, maxLines = 1, modifier = Modifier.alignByBaseline()) }
            }
            // At the end, the money with its price's age once old: "$412.06 · 2 h old".
            val end = if (fiatFirst) "${b.totalText} XMR" else b?.fiatLine
            end?.let { Text(it, color = Fob.muted, fontSize = 17.sp, maxLines = 1, modifier = Modifier.alignByBaseline()) }
        }

        Column(Modifier.padding(horizontal = 16.dp)) {
            // The words behind the phone's lock, as from Backup and keys.
            reminder?.let { BackupCard(it) { openLocked(Screen.ShowSeed) } }
            notReady?.let { ReadyCard(it) }
        }

        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlineButton("Receive", { nav.go(Screen.Receive()) }, Modifier.weight(1f), height = 52.dp, leading = Icons.down)
                if (canSpend) PrimaryButton("Send", { nav.go(Screen.Send()) }, Modifier.weight(1f), height = 52.dp, leading = Icons.up)
            }
            if (!canSpend) Text("This wallet only watches: it can see payments but not send them.", color = Fob.muted, fontSize = 13.sp)
        }

        Card(Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 24.dp), padding = PaddingValues(vertical = 8.dp)) {
            // The chips go under the title, one under the other if need be, when a narrow phone or a big font leaves
            // no room beside it; the note while syncing has a line of its own under the title.
            FlowRow(
                Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp, top = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp), itemVerticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Activity", fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f).semantics { heading() })
                Chip(Icons.people, "Contacts") { nav.go(Screen.Contacts) }
                Chip(Icons.coins, "Coins") { nav.go(Screen.Coins) }
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
                    PaymentRow(row) { open(nav, row) }
                }
            }
            // The list's last row while syncing: going as syncing ends moves nothing above it.
            if (activity.isNotEmpty() && !synced) {
                Text("Still finding older ones", color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 12.dp))
            }
        }
    }
    payCode?.let { code -> PayDrawer(model, nav, code, confirmIdentity, onClose = { payCode = null }) }
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
                val dots = listOf(Fob.ink, Fob.green)
                val priced = n.sums.any { it.fiatText != null }
                // One line per sum, read as one by TalkBack ("Ready to spend, 1.8592 XMR, $308.70"); the money in a
                // column wide enough for it, so the amounts line up.
                n.sums.forEachIndexed { i, sum ->
                    val weight = if (i == n.sums.lastIndex) FontWeight.SemiBold else FontWeight.Normal
                    Row(
                        Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = sum.description },
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        dots.getOrNull(i)?.let { Box(Modifier.size(10.dp).clip(CircleShape).background(it)) }
                        Text(sum.label, fontSize = 14.sp, fontWeight = weight, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                        Text(sum.amountText, fontSize = 14.sp, fontWeight = weight)
                        if (priced) Text(sum.fiatText ?: "", color = Fob.muted, fontSize = 14.sp, textAlign = TextAlign.End, modifier = Modifier.widthIn(min = 80.dp).padding(start = 8.dp))
                    }
                }
                Text(n.allReadyLine, color = Fob.muted, fontSize = 13.sp)
            }
        }
    }
}

/** Rust's reminder that a new wallet's seed phrase isn't written down; it opens the words after the phone's lock. */
@Composable
private fun BackupCard(r: BackupReminder, onClick: () -> Unit) {
    Warning(r.line, Modifier.padding(bottom = 12.dp), title = r.title, icon = Icons.lines, onClick = onClick)
}

/** Activity's ways to Contacts and Coins: a sunken pill, 40dp tall within a 48dp target. */
@Composable
private fun Chip(icon: String, label: String, onClick: () -> Unit) {
    Box(Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(24.dp)).clickable(role = Role.Button, onClick = onClick), contentAlignment = Alignment.Center) {
        Row(
            Modifier.heightIn(min = 40.dp).clip(RoundedCornerShape(20.dp)).background(Fob.sunken).padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            StrokeIcon(icon, size = 18.dp, width = 1.8f)
            Text(label, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
        }
    }
}

/**
 * Home's top bar (board HomePeople): the mark, "Fob" and on stagenet its
 * badge at the start; the sync [chip], your photo and Settings at the end.
 * The end's three always fit whole. Where the row is too narrow (a 360dp
 * phone with the badge and "6 min left", a big font) the word "Fob" goes
 * first, then the badge, then the chip's words (its ring or dot stays, its
 * card and TalkBack keep the words), each only when that lets the chip
 * keep its words; where even that won't, the chip's words go and "Fob" and
 * the badge stay as far as they fit.
 */
@Composable
private fun TopBar(stagenet: Boolean, wallet: String?, onWallets: () -> Unit, onMeet: () -> Unit, onSettings: () -> Unit, chip: @Composable () -> Unit) {
    val walletsLabel = wallet?.let { "Wallets, $it open" } ?: "Wallets"
    Layout(
        content = {
            // With no room for the word, the mark is still the way to the wallets.
            Box(
                Modifier.layoutId("mark").size(48.dp).clip(CircleShape).clickable(role = Role.Button, onClickLabel = walletsLabel, onClick = onWallets)
                    .semantics { contentDescription = walletsLabel },
                contentAlignment = Alignment.Center,
            ) { FobMark(26.dp) }
            Text(
                wallet?.let { if (it.length > 12) it.take(11).trimEnd() + "…" else it } ?: "Fob", style = display(22), maxLines = 1,
                modifier = Modifier.layoutId("word").clip(RoundedCornerShape(8.dp)).clickable(onClick = onWallets).clearAndSetSemantics {},
            )
            if (stagenet) Text("Stagenet", color = Fob.orangeText, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, modifier = Modifier.layoutId("badge").clip(RoundedCornerShape(10.dp)).background(Fob.highlight).padding(horizontal = 8.dp, vertical = 2.dp))
            Box(Modifier.layoutId("chip")) { chip() }
            MeetButton(onMeet, Modifier.layoutId("you"))
            EdgeIconButton(Icons.gear, "Settings", onSettings, Modifier.layoutId("cog"), width = 1.8f)
        },
        modifier = Modifier.fillMaxWidth().padding(start = 9.dp, end = EdgeButton.inset).heightIn(min = 64.dp),
    ) { parts, c ->
        fun part(id: String) = parts.firstOrNull { it.layoutId == id }
        val loose = c.copy(minWidth = 0, minHeight = 0)
        val gap = 8.dp.roundToPx()
        val mark = part("mark")!!.measure(loose)
        val you = part("you")!!.measure(loose)
        val cog = part("cog")!!.measure(loose)
        // The chip's room after the mark, a gap, and the two buttons with a little space before them.
        val room = c.maxWidth - mark.width - gap - 4.dp.roundToPx() - you.width - cog.width
        val word = part("word")!!
        val badge = part("badge")
        val wordWidth = word.maxIntrinsicWidth(Constraints.Infinity) + gap
        val badgeWidth = badge?.let { it.maxIntrinsicWidth(Constraints.Infinity) + gap } ?: 0
        // A chip without its words is the 48dp of its target; with them, as wide as they make it.
        val ring = 48.dp.roundToPx()
        val full = part("chip")!!.maxIntrinsicWidth(c.maxHeight)
        val (showWord, showBadge) = when {
            wordWidth + badgeWidth + full <= room -> true to true
            badgeWidth + full <= room -> false to true
            full <= room -> false to false
            // The chip's words can't stay whatever goes: they go, and the rest stays as far as it fits.
            else -> (wordWidth + badgeWidth + ring <= room) to (badgeWidth + ring <= room)
        }.let { (word, shown) -> word to (badge != null && shown) }
        val chipRoom = room - (if (showWord) wordWidth else 0) - (if (showBadge) badgeWidth else 0)
        val chip = part("chip")!!.measure(loose.copy(maxWidth = chipRoom.coerceAtLeast(0)))
        val start = listOfNotNull(mark, if (showWord) word.measure(loose) else null, if (showBadge) badge!!.measure(loose) else null)
        val height = c.constrainHeight((start + listOf(chip, you, cog)).maxOf { it.height })
        layout(c.maxWidth, height) {
            var x = 0
            start.forEach { it.placeRelative(x, (height - it.height) / 2); x += it.width + gap }
            x = c.maxWidth
            listOf(cog, you).forEach { x -= it.width; it.placeRelative(x, (height - it.height) / 2) }
            chip.placeRelative(x - 4.dp.roundToPx() - chip.width, (height - chip.height) / 2)
        }
    }
}

/** Meet: the person with waves in a 40dp circle on the page's chip colour, in a 56dp edge button. */
@Composable
private fun MeetButton(onClick: () -> Unit, modifier: Modifier) {
    Box(
        modifier.size(EdgeButton.target).clip(CircleShape).clickable(role = Role.Button, onClick = onClick).semantics { contentDescription = "Meet" },
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(EdgeButton.avatar).clip(CircleShape).background(Fob.onPaper), contentAlignment = Alignment.Center) {
            StrokeIcon(Icons.meet, tint = Fob.ink, size = 22.dp, width = 1.8f)
        }
    }
}

/**
 * The thin bar under the balance: what's ready in ink, what isn't on the
 * track beside it, each as wide as its share, the not-ready part filled
 * green as far as its confirmations go (weighted by amount). TalkBack reads
 * Rust's [description] of it, or nothing where the words around it say the
 * same.
 */
@Composable
fun CoinBar(ready: ULong, notReady: ULong, progress: Float, description: String? = null) {
    Row(
        Modifier.fillMaxWidth().height(6.dp).clearAndSetSemantics {
            if (description != null) {
                contentDescription = description
                progressBarRangeInfo = ProgressBarRangeInfo(progress.coerceIn(0f, 1f), 0f..1f)
            }
        },
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

/** Opens an Activity row: a payment's screen (its receipt, if it had one), a request's, or a share's code again. */
fun open(nav: Nav, row: ActivityRow) {
    val a = row.activity
    val request = row.request
    val share = row.share
    when {
        a != null -> nav.go(if (a.receipt != null) Screen.Receipt(a) else Screen.Payment(a))
        request != null -> nav.go(Screen.Request(request))
        share != null -> nav.go(Screen.ProfileShare(share))
    }
}

/**
 * One payment, a request (Round 5 › "Activity: requests and payments,
 * separate lines": the code icon, the amount asked in grey) or a share of
 * your profile (Round 6 › "Activity: each share is a line": a person, no
 * amount): what Rust wrote for it, drawn.
 */
@Composable
fun PaymentRow(row: ActivityRow, onClick: () -> Unit) {
    val a = row.activity
    val outgoing = a?.outgoing == true
    val (icon, tint) = when {
        row.share != null -> Icons.meet to Fob.muted
        a == null -> Icons.request to Fob.muted
        outgoing -> Icons.up to Fob.ink
        else -> Icons.down to Fob.green
    }
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        IconBadge(icon, tint, size = 28.dp, receipt = a?.receipt != null)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(row.title, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(row.line, color = Fob.muted, fontSize = 12.sp)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            if (row.amountText.isNotEmpty()) Text(row.amountText, fontSize = 15.sp, fontWeight = FontWeight.Medium, color = if (a == null) Fob.muted else tint)
            row.fiatText?.let { Text(it, color = Fob.muted, fontSize = 12.sp) }
        }
    }
}
