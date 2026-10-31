package com.monokulo.fob.ui.screens

import android.content.ClipData
import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.InfoRow
import com.monokulo.fob.ui.Mono
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.SwipeToSend
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.full
import com.monokulo.fob.ui.reason
import com.monokulo.fob.ui.short
import kotlinx.coroutines.launch
import uniffi.fob.CodeKind
import uniffi.fob.Prepared
import uniffi.fob.Receipt
import uniffi.fob.classifyCode
import uniffi.fob.isAmount
import uniffi.fob.parsePayment
import uniffi.fob.paymentCode
import uniffi.fob.qrCode

@Composable
fun ReceiveScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    var address by remember { mutableStateOf<String?>(null) }
    var index by remember { mutableStateOf(0u) }
    var amount by remember { mutableStateOf("") }
    LaunchedEffect(wallet) {
        val w = wallet ?: return@LaunchedEffect
        address = w.receiveAddress()
        index = w.receiveIndex()
    }
    val code = address?.let { runCatching { paymentCode(it, amount.ifBlank { null }) }.getOrNull() }
    Page(bottom = {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlineButton("Copy", {
                code?.let { scope.launch { clipboard.setClipEntry(ClipData.newPlainText("Monero address", address).toClipEntry()) } }
            }, Modifier.weight(1f))
            PrimaryButton("Share", {
                code?.let { context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, it), null)) }
            }, Modifier.weight(1f))
        }
    }) {
        TopBar("Receive", onBack = { nav.back() })
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Box(Modifier.clip(RoundedCornerShape(36.dp)).background(Fob.card).padding(20.dp)) {
                val qr = code?.let { runCatching { qrCode(it) }.getOrNull() }
                if (qr != null) QrImage(qr, 260.dp) else Spacer(Modifier.size(260.dp))
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Address $index · a new one each time you ask", color = Fob.muted, fontSize = 13.sp)
                address?.let { Mono(full(it), 14) }
            }
            Field(amount, { amount = it }, "Amount in XMR (optional)", hint = if (amount.isNotBlank() && code == null) "That isn't an amount" else null)
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text("Used this address before?", color = Fob.muted, fontSize = 14.sp, modifier = Modifier.weight(1f))
                TextButton("New address", {
                    scope.launch {
                        val w = wallet ?: return@launch
                        address = w.newReceiveAddress()
                        index = w.receiveIndex()
                    }
                })
            }
        }
    }
}

@Composable
fun ScanPayScreen(model: WalletModel, nav: Nav) {
    val network by model.network.collectAsState()
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var notice by remember { mutableStateOf<String?>(null) }
    fun read(text: String) {
        when (classifyCode(network, text)) {
            CodeKind.PAYMENT -> if (nav.current == Screen.ScanPay) { nav.back(); nav.go(Screen.Send(text)) }
            CodeKind.WALLET -> notice = "That's a wallet code, not a payment code."
            CodeKind.UNKNOWN -> notice = runCatching { parsePayment(network, text) }.exceptionOrNull()?.reason() ?: "That isn't a payment code."
        }
    }
    Box(Modifier.fillMaxSize().background(Fob.camera)) {
        CameraOrAsk(Modifier.fillMaxSize(), ::read)
        Column(Modifier.fillMaxSize().padding(top = 32.dp)) {
            TopBar("Scan to pay", onBack = { nav.back() }, closeIcon = true, tint = Fob.paper) {
                OutlineButton("Paste", {
                    scope.launch {
                        val text = clipboard.getClipEntry()?.clipData?.getItemAt(0)?.text?.toString()
                        if (text != null) read(text) else notice = "Nothing to paste."
                    }
                }, height = 40.dp, tint = Fob.paper)
            }
            Spacer(Modifier.weight(1f))
            Text(
                notice ?: "Point at a Monero payment code", color = Fob.paper, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth().padding(32.dp),
            )
        }
    }
}

