package com.monokulo.fob.ui.screens

import android.content.ClipData
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.InfoRow
import com.monokulo.fob.ui.Manrope
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.ProgressBar
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import kotlinx.coroutines.launch
import uniffi.fob.Activity
import uniffi.fob.PaymentDetail

/** A payment's words from Rust, asked again whenever Activity or the price changes. */
@Composable
fun rememberPayment(model: WalletModel, tx: String): PaymentDetail? {
    val wallet by model.wallet.collectAsState()
    val activity by model.activity.collectAsState()
    val balance by model.balance.collectAsState()
    var detail by remember { mutableStateOf<PaymentDetail?>(null) }
    LaunchedEffect(wallet, activity, balance) { detail = model.payment(tx) }
    return detail
}

/** A payment in or out without a receipt: its amount, what it's worth now and was then, and its details. */
@Composable
fun PaymentScreen(model: WalletModel, nav: Nav, activity: Activity) {
    val detail = rememberPayment(model, activity.tx)
    Page {
        TopBar(if (activity.outgoing) "Sent" else "Received", onBack = { nav.back() })
        val d = detail ?: return@Page
        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Column(Modifier.fillMaxWidth().padding(vertical = 12.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(d.amountText, style = display(44), color = if (activity.outgoing) Fob.ink else Fob.green)
                    Text("XMR", fontFamily = Manrope, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = Fob.muted, modifier = Modifier.padding(bottom = 8.dp))
                }
                Text(d.`when`, color = Fob.muted, fontSize = 15.sp, textAlign = TextAlign.Center)
            }
            if (d.priced) PriceBoxes(d)
            PaymentDetails(d)
            d.footer?.let { Text(it, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 8.dp)) }
        }
    }
}

/** "Worth now" and "When received" side by side, and how the value has moved since. */
@Composable
fun PriceBoxes(d: PaymentDetail) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            PriceBox("Worth now", d.worthNow, d.rateNow, Modifier.weight(1f))
            PriceBox(d.thenLabel, d.worthThen, d.rateThen, Modifier.weight(1f))
        }
        d.change?.let { Text(it, color = if (d.up) Fob.green else Fob.muted, fontSize = 14.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(horizontal = 8.dp)) }
    }
}

@Composable
private fun PriceBox(label: String, value: String, rate: String?, modifier: Modifier) {
    Card(modifier, padding = PaddingValues(horizontal = 16.dp, vertical = 14.dp)) {
        Text(label, color = Fob.muted, fontSize = 13.sp)
        Text(value, style = display(if (rate != null) 22 else 17), color = if (rate != null) Fob.ink else Fob.muted, modifier = Modifier.padding(top = 4.dp))
        rate?.let { Text(it, color = Fob.muted, fontSize = 12.sp) }
    }
}

/** Status (with a thin bar while not ready), who, pieces, fee, and the transaction id with Copy. */
@Composable
fun PaymentDetails(d: PaymentDetail) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    Card {
        InfoRow("Status", d.status)
        d.progress?.let { Column(Modifier.padding(start = 20.dp, end = 20.dp, bottom = 12.dp)) { ProgressBar(it, height = 4.dp, fill = Fob.green) } }
        d.party?.let { Divider(); InfoRow(d.partyLabel, it) }
        d.pieces?.let { Divider(); InfoRow(d.piecesLabel, it) }
        d.fee?.let { Divider(); InfoRow("Network fee", it) }
        Divider()
        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = 20.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Transaction", color = Fob.muted, fontSize = 15.sp, modifier = Modifier.weight(1f))
            Text(d.txShort, fontSize = 14.sp, fontFamily = FontFamily.Monospace)
            TextButton("Copy", { scope.launch { clipboard.setClipEntry(ClipData.newPlainText("Transaction", d.tx).toClipEntry()) } })
        }
    }
}
