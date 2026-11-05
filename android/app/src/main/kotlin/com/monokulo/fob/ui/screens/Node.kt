package com.monokulo.fob.ui.screens

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.EdgeButton
import com.monokulo.fob.ui.EdgeIconButton
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.LabelAndValue
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.SectionLabel
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.fob.CheckMark
import uniffi.fob.NodeCheckObserver
import uniffi.fob.NodeCheckRow
import uniffi.fob.NodeChoice
import uniffi.fob.NodeReport
import uniffi.fob.NodeSetting
import uniffi.fob.NodeUse

/**
 * Node (board 11): the node in use with Rust's checks, checked on opening
 * and by the refresh at the top; other nodes to change to, each checked
 * with Use on the ones that pass, the wallet's own removable; and Add your
 * own node's sheet (board 12).
 */
@Composable
fun NodeScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    var current by remember { mutableStateOf<NodeReport?>(null) }
    var choices by remember { mutableStateOf<List<NodeChoice>>(emptyList()) }
    var round by remember { mutableIntStateOf(0) }
    var adding by rememberSaveable { mutableStateOf(false) }
    val changing by model.nodeChanging.collectAsState()
    val failed by model.nodeError.collectAsState()
    val error = failed?.reason()
    // Rust's async calls run on the thread that calls them: these do a lot, and nothing here is on the main thread for them.
    LaunchedEffect(wallet, round) {
        val w = wallet ?: return@LaunchedEffect
        val mine = round
        withContext(Dispatchers.Default) {
            val node = w.node()
            launch {
                // Each check as it finishes; a round started since is left alone.
                val observer = object : NodeCheckObserver {
                    override fun onProgress(report: NodeReport) { if (round == mine) current = report }
                }
                val done = w.checkNode(node, observer)
                if (round == mine) current = done
            }
            launch {
                val others = w.otherNodes()
                if (round == mine) choices = others
                val checked = w.checkNodes(others.map { it.node })
                if (round == mine) choices = checked
            }
        }
    }
    // The change is over: what's checked now is the node in use (or, refused, the one before).
    var wasChanging by remember { mutableStateOf(changing != null) }
    LaunchedEffect(changing) {
        if (wasChanging && changing == null) round++
        wasChanging = changing != null
    }
    val checking = current?.checking ?: true
    // At once: the sheet goes and the card says what's happening, however long the node takes to answer.
    fun use(node: NodeSetting) {
        adding = false
        model.useNode(node.url, node.selfSigned)
    }
    // Off the list at once; the checks then run on what Rust has kept.
    fun remove(node: NodeSetting) {
        val w = wallet ?: return
        choices = choices.filter { it.node.url != node.url }
        scope.launch {
            withContext(Dispatchers.Default) { runCatching { w.removeNode(node.url) } }
            round++
        }
    }
    Box(Modifier.fillMaxSize()) {
        Page {
            TopBar("Node", onBack = { nav.back() }) {
                RefreshButton(checking || changing != null) { if (!checking && changing == null) round++ }
            }
            Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                val to = changing
                if (to != null) ChangingCard(to) else current?.let { WorkingCard(it) }
                ErrorText(error)
                Column {
                    SectionLabel("Change node")
                    Card {
                        choices.forEach { c -> ChoiceRow(c, using = c.node.url == to, canUse = to == null, onUse = { use(c.node) }, onRemove = { remove(c.node) }) }
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button) { model.clearNodeError(); adding = true }.padding(horizontal = 20.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            StrokeIcon(Icons.plus, tint = Fob.orangeText, size = 20.dp)
                            Text("Add your own node", color = Fob.orangeText, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                        }
                    }
                }
            }
        }
        if (adding) {
            AddNodeSheet(
                model,
                onUse = ::use,
                onClose = { checked ->
                    adding = false
                    val w = wallet
                    if (w != null && checked != null) {
                        scope.launch {
                            withContext(Dispatchers.Default) { runCatching { w.addNode(checked) } }
                            round++
                        }
                    }
                },
            )
        }
    }
}

