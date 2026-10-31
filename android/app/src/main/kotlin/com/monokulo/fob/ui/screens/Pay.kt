package com.monokulo.fob.ui.screens

import android.content.ClipData
import android.content.Intent
import android.view.WindowManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.border
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.IconButton
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
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
import com.monokulo.fob.ui.AmountField
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.InfoRow
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.SwipeToSend
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import com.monokulo.fob.ui.short
import kotlinx.coroutines.launch
import uniffi.fob.AddressGroup
import uniffi.fob.Prepared
import uniffi.fob.QrCode
import uniffi.fob.Receipt
import uniffi.fob.ReceiveCode
import uniffi.fob.SaveOffer
import uniffi.fob.Contact
import com.monokulo.fob.ui.Avatar
import androidx.compose.ui.text.style.TextOverflow
import uniffi.fob.AmountEntry
import uniffi.fob.ContactRow
import uniffi.fob.MadeRequest
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.SheetHandle
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.semantics.heading
import uniffi.fob.addressGroups
import uniffi.fob.fitReceiveNote
import uniffi.fob.qrCode

/**
 * Receive's form as typed, kept on the back stack while New contact (from
 * Share's sheet) is open, so it comes back filled.
 */
data class ReceiveDraft(val amount: String, val inFiat: Boolean, val note: String)

/** What a request was made from: the same again shares the same request, not a new one. */
private data class Asked(val contact: ULong?, val amount: String, val inFiat: Boolean, val note: String)

/**
 * Receive (Round 2 › Receive; Round 5 › "Share asks who it's for",
 * "Request for Sam"). Opening it makes nothing: its code is this wallet's
 * Receive address. Share or Copy makes a request (Rust: a new address for
 * it alone, with its note and amount), and the code switches to it. Share
 * asks who it's for first, unless a contact is chosen already (the chip
 * over the code, or their card's Request).
 */
