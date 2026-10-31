package com.monokulo.fob.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.fob.SplitBlock
import uniffi.fob.SplitTile
import uniffi.fob.SplitView

/**
 * Split up: what's ready to spend paid back to this wallet as pieces, so
 * after a payment there's still money ready. Rust picks the notes, the
 * suggestion, what fits and every word; this draws them and passes taps on.
 */
@Composable
fun SplitScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val balance by model.balance.collectAsState()
    val prices by model.prices.collectAsState()
    val scope = rememberCoroutineScope()
    var counts by remember { mutableStateOf<List<UInt>>(emptyList()) }
    var tip by remember { mutableStateOf<String?>(null) }
    var view by remember { mutableStateOf<SplitView?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    // Bumped when the node's fee rate arrives, so the estimate is asked again.
    var feeTick by remember { mutableIntStateOf(0) }
    LaunchedEffect(wallet) {
        val w = wallet ?: return@LaunchedEffect
        val suggestion = w.splitSuggestion()
        counts = suggestion.counts
        tip = suggestion.text
        w.refreshFeeRate()
        feeTick++
    }
    LaunchedEffect(wallet, counts, balance, prices, feeTick) {
        view = wallet?.splitView(counts)
    }
    // Why a piece couldn't be added shows for a moment.
    LaunchedEffect(error) {
        if (error != null) {
            delay(3_000)
            error = null
        }
    }
    fun change(edit: suspend () -> List<UInt>) {
        scope.launch {
            runCatching { edit() }.onSuccess { counts = it; error = null }.onFailure { error = it.reason() }
        }
    }
    val v = view
    Page(bottom = {
        ErrorText(error)
        PrimaryButton(if (busy) "Getting the fee…" else v?.button ?: "", enabled = !busy && v?.ready == true, onClick = {
            busy = true
            error = null
            scope.launch {
                runCatching { model.prepareSplit(counts) }
                    .onSuccess { nav.back(); nav.go(Screen.Confirm(it)) }
                    .onFailure { error = it.reason() }
                busy = false
            }
        })
        v?.footer?.let { Text(it, color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth()) }
    }) {
        TopBar(v?.title ?: "", onBack = { nav.back() }) {
            TextButton("Clear", { counts = emptyList(); error = null })
        }
        if (v == null) return@Page
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(v.intro, color = Fob.muted, fontSize = 15.sp)
            tip?.let {
                Text(
                    it, fontSize = 14.sp,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(Fob.highlight).padding(horizontal = 14.dp, vertical = 12.dp),
                )
            }
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                v.tiles.withIndex().chunked(3).forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        row.forEach { (i, tile) ->
                            NoteTile(
                                tile, Modifier.weight(1f),
                                onAdd = { change { wallet!!.splitAdd(counts, i.toUInt()) } },
                                onRemove = { change { wallet!!.splitRemove(counts, i.toUInt()) } },
                            )
                        }
                        repeat(3 - row.size) { Spacer(Modifier.weight(1f)) }
                    }
                }
            }
            if (v.pieces > 0u) {
                Card(padding = PaddingValues(horizontal = 16.dp, vertical = 16.dp)) {
                    PiecesBar(v.bar)
                    Spacer(Modifier.height(12.dp))
                    v.summary?.let { Text(it, fontSize = 15.sp) }
                    v.leftOver?.let { Text(it, color = Fob.muted, fontSize = 14.sp) }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/**
 * A note: tapping adds a piece of it; with pieces, a round "−" on its
 * corner takes one away.
 */
@Composable
private fun NoteTile(tile: SplitTile, modifier: Modifier, onAdd: () -> Unit, onRemove: () -> Unit) {
    val shape = RoundedCornerShape(18.dp)
    val chosen = tile.count > 0u
    Box(modifier.padding(top = 10.dp, end = 10.dp)) {
        Column(
            Modifier.fillMaxWidth().height(72.dp).clip(shape)
                .background(if (chosen) Fob.card else Color.Transparent)
                .border(1.dp, if (chosen) Fob.ink else Fob.outline, shape)
                .clickable(role = Role.Button, onClickLabel = "Add") { onAdd() }
                .clearAndSetSemantics { contentDescription = tile.addLabel },
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            // Long notes ("₫5,000,000") shrink to fit the tile.
            BasicText(
                tile.label, style = display(22).copy(color = Fob.ink), maxLines = 1,
                autoSize = TextAutoSize.StepBased(minFontSize = 12.sp, maxFontSize = 22.sp), modifier = Modifier.padding(horizontal = 6.dp),
            )
            Text(tile.countText, color = Fob.muted, fontSize = 13.sp, maxLines = 1)
        }
        if (chosen) {
            Box(
                Modifier.align(Alignment.TopEnd).offset(10.dp, (-10).dp).size(28.dp).clip(CircleShape).background(Fob.ink)
                    .clickable(role = Role.Button) { onRemove() }
                    .semantics { contentDescription = tile.removeLabel },
                contentAlignment = Alignment.Center,
            ) { Text("−", color = Fob.paper, fontSize = 18.sp, lineHeight = 18.sp) }
        }
    }
}

/** The pieces side by side, each as wide as its value and as tall as its square root; what's left over lighter. */
@Composable
private fun PiecesBar(blocks: List<SplitBlock>) {
    Row(
        Modifier.fillMaxWidth().height(44.dp).semantics { contentDescription = "${blocks.count { !it.leftOver }} coins" },
        horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.Bottom,
    ) {
        blocks.forEach { b ->
            Box(
                Modifier.weight(b.width.coerceAtLeast(0.001f)).fillMaxHeight(b.height.coerceIn(0.12f, 1f))
                    .clip(RoundedCornerShape(3.dp)).background(if (b.leftOver) Fob.track else Fob.ink),
            )
        }
    }
}
