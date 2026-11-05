package com.monokulo.fob.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.SectionLabel
import com.monokulo.fob.ui.SettingRow
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.time.LocalDate
import java.time.YearMonth
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale
import uniffi.fob.PullMode
import uniffi.fob.ScanRequest
import uniffi.fob.ScanView

/**
 * What goes over every screen: the Scan drawer, its running sheet, a pull's
 * result, and a payment the node refused with Manual Sync on.
 */
@Composable
fun ScanOverlays(model: WalletModel) {
    val open by model.scanOpen.collectAsState()
    val status by model.status.collectAsState()
    val refused by model.spendRefused.collectAsState()
    val notice by model.scanNotice.collectAsState()
    val scan = status?.scan

    if (open) {
        var seenRunning by remember { mutableStateOf(false) }
        if (scan != null) seenRunning = true
        // It finished with the sheet open: nothing more to show.
        LaunchedEffect(scan == null, seenRunning) { if (scan == null && seenRunning) model.closeScan() }
        if (scan != null) RunningSheet(model, scan) else if (!seenRunning) ScanDrawerSheet(model)
    }
    if (refused) {
        BottomSheet("Nothing was sent", onDismiss = model::dismissRefused) {
            SheetHandle()
            Column(Modifier.padding(horizontal = 4.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    StrokeIcon(Icons.warning, tint = Fob.orangeText, size = 24.dp)
                    Text("Nothing was sent", style = display(24), modifier = Modifier.semantics { heading() })
                }
                Text(
                    "Your node refused this payment. A coin in it may already be spent, maybe from another device. Sync so Fob knows, then try again.",
                    color = Fob.muted, fontSize = 16.sp, lineHeight = 23.sp,
                )
                PrimaryButton("Sync", { model.dismissRefused(); model.openScan() })
                OutlineButton("Close", model::dismissRefused, Modifier.fillMaxWidth())
            }
        }
    }
    notice?.let { said ->
        LaunchedEffect(said) {
            delay(5_000)
            model.dismissScanNotice()
        }
        Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.navigationBars).padding(start = 16.dp, end = 16.dp, bottom = 16.dp), contentAlignment = Alignment.BottomCenter) {
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Fob.inverse).clickable(onClickLabel = "Dismiss", onClick = model::dismissScanNotice)
                    .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }.padding(horizontal = 20.dp, vertical = 14.dp),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(said.title, color = Fob.onInverse, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text(said.line, color = Fob.inverseMuted, fontSize = 14.sp, lineHeight = 20.sp)
            }
        }
    }
}

/** Quick, Dates, Block: a sunken pill, the one chosen lifted on the card's white. */
@Composable
private fun PillTabs(labels: List<String>, selected: Int, onSelect: (Int) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(26.dp)).background(Fob.sunken).padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        labels.forEachIndexed { i, label ->
            val on = i == selected
            Box(
                Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(22.dp)).background(if (on) Fob.card else Color.Transparent)
                    .clickable(role = Role.Tab, onClickLabel = label) { onSelect(i) },
                contentAlignment = Alignment.Center,
            ) {
                Text(label, color = if (on) Fob.ink else Fob.muted, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            }
        }
    }
}