/** Changing to another node, until it's answered and the checks can start. */
@Composable
private fun ChangingCard(address: String) {
    Card(padding = PaddingValues(start = 20.dp, end = 20.dp, top = 18.dp, bottom = 18.dp)) {
        Row(
            Modifier.fillMaxWidth().semantics(mergeDescendants = true) { heading(); liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            MarkCircle(CheckMark.CHECKING, 40.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Changing node", style = display(22))
                Text(address, fontSize = 13.sp, color = Fob.muted)
            }
        }
    }
}

/** The header's refresh: checks again, turning while the checks run. */
@Composable
private fun RefreshButton(checking: Boolean, onClick: () -> Unit) {
    val turning = rememberInfiniteTransition(label = "refresh")
    val angle by turning.animateFloat(0f, 360f, infiniteRepeatable(tween(900, easing = LinearEasing), RepeatMode.Restart), label = "angle")
    EdgeIconButton("Check again", onClick, Modifier.semantics { if (checking) stateDescription = "Checking" }) {
        Box(Modifier.rotate(if (checking) angle else 0f)) { StrokeIcon(Icons.refresh, size = EdgeButton.icon) }
    }
}

/** A check's mark: a filled circle with a tick, a "!" , dots while running, or a dash when not run. */
@Composable
private fun MarkCircle(mark: CheckMark, size: Dp) {
    val (fill, ink, path) = when (mark) {
        CheckMark.PASS -> Triple(Fob.green, Fob.paper, Icons.check)
        CheckMark.FAIL -> Triple(Fob.red, Fob.paper, Icons.alert)
        CheckMark.CHECKING -> Triple(Fob.sunken, Fob.muted, Icons.dots)
        CheckMark.SKIPPED -> Triple(Fob.sunken, Fob.muted, Icons.dash)
    }
    Box(Modifier.size(size).clip(CircleShape).background(fill), contentAlignment = Alignment.Center) {
        StrokeIcon(path, tint = ink, size = size * 0.6f, width = if (size > 30.dp) 2.4f else 3.4f)
    }
}

@Composable
private fun CheckRows(rows: List<NodeCheckRow>, minHeight: Dp, lines: Boolean) {
    Column {
        rows.forEach { row ->
            if (lines) Box(Modifier.fillMaxWidth().heightIn(min = 1.dp, max = 1.dp).background(Fob.line))
            // The result keeps its words whole at a big font; the title wraps first.
            LabelAndValue(
                Modifier.heightIn(min = minHeight).padding(vertical = 6.dp).clearAndSetSemantics { contentDescription = row.label },
                gap = 12.dp, centred = true,
                label = {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        MarkCircle(row.mark, 20.dp)
                        Text(row.title, fontSize = 14.sp)
                    }
                },
            ) {
                val colour = if (row.mark == CheckMark.FAIL) Fob.red else Fob.muted
                // Sends blocks to Fob's link speed goes on its own line under the blocks per second.
                Column(horizontalAlignment = Alignment.End) {
                    Text(row.result, fontSize = 13.sp, color = colour, textAlign = TextAlign.End)
                    row.second?.let { Text(it, fontSize = 13.sp, color = colour, textAlign = TextAlign.End) }
                }
            }
        }
    }
}

