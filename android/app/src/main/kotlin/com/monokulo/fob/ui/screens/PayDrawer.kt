package com.monokulo.fob.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.AddressPattern
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.InfoRow
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.SwipeToSend
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.Warning
import com.monokulo.fob.ui.reason
import com.monokulo.fob.ui.short
import kotlinx.coroutines.launch
import uniffi.fob.PaymentPreview
import uniffi.fob.Prepared
import uniffi.fob.parsePayment

/**
 * Touch to pay from Home: a phone on Receive was touched, and this is
 * Confirm condensed into a drawer over Home: what is asked and from whom,
 * the fee and total (waiting bars until the fee is in), the address's
 * pattern, and the same bar as Confirm, which reads "Getting the fee…" and
 * becomes the swipe to send. A code with no amount opens Send to fill it
 * in. Closing before sending forgets the prepared payment.
 */
@Composable
fun PayDrawer(model: WalletModel, nav: Nav, code: String, confirmIdentity: suspend (String) -> Boolean, onClose: () -> Unit) {
    val network by model.network.collectAsState()
    val scope = rememberCoroutineScope()
    val request = remember(code, network) { runCatching { parsePayment(network, code) }.getOrNull() }
    var preview by remember { mutableStateOf<PaymentPreview?>(null) }
    var prepared by remember { mutableStateOf<Prepared?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val sending by rememberUpdatedState(busy)
    val amount = request?.amountText

    // The node is asked what the payment needs as the drawer opens; the fee below takes the answer, whichever is first.
    LaunchedEffect(code) { model.wallet.value?.warmUpSend() }
    // Checked without asking the node, so the drawer opens at once; no amount means the form takes it.
    LaunchedEffect(code) {
        val r = request
        if (r == null) { onClose(); return@LaunchedEffect }
        if (amount == null) { onClose(); nav.go(Screen.Send(code)); return@LaunchedEffect }
        runCatching { model.preview(r.address, amount, false, code) }.onSuccess { preview = it }.onFailure { error = it.reason() }
    }
    // Then the fee comes in on it.
    LaunchedEffect(preview) {
        val r = request ?: return@LaunchedEffect
        if (preview == null || amount == null) return@LaunchedEffect
        runCatching { model.prepare(r.address, amount, false, r.description, code) }.onSuccess { prepared = it }.onFailure { error = it.reason() }
    }
    DisposableEffect(prepared?.id) {
        val p = prepared
        onDispose { if (p != null && !sending) model.cancelLater(p) }
    }

    val title = request?.recipientName?.let { "Pay $it" } ?: "Send"
    var failed by remember { mutableStateOf(false) }
    var refused by remember { mutableStateOf(false) }
    var checked by remember { mutableStateOf(false) }
    val trezor = model.wallet.value?.isTrezor() == true
    fun swiped() {
        val ready = prepared ?: return
        busy = true
        error = null
        scope.launch {
            if (trezor) {
                runCatching { model.sendWithTrezor(ready) }
                    .onSuccess { tx -> onClose(); nav.go(Screen.Paid(ready, tx)) }
                    // Said no on the device, or it couldn't be reached: still prepared for another swipe.
                    .onFailure {
                        error = it.reason()
                        failed = !(it is uniffi.fob.FobException.TrezorCancelled || it is uniffi.fob.FobException.Trezor)
                        refused = it is uniffi.fob.FobException.SpendRejected
                    }
            } else if (!confirmIdentity(title)) {
                error = "Not sent."
            } else {
                runCatching { model.send(ready) }
                    .onSuccess { tx -> onClose(); nav.go(Screen.Paid(ready, tx)) }
                    // Trying used up the prepared payment: another touch makes another.
                    .onFailure { error = it.reason(); failed = true; refused = it is uniffi.fob.FobException.SpendRejected }
            }
            busy = false
        }
    }

    BottomSheet(title, onDismiss = { if (!busy) onClose() }) {
        SheetHandle()
        val receipt = prepared?.receipt ?: preview?.receipt
        val amountText = prepared?.amountText ?: preview?.amountText ?: amount ?: ""
        val fiat = prepared?.amountFiat ?: preview?.amountFiat
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontSize = 17.sp, fontWeight = FontWeight.Medium, modifier = Modifier.semantics { heading() })
            // The XMR sent and Fob's own valuation first; the receipt's total is only what the code claims.
            Text("$amountText XMR", style = display(44), modifier = Modifier.padding(top = 4.dp))
            fiat?.let { Text(it, color = Fob.muted, fontSize = 16.sp) }
            receipt?.let { Text("Receipt says ${it.total}", color = Fob.muted, fontSize = 14.sp) }
            request?.description?.let { Text(it, color = Fob.muted, fontSize = 14.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(top = 4.dp)) }
        }
        val to = prepared?.to ?: preview?.to ?: request?.address ?: ""
        Card {
            // Who it goes to, with the address's pattern beside it, so it can be compared with their screen.
            Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("To", color = Fob.muted, fontSize = 15.sp, modifier = Modifier.weight(1f))
                AddressPattern(to, 28.dp)
                Text(short(to, 2), fontFamily = FontFamily.Monospace, fontSize = 15.sp, modifier = Modifier.padding(start = 10.dp))
            }
            Divider()
            InfoRow("Network fee", prepared?.let { "${it.feeText} XMR" + (it.feeFiat?.let { f -> " · $f" } ?: "") })
            Divider()
            InfoRow("Total", prepared?.let { "${it.totalText} XMR" + (it.totalFiat?.let { f -> " · $f" } ?: "") }, bold = true)
        }
        // A total Fob's price disagrees with, or has no price to check, must be
        // looked at before the swipe works.
        val mismatch = receipt?.mismatch
        val notice = mismatch ?: receipt?.unchecked
        notice?.let { Warning(it, title = if (mismatch != null) "The receipt doesn't match the amount" else "Check the amount yourself") }
        if (notice != null && !checked) OutlineButton("I've checked the XMR amount", { checked = true }, Modifier.fillMaxWidth())
        if (refused) Warning("A coin in it may already be spent. Tap to sync, then try again.", title = "Your node refused this payment", onClick = { onClose(); model.openScan() })
        else ErrorText(error)
        val label = when {
            busy -> if (trezor) "Confirm on the Trezor…" else "Sending…"
            failed -> "Not sent"
            prepared == null -> if (error == null) "Getting the fee…" else "Can't send"
            trezor -> "Swipe, then confirm on the Trezor"
            else -> "Swipe to send ${prepared!!.amountText} XMR"
        }
        SwipeToSend(label, enabled = !busy && !failed && prepared != null && (notice == null || checked), sending = busy, waiting = prepared == null && error == null) { swiped() }
    }
}