@Composable
fun ReceiveScreen(model: WalletModel, nav: Nav, screen: Screen.Receive) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    var address by remember { mutableStateOf<String?>(null) }
    var index by remember { mutableStateOf(0u) }
    var amount by remember { mutableStateOf(screen.draft?.amount ?: "") }
    var note by remember { mutableStateOf(screen.draft?.note ?: "") }
    // Rust cuts the note to what a code holds, and says so when it does.
    var noteHint by remember { mutableStateOf<String?>(null) }
    var expanded by rememberSaveable { mutableStateOf(false) }
    // The amount in money (Rust converts it to XMR for the code) while there's a price; XMR otherwise.
    val prices by model.prices.collectAsState()
    val balance by model.balance.collectAsState()
    var entry by remember { mutableStateOf<AmountEntry?>(null) }
    var inFiat by remember { mutableStateOf(screen.draft?.inFiat ?: false) }
    var defaulted by remember { mutableStateOf(screen.draft != null) }
    var code by remember { mutableStateOf<ReceiveCode?>(null) }
    // Who it's for: chosen on the sheet, or the contact whose card opened it.
    var forId by remember { mutableStateOf(screen.contact) }
    var forContact by remember { mutableStateOf<Contact?>(null) }
    var choosing by remember { mutableStateOf(false) }
    var people by remember { mutableStateOf<List<ContactRow>>(emptyList()) }
    // The request made by the last Share or Copy, and what it was made from.
    var made by remember { mutableStateOf<Pair<Asked, MadeRequest>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    // "The code names you as “Henry” · change in Settings", while requests carry the profile's name.
    var nameLine by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(wallet) {
        val w = wallet ?: return@LaunchedEffect
        address = w.receiveAddress()
        index = w.receiveIndex()
        nameLine = w.receiveNameLine()
    }
    LaunchedEffect(wallet, forId) {
        val w = wallet ?: return@LaunchedEffect
        forContact = forId?.let { w.contact(it) }
        // Deleted meanwhile: no one.
        if (forContact == null) forId = null
    }
    LaunchedEffect(choosing) { if (choosing) people = model.requestContacts() }
    // A new price comes with a new balance. In money by default while there's one.
    LaunchedEffect(wallet, prices, balance, amount, inFiat) {
        val e = wallet?.amountEntry(amount, inFiat) ?: return@LaunchedEffect
        if (e.canSwitch && !defaulted && amount.isBlank() && !inFiat) { defaulted = true; inFiat = true; return@LaunchedEffect }
        if (e.canSwitch) defaulted = true
        if (!e.canSwitch && inFiat) { inFiat = false; amount = ""; return@LaunchedEffect }
        entry = e
    }
    val asked = Asked(forId, amount, entry?.inFiat == true, note)
    // The request's code while the form is as it was made from; Receive's own otherwise.
    val request = made?.takeIf { it.first == asked }?.second
    LaunchedEffect(wallet, address, asked, request) {
        if (request != null) { code = request.code; return@LaunchedEffect }
        val w = wallet ?: return@LaunchedEffect
        val a = address ?: return@LaunchedEffect
        code = runCatching { w.receiveCode(a, amount, asked.inFiat, note) }.getOrNull()
    }
    // A note typed for Receive's own code goes with its address, kept when the screen or the address changes;
    // one that went into a request is the request's.
    val latestNote by rememberUpdatedState(note.takeIf { request == null } ?: "")
    DisposableEffect(index) {
        val shown = index
        onDispose { if (shown > 0u && latestNote.isNotBlank()) model.rememberReceiveNote(shown, latestNote) }
    }
    val shownAddress = request?.address ?: address
    val shownIndex = request?.index ?: index

    /** The request for what's typed, for [contact] (the one already made, if it's the same), then [then] with it. */
    fun withRequest(contact: ULong?, then: (MadeRequest) -> Unit) {
        error = null
        val now = Asked(contact, amount, entry?.inFiat == true, note)
        made?.takeIf { it.first == now }?.let { then(it.second); return }
        scope.launch {
            runCatching { model.makeRequest(now.contact, now.amount, now.inFiat, now.note) }
                .onSuccess { made = now to it; then(it) }
                .onFailure { error = it.reason() }
        }
    }
    fun share(contact: ULong?) = withRequest(contact) {
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, it.code.uri), null))
    }
    // Back from New contact, made for this request: share with them now.
    LaunchedEffect(forContact, entry) {
        if (screen.shareOnOpen && forContact != null && entry != null) { screen.shareOnOpen = false; share(forId) }
    }

    val qr = code?.let { runCatching { qrCode(it.uri) }.getOrNull() }
    val groups = remember(shownAddress) { shownAddress?.let { addressGroups(it) } ?: emptyList() }
    val shownCode = code
    if (expanded && shownCode != null && qr != null && shownAddress != null) {
        ExpandedQr(shownCode, qr, shownAddress) { expanded = false }
        return
    }
    Box(Modifier.fillMaxSize()) {
        Page(bottom = {
            ErrorText(error)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlineButton("Copy", {
                    if (code != null) withRequest(forId) { scope.launch { clipboard.setClipEntry(ClipData.newPlainText("Monero address", it.address).toClipEntry()) } }
                }, Modifier.weight(1f))
                PrimaryButton(forContact?.let { "Share with ${it.name}" } ?: "Share", {
                    if (code != null) { if (forContact != null) share(forId) else choosing = true }
                }, Modifier.weight(1f))
            }
        }) {
            TopBar("Receive", onBack = { nav.back() })
            Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
                forContact?.let { ForChip(it) { choosing = true } }
                // The code on a white card; the round button on its corner, or the code itself, opens it full screen.
                Box(Modifier.padding(top = 4.dp)) {
                    Box(
                        Modifier.clip(RoundedCornerShape(14.dp)).background(Fob.qrLight)
                            .clickable(enabled = qr != null, onClickLabel = "Show larger", role = Role.Button) { expanded = true }
                            .padding(20.dp),
                    ) {
                        if (qr != null) QrImage(qr, 220.dp) else Spacer(Modifier.size(220.dp))
                    }
                    Box(
                        Modifier.align(Alignment.BottomEnd).offset(10.dp, 10.dp).size(32.dp).clip(CircleShape).background(Fob.sunken)
                            .clickable(enabled = qr != null, role = Role.Button) { expanded = true }
                            .semantics { contentDescription = "Show larger" },
                        contentAlignment = Alignment.Center,
                    ) { StrokeIcon(Icons.expand, tint = Fob.ink, size = 16.dp) }
                }
                shownAddress?.let {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        AddressPattern(it, 36.dp)
                        Text("The payer's Fob shows this pattern too", color = Fob.muted, fontSize = 13.sp)
                    }
                }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            if (request != null) "Address $shownIndex · this request's own" else "Address $shownIndex · a new one each time",
                            color = Fob.muted, fontSize = 13.sp, modifier = Modifier.weight(1f),
                        )
                        TextButton("New address", {
                            scope.launch {
                                val w = wallet ?: return@launch
                                address = w.newReceiveAddress()
                                index = w.receiveIndex()
                                made = null
                            }
                        })
                    }
                    AddressGroups(groups)
                }
                AmountField(
                    amount, { amount = it }, entry, placeholder = "Amount (optional)",
                    onSwitch = { scope.launch { val toFiat = !inFiat; amount = wallet?.convertAmount(amount, toFiat) ?: ""; inFiat = toFiat } },
                )
                Field(note, { typed -> fitReceiveNote(typed).let { note = it.text; noteHint = it.hint } }, "Note (optional)", placeholder = "What it's for, shown to the payer", prose = true, hint = noteHint)
                // While requests carry your profile's name: tapping it opens Your profile, where the switch is.
                nameLine?.let {
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp))
                            .clickable(role = Role.Button, onClickLabel = "Change in Settings") { nav.go(Screen.Profile) }.padding(horizontal = 4.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        StrokeIcon(Icons.person, tint = Fob.muted, size = 16.dp)
                        Text(it, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
                    }
                }
            }
        }
        if (choosing) {
            BottomSheet("Who is this for?", onDismiss = { choosing = false }) {
                WhoFor(
                    people,
                    onPick = { p -> choosing = false; forId = p.id; share(p.id) },
                    onNew = {
                        choosing = false
                        // Back from New contact, Receive comes back filled, with them chosen and shared with.
                        nav.replace(Screen.Receive(forId, ReceiveDraft(amount, inFiat, note)))
                        nav.go(Screen.ContactEdit(forRequest = true))
                    },
                    onNoName = { choosing = false; forId = null; forContact = null; share(null) },
                )
            }
        }
    }
}