/** The receipt's lines, folded into a card. */
@Composable
fun ReceiptLines(receipt: Receipt) {
    Column(Modifier.padding(horizontal = 20.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        receipt.lines.forEach { line ->
            Row { Text(line.label, fontSize = 15.sp, modifier = Modifier.weight(1f)); Text(line.amount, fontSize = 15.sp) }
        }
        receipt.tax?.let { Row { Text("Includes tax", color = Fob.muted, fontSize = 15.sp, modifier = Modifier.weight(1f)); Text(it, color = Fob.muted, fontSize = 15.sp) } }
    }
}

@Composable
fun SendScreen(model: WalletModel, nav: Nav, code: String?) {
    val network by model.network.collectAsState()
    val balance by model.balance.collectAsState()
    val scope = rememberCoroutineScope()
    val request = remember(code) { code?.let { runCatching { parsePayment(network, it) } } }
    var to by remember { mutableStateOf(request?.getOrNull()?.address ?: "") }
    var amount by remember { mutableStateOf(request?.getOrNull()?.amountText ?: "") }
    var error by remember { mutableStateOf(request?.exceptionOrNull()?.reason()) }
    var busy by remember { mutableStateOf(false) }
    val r = request?.getOrNull()
    val fixed = r?.amount != null
    Page(bottom = {
        ErrorText(error)
        PrimaryButton(if (busy) "Getting the fee…" else "Review payment", enabled = !busy && to.isNotBlank() && isAmount(amount), onClick = {
            busy = true
            error = null
            scope.launch {
                runCatching { model.prepare(to.trim(), amount, r?.receiptJson) }
                    .onSuccess { nav.back(); nav.go(Screen.Confirm(it)) }
                    .onFailure { error = it.reason() }
                busy = false
            }
        })
    }) {
        TopBar(r?.receipt?.merchant?.let { "Pay $it" } ?: r?.recipientName?.let { "Pay $it" } ?: "Send", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (r?.receipt != null) {
                Card {
                    Row(Modifier.padding(horizontal = 20.dp, vertical = 14.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        StrokeIcon(Icons.receipt, tint = Fob.orangeText, width = 1.8f)
                        Column { Text(r.receipt!!.merchant, fontSize = 17.sp, fontWeight = FontWeight.SemiBold); Text("Receipt included · ${r.receipt!!.itemCount} items · ${r.receipt!!.total}", color = Fob.muted, fontSize = 14.sp) }
                    }
                    Divider()
                    ReceiptLines(r.receipt!!)
                }
            }
            r?.description?.let { Text(it, color = Fob.muted, fontSize = 15.sp) }
            Field(to, { to = it }, "To", mono = true, lines = 2)
            Field(amount, { if (!fixed) amount = it }, "Amount in XMR", hint = when {
                fixed -> "Set by the payment code"
                else -> balance?.let { "${it.unlockedText} XMR ready to send" }
            })
        }
    }
}

@Composable
fun ConfirmScreen(model: WalletModel, nav: Nav, prepared: Prepared, confirmIdentity: suspend (String) -> Boolean) {
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val title = prepared.receipt?.merchant?.let { "Pay $it" } ?: "Send"
    Page(bottom = {
        ErrorText(error)
        SwipeToSend(if (busy) "Sending…" else "Swipe to send ${prepared.amountText} XMR", enabled = !busy) {
            busy = true
            error = null
            scope.launch {
                if (!confirmIdentity(title)) {
                    error = "Not sent."
                } else {
                    runCatching { model.send(prepared) }
                        .onSuccess { tx -> nav.back(); nav.go(Screen.Paid(prepared, tx)) }
                        .onFailure { error = it.reason() }
                }
                busy = false
            }
        }
        Text("Then confirm it's you", color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
    }) {
        TopBar(title, onBack = { scope.launch { model.wallet.value?.cancel(prepared.id) }; nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Column(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text(prepared.receipt?.total ?: "${prepared.amountText} XMR", style = display(52))
                if (prepared.receipt != null) Text("${prepared.amountText} XMR", color = Fob.muted, fontSize = 18.sp)
            }
            Card {
                InfoRow("To", short(prepared.to, 2), mono = true)
                Divider()
                InfoRow("Network fee", "${prepared.feeText} XMR")
                Divider()
                InfoRow("Total", "${prepared.totalText} XMR", bold = true)
            }
            prepared.receipt?.let { Card { Text("Receipt · ${it.itemCount} items", fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(start = 20.dp, top = 14.dp)); ReceiptLines(it) } }
        }
    }
}

@Composable
fun PaidScreen(nav: Nav, prepared: Prepared) {
    Page(bottom = { PrimaryButton("Done", { nav.reset(Screen.Home) }) }) {
        Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 72.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Box(Modifier.size(96.dp).clip(CircleShape).background(Fob.greenFill), contentAlignment = Alignment.Center) { StrokeIcon(Icons.check, size = 52.dp, width = 2f) }
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Paid", style = display(34))
                Text(prepared.receipt?.let { "${it.total} to ${it.merchant}" } ?: "${prepared.amountText} XMR to ${short(prepared.to, 1)}", fontSize = 18.sp, textAlign = TextAlign.Center)
                Text("Sent · confirming on the network", color = Fob.muted, fontSize = 14.sp)
            }
            prepared.receipt?.let {
                Card(Modifier.padding(top = 12.dp), padding = PaddingValues(vertical = 12.dp)) {
                    Row(Modifier.padding(horizontal = 20.dp), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                        StrokeIcon(Icons.receipt, tint = Fob.orangeText, size = 20.dp, width = 1.8f)
                        Text("Receipt saved", fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                    }
                    ReceiptLines(it)
                }
            }
        }
    }
}
