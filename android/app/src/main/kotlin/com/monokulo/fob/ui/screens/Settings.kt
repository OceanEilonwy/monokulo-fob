package com.monokulo.fob.ui.screens

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.semantics.Role
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
import com.monokulo.fob.ui.NoScreenshots
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.SectionLabel
import com.monokulo.fob.ui.SettingRow
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.ThemeChoice
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.full
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.launch
import uniffi.fob.Activity
import uniffi.fob.Network
import uniffi.fob.NodeCheck
import uniffi.fob.checkNodes
import uniffi.fob.defaultNodes
import uniffi.fob.qrCode
import java.text.DateFormat
import java.util.Date

@Composable
fun ReceiptScreen(model: WalletModel, nav: Nav, activity: Activity) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val receipt = activity.receipt
    Page {
        TopBar(if (receipt != null) "Receipt" else if (activity.outgoing) "Sent" else "Received", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (receipt != null) {
                Card(padding = PaddingValues(vertical = 18.dp)) {
                    Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(receipt.merchant, style = display(24))
                        Text(receipt.issued.replace('T', ' ').take(16), color = Fob.muted, fontSize = 14.sp)
                    }
                    Divider(); ReceiptLines(receipt); Divider()
                    Row(Modifier.padding(horizontal = 20.dp, vertical = 10.dp), verticalAlignment = Alignment.Bottom) {
                        Text("Total", fontWeight = FontWeight.SemiBold, fontSize = 15.sp, modifier = Modifier.weight(1f))
                        Text(receipt.total, style = display(22))
                    }
                    Row(Modifier.padding(horizontal = 20.dp)) {
                        Text("Paid in Monero", color = Fob.muted, fontSize = 15.sp, modifier = Modifier.weight(1f))
                        Text("${activity.amountText} XMR", color = Fob.muted, fontSize = 15.sp)
                    }
                }
            } else {
                Column(Modifier.fillMaxWidth().padding(vertical = 12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text((if (activity.outgoing) "−" else "+") + activity.amountText + " XMR", style = display(40), color = if (activity.outgoing) Fob.ink else Fob.green)
                    Text(DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(activity.time.toLong() * 1000)), color = Fob.muted, fontSize = 15.sp)
                }
            }
            Card {
                InfoRow("Transaction", activity.tx.take(4) + "…" + activity.tx.takeLast(4), mono = true)
                Divider()
                InfoRow("Status", when {
                    activity.pending -> if (activity.outgoing) "Sending" else "On the way"
                    activity.settled -> "Confirmed · ${activity.confirmations}"
                    else -> "Confirming · ${activity.confirmations} of 10"
                })
                activity.feeText?.let { Divider(); InfoRow("Network fee", "$it XMR") }
                activity.to?.let { Divider(); InfoRow("To", it.take(8) + "…" + it.takeLast(6), mono = true) }
            }
            if (receipt != null) Text("Open Receipt Format, from the shop's payment code. Kept on this phone with the payment.", color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 8.dp))
            OutlineButton("Copy transaction ID", {
                scope.launch { clipboard.setClipEntry(ClipData.newPlainText("Transaction", activity.tx).toClipEntry()) }
            }, Modifier.fillMaxWidth())
        }
    }
}

@Composable
fun SettingsScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val theme by model.theme.collectAsState()
    var node by remember { mutableStateOf("") }
    var forgetting by remember { mutableStateOf(false) }
    var switching by remember { mutableStateOf(false) }
    LaunchedEffect(wallet) { node = wallet?.node() ?: "" }
    val canSpend = wallet?.canSpend() != false
    Page {
        TopBar("Settings", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Column {
                SectionLabel("Wallet")
                Card {
                    if (canSpend) SettingRow("Back up seed phrase", "Show the words that restore this wallet", onClick = { nav.go(Screen.ShowSeed) })
                    SettingRow("Show wallet code", "Restore this wallet on another phone", onClick = { nav.go(Screen.ShowWalletCode) })
                }
            }
            Column {
                SectionLabel("Syncing")
                Card {
                    SettingRow("Node", node, onClick = { nav.go(Screen.Node) })
                }
            }
            Column {
                SectionLabel("Network")
                Card {
                    SettingRow(
                        "Use mainnet", if (network == Network.MAINNET) "Real money. Each network has its own wallet." else "Off: stagenet, with test coins",
                        checked = network == Network.MAINNET, onChecked = { switching = true },
                    )
                }
            }
            Column {
                SectionLabel("Theme")
                Card {
                    ThemeChoice.entries.forEachIndexed { i, choice ->
                        if (i > 0) Divider()
                        Choice(choice.label, choice == theme) { model.setTheme(choice) }
                    }
                }
            }
            Column {
                SectionLabel("Receipts")
                Card {
                    SettingRow("Kept with each payment", "Receipts from payment codes that carry one, on this phone only.")
                    SettingRow("Google Wallet", "Not yet: adding receipts needs Fob's pass issuer, which isn't set up.")
                }
            }
            Column {
                SectionLabel("This phone")
                Card { SettingRow("Remove this wallet", "Its seed phrase restores it", titleColor = Fob.red, onClick = { forgetting = true }) }
            }
            Text("Fob 0.1.0", color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
        }
    }
    if (switching) {
        val target = if (network == Network.MAINNET) Network.STAGENET else Network.MAINNET
        AlertDialog(
            onDismissRequest = { switching = false },
            title = { Text(if (target == Network.MAINNET) "Use mainnet?" else "Back to stagenet?") },
            text = { Text(if (target == Network.MAINNET) "Mainnet is real money. Fob is new: start with amounts you could lose. Your stagenet wallet stays here." else "Your mainnet wallet stays on this phone.") },
            confirmButton = { TextButton("Switch", { switching = false; model.switchNetwork(target); nav.reset(if (model.hasWallet(target)) Screen.Home else Screen.Welcome) }) },
            dismissButton = { TextButton("Cancel", { switching = false }, color = Fob.ink) },
        )
    }
    if (forgetting) {
        AlertDialog(
            onDismissRequest = { forgetting = false },
            title = { Text("Remove this wallet?") },
            text = { Text("It's deleted from this phone. Only its seed phrase or wallet code can bring it back, so check you have one first.") },
            confirmButton = { TextButton("Remove", { forgetting = false; model.forget(); nav.reset(Screen.Welcome) }, color = Fob.red) },
            dismissButton = { TextButton("Cancel", { forgetting = false }, color = Fob.ink) },
        )
    }
}

