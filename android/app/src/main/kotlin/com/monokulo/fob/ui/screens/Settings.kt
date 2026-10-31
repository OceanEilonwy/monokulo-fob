package com.monokulo.fob.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.text.style.TextOverflow
import com.monokulo.fob.ui.Avatar
import uniffi.fob.Profile
import uniffi.fob.ProfileRow
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
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
import androidx.compose.ui.unit.Dp
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import com.monokulo.fob.ui.Manrope
import com.monokulo.fob.ui.Radio
import com.monokulo.fob.ui.SearchField
import uniffi.fob.CurrencyRow
import uniffi.fob.PriceSettings
import uniffi.fob.Activity
import uniffi.fob.CurrencyList
import uniffi.fob.PriceCheck
import uniffi.fob.Network
import uniffi.fob.ZmqStatus
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
fun SettingsScreen(model: WalletModel, nav: Nav, openLocked: (Screen) -> Unit) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val theme by model.theme.collectAsState()
    val status by model.status.collectAsState()
    val scope = rememberCoroutineScope()
    var node by remember { mutableStateOf("") }
    var forgetting by remember { mutableStateOf(false) }
    var switching by remember { mutableStateOf(false) }
    var choosingTheme by remember { mutableStateOf(false) }
    val prices by model.prices.collectAsState()
    // Your profile's row, its words from Rust ("Add a shareable Fob contact" before there's a name).
    var profile by remember { mutableStateOf<Profile?>(null) }
    var profileRow by remember { mutableStateOf<ProfileRow?>(null) }
    // Backup and keys' line, from Rust: "Seed phrase, wallet code, keys".
    var backupLine by remember { mutableStateOf("") }
    // Rust's Node line, again as syncing starts or stops failing.
    LaunchedEffect(wallet, status?.error != null) { node = wallet?.nodeLine() ?: "" }
    LaunchedEffect(wallet) {
        profile = wallet?.profile()
        profileRow = wallet?.profileRow()
        backupLine = wallet?.backupHub()?.settingsLine ?: ""
    }
    // Backup and keys opens only once the phone's lock says it's you;
    // cancelling stays here. The screens it leads to don't ask again while
    // Fob stays on screen.
    fun openBackup() = openLocked(Screen.BackupKeys)
    Page {
        TopBar("Settings", onBack = { nav.back() })
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            Column {
                SectionLabel("Wallet")
                Card {
                    profileRow?.let { row -> ProfileSettingRow(row, profile) { nav.go(Screen.Profile) } }
                    LeadRow(Icons.people, "Contacts", "Who you pay, kept on this phone") { nav.go(Screen.Contacts) }
                    LeadRow(Icons.key, "Backup and keys", backupLine) { openBackup() }
                }
            }
            Column {
                SectionLabel("Display")
                Card { ValueRow("Theme", theme.label) { choosingTheme = true } }
            }
            prices?.let { p ->
                Column {
                    SectionLabel("Money")
                    Card { MoneyRows(p, onCurrency = { nav.go(Screen.CurrencyList) }, onPriceFrom = { nav.go(Screen.PriceFrom) }, onTryAgain = model::retryPrices) { first -> scope.launch { runCatching { model.setFiatFirst(first) } } } }
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
    if (choosingTheme) {
        AlertDialog(
            onDismissRequest = { choosingTheme = false },
            title = { Text("Theme") },
            text = {
                Column(Modifier.selectableGroup()) {
                    ThemeChoice.entries.forEach { choice -> Choice(choice.label, choice == theme) { model.setTheme(choice); choosingTheme = false } }
                }
            },
            confirmButton = { TextButton("Done", { choosingTheme = false }, color = Fob.ink) },
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

/**
 * Your profile in Settings (board ProfileCopy): the photo or a person
 * outline, Rust's title, and with a name "Henry · share as a code" where
 * only the name is cut short, so the rest always shows.
 */
@Composable
private fun ProfileSettingRow(row: ProfileRow, profile: Profile?, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) {}.padding(start = 20.dp, end = 16.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val name = row.name
        if (profile != null && name != null) Avatar(profile.name, profile.initial, profile.photo, 40.dp)
        else Box(Modifier.size(40.dp).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(Icons.person, tint = Fob.muted, size = 20.dp, width = 1.8f) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(row.title, fontSize = 16.sp)
            val suffix = row.suffix
            if (name != null && suffix != null) {
                Row {
                    Text(name, fontSize = 13.sp, color = Fob.muted, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                    Text(suffix, fontSize = 13.sp, color = Fob.muted, maxLines = 1)
                }
            }
        }
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
    }
}

/**
 * A row with its icon in a sunken circle, as the Wallet card has them
 * (Contacts, Backup and keys) and Backup and keys' own rows, taller
 * ([minHeight] 72, [gap] 14).
 */
@Composable
internal fun LeadRow(icon: String, title: String, line: String, minHeight: Dp = 60.dp, gap: Dp = 12.dp, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = minHeight).clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) {}.padding(start = 20.dp, end = 16.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(gap),
    ) {
        Box(Modifier.size(40.dp).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(icon, size = 20.dp, width = 1.8f) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontSize = 16.sp)
            Text(line, fontSize = 13.sp, color = Fob.muted)
        }
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
    }
}

/**
 * The Money card (board PricesWording): "Also show amounts in" with its
 * value; with a currency, Show first and Price from, red with Try again
 * while lookups fail. With XMR only, just the first row.
 */
@Composable
private fun MoneyRows(p: PriceSettings, onCurrency: () -> Unit, onPriceFrom: () -> Unit, onTryAgain: () -> Unit, onFiatFirst: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).clickable(role = Role.Button, onClick = onCurrency)
            .semantics(mergeDescendants = true) {}.padding(start = 20.dp, end = 16.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("Also show amounts in", fontSize = 16.sp)
            Text(p.currencyLine, fontSize = 13.sp, color = Fob.muted)
        }
        // A long name ("New Taiwan dollar") wraps rather than squeezing the title.
        Text(p.currencyText, fontSize = 15.sp, color = Fob.muted, textAlign = TextAlign.End, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(0.8f, fill = false))
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
    }
    val code = p.currency ?: return
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("Show first", fontSize = 16.sp)
            Text("Tap the balance to swap any time", fontSize = 13.sp, color = Fob.muted)
        }
        Segmented(listOf("XMR", code), if (p.fiatFirst) 1 else 0, { i -> onFiatFirst(i == 1) }, Modifier.semantics { contentDescription = "Show first" }, height = 48.dp)
    }
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).clickable(role = Role.Button, onClick = onPriceFrom)
            .padding(start = 20.dp, end = if (p.failing) 8.dp else 16.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("Price from", fontSize = 16.sp)
            // While lookups fail, the toast's line: "Kraken didn't answer · using the 14:02 price".
            Text(p.sourceText, fontSize = 13.sp, color = if (p.failing) Fob.red else Fob.muted)
        }
        if (p.failing) {
            Box(
                Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onTryAgain).padding(horizontal = 8.dp),
                contentAlignment = Alignment.Center,
            ) { Text("Try again", color = Fob.orangeText, fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
        } else {
            StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
        }
    }
}