/** The Scan drawer: what to look through, and what that costs. */
@Composable
private fun ScanDrawerSheet(model: WalletModel) {
    val scope = rememberCoroutineScope()
    val status by model.status.collectAsState()
    val drawer by produceState<uniffi.fob.ScanDrawer?>(null) { value = model.scanDrawer() }
    var tab by remember { mutableStateOf(0) }
    // The second choice: it's what Sync is usually for.
    var picked by remember { mutableStateOf(1) }
    var from by remember { mutableStateOf<LocalDate?>(null) }
    var to by remember { mutableStateOf<LocalDate?>(null) }
    var height by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    fun start(request: ScanRequest) {
        busy = true
        error = null
        scope.launch {
            runCatching { model.startScan(request) }
                .onSuccess { checked -> if (checked) model.closeScan() }
                .onFailure { error = it.reason() }
            busy = false
        }
    }

    BottomSheet("Scan", onDismiss = model::closeScan) {
        SheetHandle()
        Column(Modifier.padding(horizontal = 4.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Column {
                Text("Scan", style = display(24), modifier = Modifier.semantics { heading() })
                drawer?.let { Text(it.lastScan, color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp, modifier = Modifier.padding(top = 4.dp)) }
            }
            PillTabs(listOf("Quick", "Dates", "Block"), tab) { tab = it }
            when (tab) {
                0 -> Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Fob.card).padding(vertical = 4.dp)) {
                    drawer?.options?.forEachIndexed { i, option ->
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 60.dp).selectable(i == picked, role = Role.RadioButton) { picked = i }.padding(horizontal = 20.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
                        ) {
                            Box(
                                Modifier.size(22.dp).clip(CircleShape).border(if (i == picked) 7.dp else 2.dp, if (i == picked) Fob.orange else Fob.outline, CircleShape),
                            )
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                Text(option.label, fontSize = 16.sp, fontWeight = FontWeight.Medium)
                                Text(option.note, fontSize = 13.sp, color = Fob.muted)
                            }
                        }
                    }
                }
                1 -> DatesTab(from, to) { newFrom, newTo -> from = newFrom; to = newTo }
                else -> BlockTab(model, height, status?.tip ?: 0uL) { height = it }
            }
            ErrorText(error)
            when (tab) {
                0 -> drawer?.options?.getOrNull(picked)?.let { option ->
                    Text(option.cost, color = Fob.muted, fontSize = 14.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
                    PrimaryButton(option.action, { start(option.request) }, enabled = !busy)
                }
                1 -> PrimaryButton(
                    "Scan these dates",
                    {
                        val zone = ZoneId.systemDefault()
                        from?.let { first ->
                            start(ScanRequest.Dates(first.atStartOfDay(zone).toEpochSecond().toULong(), to?.plusDays(1)?.atStartOfDay(zone)?.toEpochSecond()?.toULong()))
                        }
                    },
                    enabled = !busy && from != null,
                )
                else -> {
                    val block = height.toULongOrNull()
                    PrimaryButton("Scan from this block", { block?.let { start(ScanRequest.Height(it)) } }, enabled = !busy && block != null && block < (status?.tip ?: 0uL))
                }
            }
        }
    }
}

/** From, To and a month to pick them in. With no To, the scan runs up to now. */
@Composable
private fun DatesTab(from: LocalDate?, to: LocalDate?, onChange: (LocalDate?, LocalDate?) -> Unit) {
    var month by remember { mutableStateOf(YearMonth.from(from ?: LocalDate.now())) }
    val shown = DateTimeFormatter.ofPattern("d MMM yyyy")
    val today = LocalDate.now()
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            DateBox("From", from?.format(shown) ?: "Pick a day", Modifier.weight(1f))
            DateBox("To", to?.format(shown) ?: "Now", Modifier.weight(1f))
        }
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Fob.card).padding(start = 12.dp, end = 12.dp, top = 6.dp, bottom = 12.dp)) {
            Row(Modifier.fillMaxWidth().height(44.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(44.dp).clip(CircleShape).clickable(role = Role.Button, onClickLabel = "Month before") { month = month.minusMonths(1) }, contentAlignment = Alignment.Center) {
                    StrokeIcon(Icons.back, size = 18.dp)
                }
                Text(
                    month.month.getDisplayName(TextStyle.FULL, Locale.getDefault()) + " " + month.year,
                    fontSize = 15.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center, modifier = Modifier.weight(1f),
                )
                val can = month < YearMonth.from(today)
                Box(
                    Modifier.size(44.dp).clip(CircleShape).clickable(enabled = can, role = Role.Button, onClickLabel = "Month after") { month = month.plusMonths(1) },
                    contentAlignment = Alignment.Center,
                ) { if (can) StrokeIcon(Icons.chevron, size = 18.dp) }
            }
            Row(Modifier.fillMaxWidth().clearAndSetSemantics {}) {
                java.time.DayOfWeek.entries.forEach { weekday ->
                    Text(weekday.getDisplayName(TextStyle.NARROW, Locale.getDefault()), color = Fob.muted, fontSize = 12.sp, fontWeight = FontWeight.Medium, textAlign = TextAlign.Center, modifier = Modifier.weight(1f))
                }
            }
            val lead = month.atDay(1).dayOfWeek.value - 1
            val cells: List<Int?> = List(lead) { null } + (1..month.lengthOfMonth()).toList()
            cells.chunked(7).forEach { week ->
                Row(Modifier.fillMaxWidth()) {
                    week.forEach { n ->
                        Box(Modifier.weight(1f).aspectRatio(1.15f), contentAlignment = Alignment.Center) {
                            if (n != null) {
                                val day = month.atDay(n)
                                val end = day == from || day == to
                                val inside = from != null && to != null && day > from && day < to
                                val future = day > today
                                Box(
                                    Modifier.fillMaxSize().clip(RoundedCornerShape(if (end) 17.dp else 0.dp))
                                        .background(if (end) Fob.orange else if (inside) Fob.selectedFill else Color.Transparent)
                                        .clickable(enabled = !future, role = Role.Button, onClickLabel = day.format(shown)) {
                                            when {
                                                from == null || to != null || day < from -> onChange(day, null)
                                                else -> onChange(from, day)
                                            }
                                        },
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text(
                                        "$n", fontSize = 14.sp, fontWeight = if (end) FontWeight.SemiBold else FontWeight.Normal,
                                        color = if (end) Fob.onOrange else if (future) Fob.outline else Fob.ink,
                                    )
                                }
                            }
                        }
                    }
                    repeat(7 - week.size) { Box(Modifier.weight(1f)) }
                }
            }
        }
    }
}

