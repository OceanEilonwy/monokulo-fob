package com.monokulo.fob.ui.screens

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.TopBar
import kotlinx.coroutines.delay
import uniffi.fob.Coin
import uniffi.fob.Coins
import uniffi.fob.NotReadyPiece

/**
 * Every coin the wallet holds: why there are several, the bar from Home,
 * those ready to spend and those not yet, and Split up at the bottom.
 * Rust wrote every word.
 */
@Composable
fun CoinsScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val balance by model.balance.collectAsState()
    val notReady by model.notReady.collectAsState()
    var coins by remember { mutableStateOf<Coins?>(null) }
    // Again whenever the wallet changes, and each half minute while some coins wait.
    LaunchedEffect(wallet, balance, notReady) {
        while (true) {
            coins = model.coins()
            if (coins?.notReadyCoins.isNullOrEmpty()) break
            delay(30_000)
        }
    }
    val canSpend = wallet?.canSpend() != false
    Page(bottom = if (canSpend) ({ PrimaryButton("Split up", { nav.go(Screen.Split) }, leading = Icons.split, iconWidth = 1.8f) }) else null) {
        TopBar("Coins", onBack = { nav.back() })
        val c = coins ?: return@Page
        Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Column(Modifier.padding(horizontal = 4.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(c.intro, color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp)
                if (c.ready > 0u || c.notReady > 0u) CoinBar(c.ready, c.notReady, c.progress)
            }
            if (c.readyCoins.isNotEmpty()) {
                CoinCard(c.readyTitle, c.readySum) { c.readyCoins.forEach { ReadyCoinRow(it) } }
            }
            if (c.notReadyCoins.isNotEmpty()) {
                CoinCard(c.notReadyTitle, c.notReadySum) { c.notReadyCoins.forEach { WaitingCoinRow(it) } }
            }
        }
    }
}

/** A card of coins under its title and sum: "Ready · 5 coins  1.8592 · $308.70". */
@Composable
private fun CoinCard(title: String, sum: String, rows: @Composable () -> Unit) {
    Card(padding = PaddingValues(vertical = 4.dp)) {
        Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 10.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(title, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f).semantics { heading() })
            Text(sum, color = Fob.muted, fontSize = 14.sp)
        }
        rows()
    }
}

/** A coin ready to spend: an ink dot, its amount, where it came from, its worth. */
@Composable
private fun ReadyCoinRow(c: Coin) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics(mergeDescendants = true) {}.padding(horizontal = 20.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(12.dp).clip(CircleShape).background(Fob.ink))
        Column(Modifier.weight(1f)) {
            Text(c.amountText, fontSize = 15.sp, fontWeight = FontWeight.Medium)
            Text(c.source, color = Fob.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        c.fiatText?.let { Text(it, color = Fob.muted, fontSize = 14.sp) }
    }
}

/** A coin not ready yet: its confirmations in a ring, its amount and source, when it will be ready. */
@Composable
private fun WaitingCoinRow(p: NotReadyPiece) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics(mergeDescendants = true) {}.padding(start = 14.dp, end = 20.dp, top = 2.dp, bottom = 2.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        ConfirmationRing(p.confirmations, p.needed, p.progress)
        Column(Modifier.weight(1f)) {
            Text(p.amountText + (p.fiatText?.let { " · $it" } ?: ""), fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(p.source, color = Fob.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(p.ready, fontSize = 14.sp)
            Text(p.confirmationsText, color = Fob.muted, fontSize = 12.sp)
        }
    }
}

/** A 24dp ring: green as far as the confirmations go. */
@Composable
private fun ConfirmationRing(confirmations: ULong, needed: ULong, progress: Float) {
    val (track, arc) = Fob.track to Fob.green
    Canvas(
        Modifier.size(24.dp).semantics {
            contentDescription = "$confirmations of $needed confirmations"
            progressBarRangeInfo = ProgressBarRangeInfo(confirmations.toFloat(), 0f..needed.toFloat())
        },
    ) {
        val width = 3.dp.toPx()
        val radius = 9.dp.toPx()
        drawCircle(track, radius = radius, style = Stroke(width))
        if (progress > 0f) {
            drawArc(
                arc, -90f, 360f * progress.coerceIn(0f, 1f), false,
                topLeft = Offset(center.x - radius, center.y - radius),
                size = Size(radius * 2, radius * 2),
                style = Stroke(width, cap = StrokeCap.Round),
            )
        }
    }
}