/**
 * "Also show amounts in" (board CurrencyPicker): a search, then Rust's
 * groups (XMR only, this phone's currency, all of them A–Z). Choosing one
 * goes back; when the provider had to change for it, Rust's words show as
 * a notice.
 */
@Composable
fun CurrencyListScreen(model: WalletModel, nav: Nav) {
    val scope = rememberCoroutineScope()
    val keyboard = LocalSoftwareKeyboardController.current
    var query by rememberSaveable { mutableStateOf("") }
    var list by remember { mutableStateOf<CurrencyList?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    // Rust filters as each letter is typed.
    LaunchedEffect(query) { list = model.currencyList(query) }
    val choose = { code: String? ->
        scope.launch {
            runCatching { model.setCurrency(code) }
                .onSuccess { keyboard?.hide(); nav.back() }
                .onFailure { error = it.reason() }
        }
    }
    Page(scroll = false) {
        TopBar("Also show amounts in", onBack = { nav.back() })
        SearchField(
            query, { query = it }, "Search: name, code or symbol", "Search currencies",
            Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 8.dp), height = 52.dp, onSearch = { keyboard?.hide() },
        )
        ErrorText(error)
        LazyColumn(Modifier.fillMaxWidth().weight(1f), contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp)) {
            list?.empty?.let { words ->
                item { Text(words, color = Fob.muted, fontSize = 15.sp, modifier = Modifier.padding(horizontal = 8.dp, vertical = 16.dp)) }
            }
            list?.groups?.forEach { g ->
                item(key = g.title) {
                    Column(Modifier.padding(top = 14.dp)) {
                        Text(
                            g.title, color = Fob.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.4.sp,
                            modifier = Modifier.padding(start = 8.dp, bottom = 6.dp).semantics { heading() },
                        )
                        Card(Modifier.selectableGroup()) { g.rows.forEach { row -> CurrencyLine(row) { choose(row.code) } } }
                    }
                }
            }
        }
    }
}

/** One currency: its sign in a circle, its name, who prices it, and a tick on the chosen one. */
@Composable
private fun CurrencyLine(row: CurrencyRow, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).selectable(row.chosen, role = Role.RadioButton, onClick = onClick)
            .semantics(mergeDescendants = true) {}.padding(horizontal = 20.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        // The sign isn't read: "A$" says nothing the name doesn't.
        Box(Modifier.size(40.dp).clip(CircleShape).background(Fob.sunken).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
            Text(
                row.sign, fontFamily = Manrope, fontWeight = FontWeight.ExtraBold, fontSize = if (row.sign.length > 2) 13.sp else 17.sp,
                maxLines = 1, softWrap = false,
            )
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(row.name, fontSize = 16.sp)
            Text(row.line, fontSize = 12.5.sp, color = Fob.muted)
        }
        if (row.chosen) StrokeIcon(Icons.check, size = 20.dp)
    }
}