@Composable
private fun DateBox(label: String, value: String, modifier: Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, color = Fob.muted, fontSize = 14.sp)
        Box(
            Modifier.fillMaxWidth().height(52.dp).clip(RoundedCornerShape(14.dp)).background(Fob.card).border(1.dp, Fob.outline, RoundedCornerShape(14.dp)).padding(horizontal = 12.dp),
            contentAlignment = Alignment.CenterStart,
        ) { Text(value, fontSize = 16.sp, maxLines = 1) }
    }
}

/** A block to scan from, and what's known around it. */
@Composable
private fun BlockTab(model: WalletModel, height: String, tip: ULong, onChange: (String) -> Unit) {
    val block = height.toULongOrNull()
    val info by produceState<uniffi.fob.BlockInfo?>(null, block) { value = model.blockInfo(block ?: 0uL) }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LabelledInput(
            "From block", height, { typed -> onChange(typed.filter(Char::isDigit).take(12)) }, "3,800,000", fontSize = 18,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        )
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Fob.card).padding(horizontal = 20.dp, vertical = 4.dp)) {
            info?.let { facts ->
                if (block != null) facts.mined?.let { FactRow("Mined", it); Divider() }
                FactRow("Newest block", facts.newest)
                Divider()
                FactRow("Wallet's first block", facts.first)
            }
        }
        if (block != null && block >= tip) Text("That's past the newest block.", color = Fob.red, fontSize = 13.sp)
    }
}

@Composable
private fun FactRow(label: String, value: String) {
    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, color = Fob.muted, fontSize = 15.sp, modifier = Modifier.weight(1f))
        Text(value, fontSize = 15.sp, fontWeight = FontWeight.Medium)
    }
}

/** A scan under way: how far, and on its bar where payments were found. */
@Composable
private fun RunningSheet(model: WalletModel, scan: ScanView) {
    BottomSheet(scan.title, onDismiss = model::closeScan) {
        SheetHandle()
        Column(Modifier.padding(horizontal = 4.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Column {
                Text(scan.title, style = display(24), modifier = Modifier.semantics { heading() })
                Text("You can close this. Fob keeps going, and you can still send.", color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp, modifier = Modifier.padding(top = 4.dp))
            }
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Fob.card).padding(horizontal = 20.dp, vertical = 16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(scan.blocks, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                ScanBar(scan)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlineButton("Stop", model::stopScan, Modifier.weight(1f))
                PrimaryButton("Close", model::closeScan, Modifier.weight(1f))
            }
        }
    }
}

/** Orange as far as it's got. Payments are marked in the colours of Activity's arrows, money in on top where they meet. */
@Composable
private fun ScanBar(scan: ScanView) {
    val found = scan.marks.count { it.incoming } to scan.marks.count { !it.incoming }
    val spoken = "${(scan.progress * 100).toInt()} percent, ${found.first} incoming and ${found.second} outgoing payments found"
    BoxWithConstraints(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)).background(Fob.track).semantics { contentDescription = spoken }) {
        Box(Modifier.width(maxWidth * scan.progress.coerceIn(0f, 1f)).fillMaxSize().clip(RoundedCornerShape(4.dp)).background(Fob.orange))
        val room = maxWidth - 8.dp
        scan.marks.sortedBy { it.incoming }.forEach { mark ->
            Box(Modifier.offset(x = room * mark.position).size(8.dp).background(if (mark.incoming) Fob.green else Fob.ink))
        }
    }
}

