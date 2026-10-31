package com.monokulo.fob.ui.screens

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
import androidx.compose.ui.semantics.Role
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
import com.monokulo.fob.ui.NoScreenshots
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.SectionLabel
import com.monokulo.fob.ui.Segmented
import com.monokulo.fob.ui.SettingRow
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.ThemeChoice
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.launch
import uniffi.fob.Activity
import uniffi.fob.Network
import uniffi.fob.NodeCheck
import uniffi.fob.ZmqStatus
import uniffi.fob.checkNodes
import uniffi.fob.defaultNodes
import uniffi.fob.qrCode

/** A payment that came with a receipt: the receipt, then what it was worth and its details, as for any payment. */
@Composable
fun ReceiptScreen(model: WalletModel, nav: Nav, activity: Activity) {
    val receipt = activity.receipt
    val detail = rememberPayment(model, activity.tx)
    Page {
        TopBar("Receipt", onBack = { nav.back() })
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
            }
            detail?.let { d ->
                if (d.priced) PriceBoxes(d)
                PaymentDetails(d)
            }
            Text("Open Receipt Format, from the shop's payment code. Kept on this phone with the payment.", color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 8.dp))
            detail?.footer?.let { Text(it, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 8.dp)) }
        }
    }
}

@Composable
fun SettingsScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val theme by model.theme.collectAsState()
    val status by model.status.collectAsState()
    val scope = rememberCoroutineScope()
    var node by remember { mutableStateOf("") }
    var forgetting by remember { mutableStateOf(false) }
    var switching by remember { mutableStateOf(false) }
    var choosing by remember { mutableStateOf<Pick?>(null) }
    var priceError by remember { mutableStateOf<String?>(null) }
    val prices by model.prices.collectAsState()
    LaunchedEffect(wallet) { node = wallet?.node() ?: "" }
    val canSpend = wallet?.canSpend() != false
    Page {
        TopBar("Settings", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Column {
                SectionLabel("Wallet")
                Card {
                    SettingRow("Contacts", "Who you pay, kept on this phone", onClick = { nav.go(Screen.Contacts) })
                    if (canSpend) SettingRow("Back up seed phrase", "Show the words that restore this wallet", onClick = { nav.go(Screen.ShowSeed) })
                    SettingRow("Show wallet code", "Restore this wallet on another phone", onClick = { nav.go(Screen.ShowWalletCode) })
                }
            }
            Column {
                SectionLabel("Syncing")
                Card {
                    SettingRow("Node", node, onClick = { nav.go(Screen.Node) })
                    val zmqOn = status?.zmq?.let { it !is ZmqStatus.Off } ?: false
                    SettingRow(
                        "Instant updates (ZMQ)", status?.zmqText ?: "Off", onClick = { nav.go(Screen.Zmq) },
                        // Turning on with nothing Rust accepts opens the address to fix.
                        checked = zmqOn, onChecked = { on -> scope.launch { runCatching { model.setZmq(on) }.onFailure { nav.go(Screen.Zmq) } } },
                    )
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
                SectionLabel("Display")
                Card {
                    ThemeChoice.entries.forEachIndexed { i, choice ->
                        if (i > 0) Divider()
                        Choice(choice.label, choice == theme) { model.setTheme(choice) }
                    }
                    prices?.let { p ->
                        Divider()
                        val name = p.currencies.firstOrNull { it.code == p.currency }?.name
                        SettingRow("Currency", listOfNotNull(name, p.currency).joinToString(" · "), onClick = { choosing = Pick.CURRENCY })
                        Divider()
                        Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 20.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text("Show amounts in", fontSize = 16.sp, modifier = Modifier.weight(1f))
                            Segmented(listOf("XMR", p.currency), if (p.fiatFirst) 1 else 0, { i -> scope.launch { model.setFiatFirst(i == 1) } })
                        }
                    }
                }
            }
            prices?.let { p ->
                Column {
                    SectionLabel("Prices")
                    Card {
                        SettingRow("Show prices", "Off: Fob never asks for a price", checked = p.on, onChecked = { on -> scope.launch { model.setPricesOn(on) } })
                        Divider()
                        SettingRow("Price source", listOfNotNull(p.sourceText, p.error).joinToString(" · "), onClick = { choosing = Pick.SOURCE })
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
    val p = prices
    if (choosing != null && p != null) {
        AlertDialog(
            onDismissRequest = { choosing = null; priceError = null },
            title = { Text(if (choosing == Pick.CURRENCY) "Currency" else "Price source") },
            text = {
                Column {
                    if (choosing == Pick.CURRENCY) {
                        p.currencies.forEach { c ->
                            Choice(listOfNotNull(c.name.ifEmpty { null }, c.code).joinToString(" · "), c.code == p.currency) {
                                scope.launch { runCatching { model.setCurrency(c.code) }.onSuccess { choosing = null }.onFailure { priceError = it.reason() } }
                            }
                        }
                    } else {
                        p.sources.forEach { s ->
                            Choice(s.text, s.chosen, subtitle = s.note, enabled = s.available) {
                                scope.launch { runCatching { model.setPriceTransport(s.transport) }.onSuccess { choosing = null }.onFailure { priceError = it.reason() } }
                            }
                        }
                    }
                    ErrorText(priceError)
                }
            },
            confirmButton = { TextButton("Done", { choosing = null; priceError = null }, color = Fob.ink) },
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

/** Which list Settings has open in a dialog. */
private enum class Pick { CURRENCY, SOURCE }

/** One of a few choices: its name, and a tick when it's the one chosen; one not [enabled] is muted with why under it. */
@Composable
private fun Choice(title: String, chosen: Boolean, subtitle: String? = null, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).selectable(chosen, enabled = enabled, role = Role.RadioButton, onClick = onClick).padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontSize = 16.sp, color = if (enabled) Fob.ink else Fob.muted)
            if (subtitle != null) Text(subtitle, fontSize = 13.sp, color = Fob.muted)
        }
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
            scope.launch { runCatching { model.setNode(node) }.onSuccess { nav.back() }.onFailure { error = it.reason() } }
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

/** Instant updates: the node's ZMQ address, edited as the Node screen edits the node's. */
@Composable
fun ZmqScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val status by model.status.collectAsState()
    val scope = rememberCoroutineScope()
    var endpoint by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(wallet) { endpoint = wallet?.zmqEndpoint() ?: "" }
    val on = status?.zmq?.let { it !is ZmqStatus.Off } ?: false
    Page(bottom = {
        ErrorText(error)
        PrimaryButton("Use this address", enabled = endpoint.isNotBlank(), onClick = {
            scope.launch { runCatching { model.setZmq(true, endpoint) }.onSuccess { nav.back() }.onFailure { error = it.reason() } }
        })
        if (on) Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            TextButton("Turn off", { scope.launch { runCatching { model.setZmq(false) }.onSuccess { nav.back() }.onFailure { error = it.reason() } } }, color = Fob.ink)
        }
    }) {
        TopBar("Instant updates", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            status?.let { s ->
                Card {
                    InfoRow("Status", s.zmqText)
                    (s.zmq as? ZmqStatus.CantConnect)?.let { Divider(); Text(it.reason, color = Fob.red, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp)) }
                }
            }
            Field(endpoint, { endpoint = it }, "ZMQ address", hint = "Your node's --zmq-pub, on the node's own host: tcp://host:port")
            Text(
                "Your node tells Fob about new blocks and payments the moment it sees them. Fob still checks every two minutes in case one is missed. Only your node's host is ever contacted.",
                color = Fob.muted, fontSize = 13.sp,
            )
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