/**
 * Price from (board PriceSource): each provider of the chosen currency,
 * asked as the page opens (and again when the currency or the way of
 * asking changes), then how requests travel. Picking stays here; Rust's
 * words show as a notice when a pick moved the transport.
 */
@Composable
fun PriceFromScreen(model: WalletModel, nav: Nav) {
    val prices by model.prices.collectAsState()
    val scope = rememberCoroutineScope()
    var checks by remember { mutableStateOf<List<PriceCheck>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val p = prices
    val transport = p?.sources?.firstOrNull { it.chosen }?.transport
    LaunchedEffect(p?.currency, transport) {
        checks = null
        checks = model.checkPrices()
    }
    Page {
        TopBar("Price from", onBack = { nav.back() })
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (p?.currency == null) {
                Text("With XMR only, Fob never looks up a price.", color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 8.dp))
                return@Column
            }
            Text(
                buildAnnotatedString {
                    append("Prices in ")
                    withStyle(SpanStyle(fontWeight = FontWeight.SemiBold, color = Fob.ink)) { append(p.currencyPlural ?: p.currencyText) }
                    append(", checked as this page opens. Tap a provider to pick it.")
                },
                color = Fob.muted, fontSize = 14.sp, lineHeight = 20.sp, modifier = Modifier.padding(horizontal = 8.dp),
            )
            Card(Modifier.selectableGroup()) {
                val lines = checks
                if (lines == null) {
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 20.dp).semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        CircularProgressIndicator(Modifier.size(20.dp), color = Fob.muted, strokeWidth = 2.dp)
                        Text("Asking each provider…", color = Fob.muted, fontSize = 14.sp)
                    }
                }
                lines?.forEach { c ->
                    ProviderLine(c, chosen = c.provider == p.provider) {
                        scope.launch { runCatching { model.setPriceProvider(c.provider) }.onSuccess { error = null }.onFailure { error = it.reason() } }
                    }
                }
            }
            Column {
                SectionLabel("Ask over")
                Card(Modifier.selectableGroup()) {
                    p.sources.forEach { s ->
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 60.dp)
                                .selectable(s.chosen, enabled = s.available, role = Role.RadioButton) {
                                    scope.launch { runCatching { model.setPriceTransport(s.transport) }.onSuccess { error = null }.onFailure { error = it.reason() } }
                                }
                                .semantics(mergeDescendants = true) {}.padding(horizontal = 20.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp),
                        ) {
                            Radio(s.chosen, s.available)
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                Text(s.title, fontSize = 16.sp, color = if (s.available) Fob.ink else Fob.outline)
                                Text(s.line, fontSize = 13.sp, color = Fob.muted)
                            }
                        }
                    }
                }
            }
            ErrorText(error)
        }
    }
}

/**
 * One provider on one line: radio, name, a globe (web) and an onion (Tor)
 * for how it can be asked, and its rate; one that failed is greyed, its
 * failure in red, and can't be picked. TalkBack reads Rust's label.
 */
@Composable
private fun ProviderLine(c: PriceCheck, chosen: Boolean, onClick: () -> Unit) {
    val ink = if (c.canPick) Fob.ink else Fob.outline
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).selectable(chosen, enabled = c.canPick, role = Role.RadioButton, onClick = onClick)
            .semantics { contentDescription = c.label }.padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(Modifier.weight(1f).clearAndSetSemantics {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Radio(chosen, c.canPick)
            Text(c.name, fontSize = 16.sp, color = ink)
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                if (c.web) WayIcon(Icons.globe, ink, 1.8f)
                if (c.tor) WayIcon(Icons.onion, ink, 1.6f)
            }
            val failure = c.failure
            Text(failure ?: c.rate ?: "", fontSize = 14.sp, color = if (failure != null) Fob.red else Fob.muted, textAlign = TextAlign.End)
        }
    }
}

/** A small sunken badge with a way of asking's icon. */
@Composable
private fun WayIcon(path: String, tint: Color, width: Float) {
    Box(Modifier.size(24.dp).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(path, tint = tint, size = 14.dp, width = width) }
}

/** A setting whose value shows at the row's end ("Theme · Same as the phone"), opening a choice. */
@Composable
private fun ValueRow(title: String, value: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(title, fontSize = 16.sp)
        // The value at the end, by the chevron, cut short before the title is.
        Text(value, fontSize = 15.sp, color = Fob.muted, maxLines = 1, overflow = TextOverflow.Ellipsis, textAlign = TextAlign.End, modifier = Modifier.weight(1f))
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
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