/** One of a few choices: its name, and a tick when it's the one chosen. */
@Composable
private fun Choice(title: String, chosen: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).selectable(chosen, role = Role.RadioButton, onClick = onClick).padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, fontSize = 16.sp, modifier = Modifier.weight(1f))
        if (chosen) StrokeIcon(Icons.check, size = 20.dp)
    }
}

@Composable
fun NodeScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val status by model.status.collectAsState()
    val scope = rememberCoroutineScope()
    var node by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var testing by remember { mutableStateOf(false) }
    var checks by remember { mutableStateOf<List<NodeCheck>>(emptyList()) }
    LaunchedEffect(wallet) { node = wallet?.node() ?: "" }
    Page(bottom = {
        ErrorText(error)
        PrimaryButton("Use this node", enabled = node.isNotBlank(), onClick = {
            scope.launch { runCatching { wallet?.setNode(node) }.onSuccess { nav.back() }.onFailure { error = it.reason() } }
        })
    }) {
        TopBar("Node", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            status?.let { s ->
                Card {
                    InfoRow("Status", when { s.error != null && !s.synced -> "Can't reach it"; s.synced -> "Synced"; else -> "Catching up · ${(s.progress * 100).toInt()}%" })
                    Divider()
                    InfoRow("Scanned", "${s.scanned} of ${s.tip}")
                    s.error?.let { Divider(); Text(it, color = Fob.red, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp)) }
                }
            }
            Field(node, { node = it }, "Node address", hint = "Your own node, or a public one: http://host:port or https://…")
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { SectionLabel("Nodes") }
                TextButton(if (testing) "Testing…" else "Test speed", {
                    if (!testing) {
                        testing = true
                        scope.launch {
                            val urls = (listOf(node.trim()).filter { it.isNotEmpty() } + defaultNodes(network)).distinct()
                            checks = checkNodes(network, urls)
                            testing = false
                        }
                    }
                })
            }
            Card {
                val urls = (defaultNodes(network) + checks.map { it.url }).distinct()
                val fastest = checks.filter { it.rightNetwork && it.error == null }.maxByOrNull { it.blocksPerSecond }?.url
                urls.forEach { url ->
                    val check = checks.firstOrNull { it.url == url }
                    val result = when {
                        check == null -> null
                        !check.reachable -> "Can't reach it"
                        !check.rightNetwork -> "Wrong network"
                        check.error != null -> "${check.latencyMs} ms · ${check.error}"
                        else -> "${check.blocksPerSecond.toInt()} blocks/s · ${check.latencyMs} ms" + (if (!check.synced) " · still syncing" else "") + (if (url == fastest) " · fastest" else "")
                    }
                    SettingRow(url, result, onClick = { node = url })
                }
            }
            Text("Fob asks the node for blocks and sends your payments through it. It never sends your keys.", color = Fob.muted, fontSize = 13.sp)
        }
    }
}

@Composable
fun WalletCodeScreen(model: WalletModel, nav: Nav) {
    NoScreenshots()
    val wallet by model.wallet.collectAsState()
    var code by remember { mutableStateOf<String?>(null) }
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(wallet) { code = wallet?.walletCode() }
    Page(bottom = {
        if (!shown) PrimaryButton("Show the code", { shown = true }) else OutlineButton("Hide", { shown = false }, Modifier.fillMaxWidth())
    }) {
        TopBar("Wallet code", onBack = { nav.back() })
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.highlight).padding(16.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                StrokeIcon(Icons.warning, tint = Fob.orangeText, size = 22.dp, width = 1.8f)
                Text(
                    if (wallet?.canSpend() != false) "This code holds the keys to spend your money. Show it only to restore this wallet, with no one watching."
                    else "This code lets anyone see this wallet's payments.",
                    fontSize = 14.sp, lineHeight = 20.sp,
                )
            }
            val c = code
            if (shown && c != null) {
                runCatching { qrCode(c) }.getOrNull()?.let { qr ->
                    Box(Modifier.clip(RoundedCornerShape(28.dp)).background(Fob.qrLight).padding(16.dp)) { QrImage(qr, 280.dp, description = "Wallet code") }
                }
                Text("Scan it in Fob, the Monero GUI or Cake Wallet.", color = Fob.muted, fontSize = 14.sp)
            }
        }
    }
}
