package com.monokulo.fob.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.semantics.Role
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
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.launch
import uniffi.fob.ReceiveCode
import uniffi.fob.RequestDetail
import uniffi.fob.paymentQrCode

/**
 * A request (Round 5 › "A request: no edits, can be deleted"): the amount
 * asked, of whom and what for, when; what its address received; the
 * address; each payment to it; Show the code again; Delete request, asked
 * first. Rust writes every word. Nothing on it can be changed.
 */
@Composable
fun RequestScreen(model: WalletModel, nav: Nav, id: ULong) {
    val wallet by model.wallet.collectAsState()
    val activity by model.activity.collectAsState()
    val scope = rememberCoroutineScope()
    var detail by remember { mutableStateOf<RequestDetail?>(null) }
    var showing by remember { mutableStateOf<ReceiveCode?>(null) }
    var confirming by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(wallet, activity) {
        if (wallet == null) return@LaunchedEffect
        val shown = model.request(id)
        // Deleted: nothing to show.
        if (shown == null && nav.current == Screen.Request(id)) nav.back() else detail = shown
    }
    val d = detail ?: return Page { TopBar("Request", onBack = { nav.back() }) }
    showing?.let { code ->
        val qr = remember(code) { runCatching { paymentQrCode(code.uri) }.getOrNull() }
        if (qr != null) { ExpandedQr(code, qr, d.address) { showing = null }; return }
    }
    Box(Modifier.fillMaxSize()) {
        Page(bottom = {
            ErrorText(error)
            OutlineButton("Delete request", { confirming = true }, Modifier.fillMaxWidth(), height = 52.dp, tint = Fob.red, edge = Fob.red)
        }) {
            TopBar("Request", onBack = { nav.back() })
            Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Column(Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 8.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(d.headline, style = display(40), textAlign = TextAlign.Center)
                    Text(d.askedLine, color = Fob.muted, fontSize = 15.sp, textAlign = TextAlign.Center)
                    Text(d.`when`, color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.Center)
                }
                Card(padding = PaddingValues(horizontal = 20.dp, vertical = 6.dp)) {
                    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically) {
                        Text(d.receivedLabel, color = Fob.muted, fontSize = 15.sp, modifier = Modifier.weight(1f))
                        Text(d.received, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = if (d.receivedAny) Fob.ink else Fob.muted)
                    }
                    Divider()
                    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(d.addressLabel, color = Fob.muted, fontSize = 15.sp)
                            Text(d.addressShort, fontFamily = FontFamily.Monospace, fontSize = 13.5.sp, maxLines = 1)
                        }
                        AddressPattern(d.address, 30.dp)
                    }
                }
                Card(padding = PaddingValues(vertical = 6.dp)) {
                    Text("Payments to it", fontSize = 14.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 2.dp).semantics { heading() })
                    d.paymentsEmpty?.let { Text(it, color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp)) }
                    d.payments.forEach { row -> PaymentRow(row) { open(nav, row) } }
                }
                OutlineButton("Show the code again", {
                    scope.launch { showing = wallet?.requestCode(id) }
                }, Modifier.fillMaxWidth(), height = 52.dp)
            }
        }
        if (confirming) {
            BottomSheet(d.deleteTitle, onDismiss = { confirming = false }, background = Fob.card) {
                Column(Modifier.padding(start = 4.dp, end = 4.dp, top = 14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(d.deleteTitle, style = display(22), modifier = Modifier.semantics { heading() })
                    Text(d.deleteBody, color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp)
                    Box(
                        Modifier.fillMaxWidth().height(52.dp).clip(RoundedCornerShape(26.dp)).background(Fob.red).clickable(role = Role.Button) {
                            confirming = false
                            scope.launch {
                                runCatching { model.deleteRequest(id) }
                                    .onSuccess { if (nav.current == Screen.Request(id)) nav.back() }
                                    .onFailure { error = it.reason() }
                            }
                        },
                        contentAlignment = Alignment.Center,
                    ) { Text("Delete", color = Fob.onRed, fontSize = 16.sp, fontWeight = FontWeight.SemiBold) }
                    OutlineButton("Keep", { confirming = false }, Modifier.fillMaxWidth(), height = 52.dp)
                }
            }
        }
    }
}