/** Manual Sync's settings: off by default, and asked about before it's turned on. */
@Composable
fun ManualSyncScreen(model: WalletModel, nav: Nav) {
    val scope = rememberCoroutineScope()
    val settings by model.manual.collectAsState()
    var asking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val s = settings

    Box(Modifier.fillMaxSize()) {
        Page {
            TopBar("Manual Sync", onBack = { nav.back() })
            Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Card {
                    SettingRow(
                        "Manual Sync", "Only scan when you ask.", checked = s?.on == true,
                        onChecked = { on -> if (on) asking = true else scope.launch { runCatching { model.setManualSync(false) }.onFailure { error = it.reason() } } },
                    )
                }
                if (s?.on == true) {
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        SectionLabel("Pull to refresh")
                        val modes = listOf(PullMode.DISABLED, PullMode.SEND, PullMode.AUTO)
                        PillTabs(listOf("Disabled", "Send", "Auto"), modes.indexOf(s.pullMode)) { i ->
                            scope.launch { runCatching { model.setPullMode(modes[i]) }.onFailure { error = it.reason() } }
                        }
                        Text(
                            when (s.pullMode) {
                                PullMode.DISABLED -> "Pulling down on Home does nothing. Sync still works."
                                PullMode.SEND -> "Updates payments you've sent, and their change."
                                PullMode.AUTO -> "Also looks for new money in the node's pool and recent blocks. Your node sees each pull."
                            },
                            color = Fob.muted, fontSize = 14.sp, lineHeight = 20.sp, modifier = Modifier.padding(horizontal = 4.dp),
                        )
                    }
                    if (s.pullMode == PullMode.AUTO) {
                        NumberSetting("Incoming scan blocks", s.incomingScanBlocks, 1u, uniffi.fob.manualIncomingMax(), "1 to ${uniffi.fob.manualIncomingMax()}. A block is about 2 minutes.") { blocks ->
                            model.setIncomingScanBlocks(blocks)
                        }
                    }
                    NumberSetting("Settled confirmations", s.settledConfirmations, uniffi.fob.manualSettledMin(), uniffi.fob.manualSettledMax(), "${uniffi.fob.manualSettledMin()} to ${uniffi.fob.manualSettledMax()}. Coins can be spent from here, and payments stop being checked.") { confirmations ->
                            model.setSettledConfirmations(confirmations)
                    }
                }
                ErrorText(error)
            }
        }
        if (asking) {
            BottomSheet("Turn on Manual Sync?", onDismiss = { asking = false }) {
                SheetHandle()
                Column(Modifier.padding(horizontal = 4.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text("Turn on Manual Sync?", style = display(24), modifier = Modifier.semantics { heading() })
                    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Fob.card).padding(horizontal = 20.dp, vertical = 4.dp)) {
                        TurnOnRow("Fob stops looking for blocks", "Instant updates turn off too. It scans when you press Sync. What pulling down on Home does is up to you.")
                        Divider()
                        TurnOnRow("New payments stay hidden until then", "The money is still yours. Your balance is as of your last scan.")
                        Divider()
                        TurnOnRow("You can always send", "Fob asks your node only for what a payment needs. If a coin was spent from another device, the payment fails and nothing is lost.")
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlineButton("Not now", { asking = false }, Modifier.weight(1f))
                        PrimaryButton(
                            "Turn on",
                            {
                                asking = false
                                scope.launch { runCatching { model.setManualSync(true) }.onFailure { error = it.reason() } }
                            },
                            Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun TurnOnRow(title: String, line: String) {
    Column(Modifier.padding(vertical = 13.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(title, fontSize = 16.sp, fontWeight = FontWeight.Medium)
        Text(line, color = Fob.muted, fontSize = 14.sp, lineHeight = 20.sp)
    }
}

/** A number typed in, kept once it's within its limits. */
@Composable
private fun ColumnScope.NumberSetting(label: String, value: UInt, min: UInt, max: UInt, hint: String, onSet: suspend (UInt) -> Unit) {
    val scope = rememberCoroutineScope()
    var typed by remember(value) { mutableStateOf(value.toString()) }
    val number = typed.toUIntOrNull()
    val valid = number != null && number in min..max
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        LabelledInput(
            label, typed,
            { text ->
                typed = text.filter(Char::isDigit).take(3)
                val next = typed.toUIntOrNull()
                if (next != null && next in min..max && next != value) scope.launch { runCatching { onSet(next) } }
            },
            value.toString(), fontSize = 17, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        )
        Text(hint, color = if (valid) Fob.muted else Fob.red, fontSize = 13.sp, lineHeight = 18.sp, modifier = Modifier.padding(horizontal = 4.dp))
    }
}