/** Who a request is for, over the code: their avatar and name, as wide as the name; tapping chooses again. */
@Composable
private fun ForChip(contact: Contact, onClick: () -> Unit) {
    Row(
        Modifier.widthIn(max = 600.dp).heightIn(min = 48.dp).clip(RoundedCornerShape(14.dp)).background(Fob.card)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = "For ${contact.name}. Change who it's for" }
            .padding(start = 8.dp, end = 14.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Avatar(contact.name, contact.initial, contact.photo, 32.dp)
        Text(contact.name, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * Share's sheet (Round 5 › "Share asks who it's for"): every contact with
 * Rust's line about their requests, New contact, then Share without a name.
 */
@Composable
private fun ColumnScope.WhoFor(people: List<ContactRow>, onPick: (ContactRow) -> Unit, onNew: () -> Unit, onNoName: () -> Unit) {
    SheetHandle()
    Column(Modifier.padding(start = 4.dp, end = 4.dp, top = 6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("Who is this for?", style = display(22), modifier = Modifier.semantics { heading() })
        Text("Pick a contact: this request and its address go on their card, and what arrives shows as from them.", color = Fob.muted, fontSize = 14.sp, lineHeight = 20.sp)
    }
    Card(Modifier.weight(1f, fill = false)) {
        Column(Modifier.verticalScroll(rememberScrollState())) {
            people.forEach { p ->
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 60.dp).clickable(role = Role.Button) { onPick(p) }.padding(horizontal = 20.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Avatar(p.name, p.initial, p.photo, 36.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                        Text(p.name, fontSize = 16.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(p.line, color = Fob.muted, fontSize = 12.5.sp)
                    }
                }
            }
            Row(
                Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button, onClick = onNew).padding(horizontal = 20.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Box(Modifier.size(36.dp).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(Icons.plus, size = 18.dp) }
                Text("New contact", fontSize = 16.sp, fontWeight = FontWeight.Medium)
            }
        }
    }
    OutlineButton("Share without a name", onNoName, Modifier.fillMaxWidth().padding(top = 4.dp), height = 52.dp)
}

/** The address in full, in Rust's groups of five; the first, middle and last stand out. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AddressGroups(groups: List<AddressGroup>) {
    FlowRow(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        groups.forEach { g ->
            Text(
                g.text, fontFamily = FontFamily.Monospace, fontSize = 14.sp, lineHeight = 20.sp,
                fontWeight = if (g.highlighted) FontWeight.Bold else FontWeight.Normal,
                color = if (g.highlighted) Fob.onAddressMark else Fob.ink,
                modifier = Modifier.clip(RoundedCornerShape(4.dp)).background(if (g.highlighted) Fob.addressMark else Color.Transparent).padding(horizontal = 3.dp),
            )
        }
    }
}

/**
 * The payment code full screen, white in both themes, with the screen as
 * bright as it goes until it closes.
 */
@Composable
fun ExpandedQr(code: ReceiveCode, qr: QrCode, address: String, onClose: () -> Unit) {
    BackHandler(onBack = onClose)
    FullBrightness()
    Column(Modifier.fillMaxSize().background(Fob.qrLight).safeDrawingPadding()) {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onClose, modifier = Modifier.semantics { contentDescription = "Close" }) { StrokeIcon(Icons.close, tint = Fob.qrDark, size = 20.dp) }
        }
        Column(
            Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        ) {
            if (code.headline != null || code.detail != null) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    // Rust writes both: "$20.00" over "0.1204 XMR · note" for money, "0.5 XMR" over the note for XMR.
                    code.headline?.let { Text(it, style = display(34), color = Fob.qrDark, textAlign = TextAlign.Center) }
                    code.detail?.let { Text(it, color = Fob.qrMuted, fontSize = 16.sp, textAlign = TextAlign.Center) }
                }
            }
            QrImage(qr, 320.dp, quiet = 4)
            AddressPattern(address, 48.dp)
            Text("Screen brightness turned up while this is open", color = Fob.qrMuted, fontSize = 13.sp, textAlign = TextAlign.Center)
        }
    }
}