/** The node in use: the verdict, its address, and the checks. */
@Composable
private fun WorkingCard(report: NodeReport) {
    Card(padding = PaddingValues(start = 20.dp, end = 20.dp, top = 18.dp, bottom = 12.dp)) {
        Row(
            Modifier.fillMaxWidth().padding(bottom = 12.dp).semantics(mergeDescendants = true) { heading(); liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            MarkCircle(report.mark, 40.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(report.verdict, style = display(22))
                // Whole, wrapping if it must: it's what's being checked.
                Text(report.address, fontSize = 13.sp, color = Fob.muted)
            }
        }
        CheckRows(report.rows, minHeight = 40.dp, lines = true)
    }
}

/** Another node: its dot, address and one line, Use when it passed, and Remove when it's the wallet's own. */
@Composable
private fun ChoiceRow(choice: NodeChoice, using: Boolean, canUse: Boolean, onUse: () -> Unit, onRemove: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).padding(start = 20.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val dot = when (choice.mark) {
            CheckMark.PASS -> Fob.greenDot
            CheckMark.FAIL -> Fob.red
            else -> Fob.outline
        }
        Box(Modifier.size(10.dp).clip(CircleShape).background(dot))
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(choice.address, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(choice.line, fontSize = 12.5.sp, color = Fob.muted)
        }
        if (using) {
            Text("Changing", color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 12.dp))
        } else if (choice.canUse && canUse) {
            Box(
                Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp))
                    .clickable(role = Role.Button, onClickLabel = "Use ${choice.address}", onClick = onUse)
                    .semantics { contentDescription = "Use ${choice.address}" }
                    .padding(horizontal = 12.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text("Use", color = Fob.orangeText, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        if (choice.removable && canUse && !using) {
            EdgeIconButton(Icons.trash, "Remove ${choice.address}", onRemove, tint = Fob.muted)
        }
    }
}

/**
 * Add your own node (board 12): an address, whether to accept a
 * self-signed certificate, and the same checks; Use, or Use it anyway when
 * only Up to date failed. Closing hands back the node if it passed, to keep.
 */
@Composable
private fun AddNodeSheet(model: WalletModel, onUse: (NodeSetting) -> Unit, onClose: (NodeSetting?) -> Unit) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    var address by rememberSaveable { mutableStateOf("") }
    var selfSigned by rememberSaveable { mutableStateOf(false) }
    var report by remember { mutableStateOf<NodeReport?>(null) }
    var job by remember { mutableStateOf<Job?>(null) }
    // Counts runs, so one stopped early doesn't show its checks.
    var attempt by remember { mutableIntStateOf(0) }
    // Another address or box makes the last checks stale.
    fun edited() {
        attempt++
        job?.cancel()
        report = null
    }
    fun check() {
        val w = wallet ?: return
        val node = NodeSetting(address.trim(), selfSigned)
        val mine = ++attempt
        val show = { r: NodeReport -> if (attempt == mine) report = r }
        job?.cancel()
        job = scope.launch {
            withContext(Dispatchers.Default) {
                show(w.checkNode(node, object : NodeCheckObserver {
                    override fun onProgress(report: NodeReport) = show(report)
                }))
            }
        }
    }
    val close = { job?.cancel(); onClose(report?.takeIf { !it.checking && it.usable != NodeUse.NO }?.node) }
    BottomSheet("Add your own node", onDismiss = { close() }) {
        SheetHandle()
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Add your own node", style = display(22), modifier = Modifier.weight(1f).semantics { heading() })
            EdgeIconButton(Icons.close, "Close", { close() })
        }
        // The checks' rows and the keyboard can be more than the screen holds: the buttons stay reachable.
        Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Field(address, { address = it; edited() }, "Address", mono = true, hint = "Your own node, or a public one: host:port, http:// or https://")
        Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp))
                .toggleable(value = selfSigned, role = Role.Checkbox, onValueChange = { selfSigned = it; edited() })
                .padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            val box = RoundedCornerShape(6.dp)
            Box(
                Modifier.size(22.dp).clip(box).background(if (selfSigned) Fob.ink else Color.Transparent).border(2.dp, if (selfSigned) Fob.ink else Fob.outline, box),
                contentAlignment = Alignment.Center,
            ) {
                if (selfSigned) StrokeIcon(Icons.check, tint = Fob.paper, size = 14.dp, width = 3.4f)
            }
            Text("Accept self-signed certificates", fontSize = 15.sp)
        }
        report?.let { r ->
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card).padding(horizontal = 16.dp, vertical = 4.dp)) {
                CheckRows(r.rows, minHeight = 44.dp, lines = false)
            }
        }
        val r = report
        if (r == null) {
            PrimaryButton("Check", ::check, enabled = address.isNotBlank(), height = 52.dp)
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlineButton(if (r.checking) "Checking…" else "Check again", ::check, Modifier.weight(1f), enabled = !r.checking, height = 52.dp)
                PrimaryButton(
                    if (r.usable == NodeUse.USE_ANYWAY) "Use it anyway" else "Use", { job?.cancel(); onUse(r.node) }, Modifier.weight(1f),
                    enabled = r.usable != NodeUse.NO, height = 52.dp,
                )
            }
        }
        }
    }
}