/** The screen as bright as it goes while this is shown, as it was after. */
@Composable
fun FullBrightness() {
    val activity = LocalActivity.current
    DisposableEffect(activity) {
        val window = activity?.window
        val before = window?.attributes?.screenBrightness
        window?.attributes = window?.attributes?.apply { screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_FULL }
        onDispose { if (before != null) window.attributes = window.attributes.apply { screenBrightness = before } }
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
fun ConfirmScreen(model: WalletModel, nav: Nav, prepared: Prepared, note: String?, confirmIdentity: suspend (String) -> Boolean) {
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val split = prepared.split
    val title = split?.title ?: prepared.receipt?.merchant?.let { "Pay $it" } ?: "Send"
    Page(bottom = {
        ErrorText(error)
        SwipeToSend(if (busy) "Sending…" else split?.swipe ?: "Swipe to send ${prepared.amountText} XMR", enabled = !busy) {
            busy = true
            error = null
            scope.launch {
                if (!confirmIdentity(title)) {
                    error = "Not sent."
                } else {
                    runCatching { model.send(prepared) }
                        // Sent: neither this nor the filled form under it is a step back from Paid.
                        .onSuccess { tx -> nav.back(); if (nav.current is Screen.Send) nav.back(); nav.go(Screen.Paid(prepared, tx)) }
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
                // Worth now, at the current price.
                else prepared.amountFiat?.let { Text(it, color = Fob.muted, fontSize = 18.sp) }
                note?.let { Text(it, color = Fob.muted, fontSize = 15.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(top = 6.dp)) }
            }
            Card {
                if (split != null) InfoRow("To", split.to) else InfoRow("To", short(prepared.to, 2), mono = true)
                Divider()
                InfoRow("Network fee", "${prepared.feeText} XMR" + (prepared.feeFiat?.let { " · $it" } ?: ""))
                Divider()
                InfoRow("Total", "${prepared.totalText} XMR" + (prepared.totalFiat?.let { " · $it" } ?: ""), bold = true)
            }
            // A split's pieces; its address is this wallet's own, with no pattern to compare.
            if (split != null) Card {
                Text(split.piecesText, fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(start = 20.dp, top = 14.dp, bottom = 4.dp))
                split.pieces.forEach { InfoRow(it.label, it.amountText) }
            } else Card(padding = PaddingValues(horizontal = 20.dp, vertical = 16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    AddressPattern(prepared.to, 54.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text("Address pattern", fontSize = 16.sp, fontWeight = FontWeight.Medium)
                        Text("Check it matches the pattern on their screen. If it differs, don't send.", color = Fob.muted, fontSize = 14.sp)
                    }
                }
            }
            prepared.receipt?.let { Card { Text("Receipt · ${it.itemCount} items", fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(start = 20.dp, top = 14.dp)); ReceiptLines(it) } }
        }
    }
}

/**
 * Paid: the tick and what went where, then (Round 4 › "Sent: a small Save
 * contact button"; Round 5 › "Paid a code named Sam") what Rust offers:
 * Add to the contact the code's name matches, or Save as a contact;
 * nothing when the address is already a contact's.
 */
@Composable
fun PaidScreen(model: WalletModel, nav: Nav, prepared: Prepared, tx: String) {
    val wallet by model.wallet.collectAsState()
    val activity by model.activity.collectAsState()
    val scope = rememberCoroutineScope()
    var offer by remember { mutableStateOf<SaveOffer?>(null) }
    LaunchedEffect(wallet, activity) { offer = model.saveOffer(tx) }
    Page(bottom = { PrimaryButton("Done", { nav.reset(Screen.Home) }) }) {
        Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 72.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Box(Modifier.size(96.dp).clip(CircleShape).background(Fob.greenFill), contentAlignment = Alignment.Center) { StrokeIcon(Icons.check, tint = Fob.onGreen, size = 52.dp, width = 2f) }
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                val split = prepared.split
                Text(split?.title ?: "Paid", style = display(34))
                Text(split?.done ?: prepared.receipt?.let { "${it.total} to ${it.merchant}" } ?: "${prepared.amountText} XMR to ${short(prepared.to, 1)}", fontSize = 18.sp, textAlign = TextAlign.Center)
                Text("Sent · confirming on the network", color = Fob.muted, fontSize = 14.sp)
                offer?.named?.let { Text(it, color = Fob.muted, fontSize = 15.sp, textAlign = TextAlign.Center) }
            }
            offer?.let { o -> SaveOfferButtons(o, onAdd = { c ->
                scope.launch {
                    runCatching { model.changeContacts { it.addContactPaidAt(c.id, o.address) } }.onSuccess { nav.go(Screen.Contact(c.id)) }
                }
            }, onNew = { nav.go(Screen.ContactEdit(prefill = o.draft)) }) }
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

/**
 * After paying: "Add to Sam" with Sam's avatar and "A different Sam? New
 * contact" when the code's name is a contact's; otherwise "Save as a
 * contact". Under a named code's, Rust's word that the name wasn't checked.
 */
@Composable
private fun SaveOfferButtons(offer: SaveOffer, onAdd: (Contact) -> Unit, onNew: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        val match = offer.addTo
        Row(
            Modifier.height(52.dp).clip(RoundedCornerShape(26.dp)).border(1.dp, Fob.outline, RoundedCornerShape(26.dp))
                .clickable(role = Role.Button) { if (match != null) onAdd(match) else onNew() }
                .padding(start = if (match != null) 14.dp else 18.dp, end = 22.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(if (match != null) 10.dp else 8.dp),
        ) {
            if (match != null) Avatar(match.name, match.initial, match.photo, 30.dp)
            else StrokeIcon(Icons.personPlus, tint = Fob.ink, size = 20.dp, width = 2f)
            Text(offer.button, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 220.dp))
        }
        offer.other?.let { TextButton(it, onNew) }
        offer.footnote?.let {
            Text(it, color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, textAlign = TextAlign.Center, modifier = Modifier.widthIn(max = 320.dp).padding(top = 4.dp))
        }
    }
}
