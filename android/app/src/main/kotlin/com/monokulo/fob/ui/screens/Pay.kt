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
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
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
import com.monokulo.fob.ui.BarIcons
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.EdgeButton
import com.monokulo.fob.ui.EdgeIconButton
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.InfoRow
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.PatternBadge
import com.monokulo.fob.ui.PatternBadgeSize
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.qrModule
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
    // The model's, read with the wallet, so the address is there on the first frame; none while Rust has none to
    // show (a new wallet's first, say), when opening has Rust hand one out, its room kept meanwhile.
    val own by model.receive.collectAsState()
    val address = own?.address
    val index = own?.index ?: 0u
    var amount by remember { mutableStateOf(screen.draft?.amount ?: "") }
    var note by remember { mutableStateOf(screen.draft?.note ?: "") }
    // Rust cuts the note to what a code holds, and says so when it does.
    var noteHint by remember { mutableStateOf<String?>(null) }
    var expanded by rememberSaveable { mutableStateOf(false) }
    // The amount in money (Rust converts it to XMR for the code) while there's a price; XMR otherwise.
    val prices by model.prices.collectAsState()
    val balance by model.balance.collectAsState()
    // Opened blank as Rust has it already (in money while there's a price), so the field's sign, chip and line are
    // there on the first frame; a form filled before keeps its unit.
    val blank = prices?.blankEntry
    var inFiat by remember { mutableStateOf(screen.draft?.inFiat ?: blank?.inFiat ?: false) }
    var entry by remember { mutableStateOf(blank?.takeIf { it.inFiat == inFiat }) }
    var defaulted by remember { mutableStateOf(screen.draft != null || blank?.canSwitch == true) }
    // Money typed when the price went, kept aside until one comes.
    var heldMoney by remember { mutableStateOf<String?>(null) }
    var code by remember { mutableStateOf<ReceiveCode?>(null) }
    // Who it's for: chosen on the sheet, or the contact whose card opened it.
    var forId by remember { mutableStateOf(screen.contact) }
    var forContact by remember { mutableStateOf<Contact?>(null) }
    var choosing by remember { mutableStateOf(false) }
    var people by remember { mutableStateOf<List<ContactRow>>(emptyList()) }
    // The request made by the last Share or Copy, and what it was made from.
    var made by remember { mutableStateOf<Pair<Asked, MadeRequest>?>(null) }
    // A request on its way: a second tap waits for it, not another (Rust gives the same one again anyway).
    var making by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(wallet, forId) {
        val w = wallet ?: return@LaunchedEffect
        forContact = forId?.let { w.contact(it) }
        // Deleted meanwhile: no one.
        if (forContact == null) forId = null
    }
    LaunchedEffect(wallet, own == null) {
        if (wallet != null && own == null) runCatching { model.openReceive() }.onFailure { error = it.reason() }
    }
    LaunchedEffect(choosing) { if (choosing) people = model.requestContacts() }
    // The price toast's cog opens Price from over this: Back finds the form as typed.
    DisposableEffect(nav) {
        val keep = { nav.replace(Screen.Receive(forId, ReceiveDraft(heldMoney ?: amount, heldMoney != null || inFiat, note))) }
        nav.keepForm = keep
        onDispose { if (nav.keepForm === keep) nav.keepForm = null }
    }
    // A new price comes with a new balance. In money by default while there's one.
    LaunchedEffect(wallet, prices, balance, amount, inFiat) {
        if (wallet == null) return@LaunchedEffect
        val e = model.amountEntry(amount, inFiat) ?: return@LaunchedEffect
        // A price come since it opened: in money, the field's words with it at once rather than a round later.
        if (e.canSwitch && !defaulted && amount.isBlank() && !inFiat) { defaulted = true; entry = model.amountEntry(amount, true); inFiat = true; return@LaunchedEffect }
        if (e.canSwitch) defaulted = true
        // The price gone (another provider not answered yet, or over a day old): the money typed is kept
        // aside, never read as XMR, and put back once a price comes.
        if (e.canSwitch && heldMoney != null && amount.isBlank() && !inFiat) { amount = heldMoney!!; heldMoney = null; inFiat = true; return@LaunchedEffect }
        if (!inFiat && amount.isNotBlank()) heldMoney = null
        if (!e.canSwitch && inFiat) { if (amount.isNotBlank()) heldMoney = amount; inFiat = false; amount = ""; return@LaunchedEffect }
        entry = e
    }
    // What's typed, in the unit Rust last said it's in; none while Rust is behind (the chip just tapped), so
    // nothing is made, copied or shared from an amount read in the wrong unit.
    val asked = entry?.takeIf { it.forText == amount && it.forInFiat == inFiat }?.let { Asked(forId, amount, it.inFiat, note) }
    // The request's code while the form is as it was made from; Receive's own otherwise.
    val request = made?.takeIf { it.first == asked }?.second
    // Keyed on the price too: a new one makes the code again, so it asks what the field and Share say.
    LaunchedEffect(wallet, address, asked, request, prices?.priceAt) {
        if (request != null) { code = request.code; return@LaunchedEffect }
        val w = wallet ?: return@LaunchedEffect
        val a = address ?: return@LaunchedEffect
        val q = asked ?: return@LaunchedEffect
        code = runCatching { w.receiveCode(a, q.amount, q.inFiat, q.note) }.getOrNull()
    }
    // A note typed for Receive's own code goes with its address, kept when the screen or the address changes;
    // one that went into a request is the request's.
    val latestNote by rememberUpdatedState(note.takeIf { request == null } ?: "")
    DisposableEffect(index) {
        val shown = index
        onDispose { if (shown > 0u && latestNote.isNotBlank()) model.rememberReceiveNote(shown, latestNote) }
    }
    val shownAddress = request?.address ?: address

    /** The request for what's typed, for [contact] (the one already made, if it's the same), then [then] with it. */
    fun withRequest(contact: ULong?, then: (MadeRequest) -> Unit) {
        if (making) return
        val now = asked?.copy(contact = contact) ?: return
        error = null
        made?.takeIf { it.first == now }?.let { then(it.second); return }
        making = true
        scope.launch {
            runCatching { model.makeRequest(now.contact, now.amount, now.inFiat, now.note) }
                .onSuccess { made = now to it; then(it) }
                .onFailure { error = it.reason() }
            making = false
        }
    }
    fun share(contact: ULong?) = withRequest(contact) {
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, it.code.uri), null))
    }
    // Back from New contact, made for this request: share with them now.
    LaunchedEffect(forContact, asked) {
        if (screen.shareOnOpen && forContact != null && asked != null) { screen.shareOnOpen = false; share(forId) }
    }

    val qr = remember(code) { code?.let { runCatching { qrCode(it.uri) }.getOrNull() } }
    val groups = remember(shownAddress) { shownAddress?.let { addressGroups(it) } }
    // While the address comes, an unseen stand-in of the same length keeps its lines' room.
    val standIn = remember(wallet) { wallet?.let { addressGroups(it.primaryAddress()) } ?: emptyList() }
    val shownCode = code
    if (expanded && shownCode != null && qr != null) {
        ExpandedQr(shownCode, qr, shownAddress) { expanded = false }
        return
    }
    Box(Modifier.fillMaxSize()) {
        Page(bottom = {
            ErrorText(error)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                val ready = code != null && asked != null && !making
                OutlineButton("Copy", {
                    if (ready) withRequest(forId) { scope.launch { clipboard.setClipEntry(ClipData.newPlainText("Monero address", it.address).toClipEntry()) } }
                }, Modifier.weight(1f), enabled = ready)
                PrimaryButton(forContact?.let { "Share with ${it.name}" } ?: "Share", {
                    if (ready) { if (forContact != null) share(forId) else choosing = true }
                }, Modifier.weight(1f), enabled = ready)
            }
        }) {
            TopBar("Receive", onBack = { nav.back() }) {
                // A new address, as the refresh icon alone (Round 9).
                EdgeIconButton(Icons.refresh, "New address", {
                    scope.launch { runCatching { model.newReceiveAddress() }.onSuccess { made = null }.onFailure { error = it.reason() } }
                })
            }
            Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
                // The code on a white card as wide as the page (at most 360 dp of code); the round button on its
                // corner, or the code itself, opens it full screen. With the code's own 2-module quiet zone, 12 dp
                // leaves 4 modules or more of white beside it. The address's pattern sits across the card's bottom
                // edge and who it's for across its top, each reaching into the quiet zone and stopping one module
                // short of the code, whatever its module size (a bare address's, version 6, until the code comes, so
                // nothing moves as it does). Opened for a contact, the top's room is kept while Rust reads them.
                BoxWithConstraints(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                    val side = (maxWidth - 24.dp).coerceAtMost(360.dp)
                    val module = qrModule(qr?.size?.toInt() ?: 41, side)
                    val forWho = forId != null
                    Box(Modifier.padding(top = if (forWho) PatternBadgeSize / 2 else 4.dp, bottom = PatternBadgeSize / 2)) {
                        Box(
                            Modifier.clip(RoundedCornerShape(14.dp)).background(Fob.qrLight)
                                .clickable(enabled = qr != null, onClickLabel = "Show larger", role = Role.Button) { expanded = true }
                                .padding(
                                    start = 12.dp, end = 12.dp, bottom = PatternBadgeSize / 2 - module,
                                    top = if (forWho) PatternBadgeSize / 2 - module else 12.dp,
                                ),
                        ) {
                            if (qr != null) QrImage(qr, side) else Spacer(Modifier.size(side))
                        }
                        forContact?.let { ForChip(it, Modifier.align(Alignment.TopCenter).offset(y = -PatternBadgeSize / 2)) { choosing = true } }
                        if (qr != null && shownAddress != null) PatternBadge(shownAddress, Modifier.align(Alignment.BottomCenter).offset(y = PatternBadgeSize / 2))
                        Box(
                            Modifier.align(Alignment.BottomEnd).offset(10.dp, 10.dp).size(32.dp).clip(CircleShape).background(Fob.sunken)
                                .clickable(enabled = qr != null, role = Role.Button) { expanded = true }
                                .semantics { contentDescription = "Show larger" },
                            contentAlignment = Alignment.Center,
                        ) { StrokeIcon(Icons.expand, tint = Fob.ink, size = 16.dp) }
                    }
                }
                AddressGroups(groups ?: standIn, shown = groups != null)
                AmountField(
                    amount, { amount = it }, entry, placeholder = "Amount",
                    onSwitch = {
                        // The text, unit and Rust's word on them change together.
                        scope.launch {
                            val w = wallet ?: return@launch
                            val toFiat = !inFiat
                            val switched = w.convertAmount(amount, toFiat)
                            val e = model.amountEntry(switched, toFiat) ?: return@launch
                            amount = switched; inFiat = toFiat; entry = e
                        }
                    },
                )
                Field(note, { typed -> fitReceiveNote(typed).let { note = it.text; noteHint = it.hint } }, "Note", placeholder = "What it's for, shown to the payer", prose = true, hint = noteHint)
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
                        nav.replace(Screen.Receive(forId, ReceiveDraft(heldMoney ?: amount, heldMoney != null || inFiat, note)))
                        nav.go(Screen.ContactEdit(forRequest = true))
                    },
                    onNoName = { choosing = false; forId = null; forContact = null; share(null) },
                )
            }
        }
    }
}

/**
 * Who a request is for, across the top edge of the code's card as the
 * pattern is across its bottom (Round 9): their avatar and name on the
 * card's white, ringed in the page's colour, [PatternBadgeSize] tall and as
 * wide as the name; tapping chooses again.
 */
@Composable
private fun ForChip(contact: Contact, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Row(
        modifier.widthIn(max = 320.dp).height(PatternBadgeSize).clip(CircleShape).background(Fob.paper).padding(3.dp)
            .clip(CircleShape).background(Fob.qrLight)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = "For ${contact.name}. Change who it's for" }
            .padding(start = 4.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Avatar(contact.name, contact.initial, contact.photo, 36.dp)
        Text(contact.name, color = Fob.qrDark, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * Share's sheet (Round 5 › "Share asks who it's for"): every contact with
 * Rust's line about their requests, New contact, then Share without a name.
 */
@Composable
private fun ColumnScope.WhoFor(people: List<ContactRow>, onPick: (ContactRow) -> Unit, onNew: () -> Unit, onNoName: () -> Unit) {
    SheetHandle()
    Text("Who is this for?", style = display(22), modifier = Modifier.padding(start = 4.dp, end = 4.dp, top = 6.dp).semantics { heading() })
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

/**
 * The address in full, in Rust's groups of five; the first, middle and last
 * stand out. Not [shown]: laid out unseen and unheard, keeping its room.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AddressGroups(groups: List<AddressGroup>, shown: Boolean = true) {
    FlowRow(
        Modifier.fillMaxWidth().then(if (shown) Modifier.semantics(mergeDescendants = true) {} else Modifier.alpha(0f).clearAndSetSemantics {}),
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
fun ExpandedQr(code: ReceiveCode, qr: QrCode, address: String?, onClose: () -> Unit) {
    BackHandler(onBack = onClose)
    FullBrightness()
    // White in both themes: the bars' icons dark on it.
    BarIcons(darkIcons = true)
    Column(Modifier.fillMaxSize().background(Fob.qrLight).safeDrawingPadding()) {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically) {
            EdgeIconButton(Icons.close, "Close", onClose, tint = Fob.qrDark)
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
            // Under the code's own quiet zone, as on Receive's card: never over it.
            address?.let { AddressPattern(it, PatternBadgeSize) }
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
        window?.attributes = window.attributes.apply { screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_FULL }
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

/**
 * Confirm: what's paid, to whom, the fee; swipe, then the phone's lock.
 * Opened from Send before the fee is in ([Screen.Confirm.pending]), it
 * shows what Rust checked at once, the fee and total waiting, and prepares
 * the payment itself, becoming the prepared screen when it's in (Round 9 ›
 * 3b). While it's sending, Back does nothing (Rust finishes and records the
 * payment whatever happens to the screen). Leaving any other way forgets
 * the prepared payment, unless the screen is only being made again; an
 * identity prompt still up then ([identityPending]) carries on here.
 */
@Composable
fun ConfirmScreen(
    model: WalletModel, nav: Nav, screen: Screen.Confirm, confirmIdentity: suspend (String) -> Boolean, identityPending: () -> Boolean,
) {
    val scope = rememberCoroutineScope()
    val activity = LocalActivity.current
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val prepared = screen.prepared
    val preview = screen.pending?.preview
    val note = screen.note
    val split = prepared?.split
    val receipt = prepared?.receipt ?: preview?.receipt
    val amountText = prepared?.amountText ?: preview?.amountText ?: ""
    val to = prepared?.to ?: preview?.to ?: ""
    val title = split?.title ?: receipt?.merchant?.let { "Pay $it" } ?: "Send"
    val sending by rememberUpdatedState(busy)
    DisposableEffect(prepared?.id) {
        onDispose { if (prepared != null && !sending && activity?.isChangingConfigurations != true) model.cancelLater(prepared) }
    }
    BackHandler(enabled = busy) {}
    // The fee: once in, this screen becomes the prepared one; gone meanwhile, Rust forgets it.
    LaunchedEffect(screen) {
        val pending = screen.pending
        if (prepared != null || pending == null) return@LaunchedEffect
        val d = pending.draft
        runCatching { model.prepare(d.to, d.amount, d.inFiat, note, d.code) }
            .onSuccess { if (nav.current === screen) nav.replace(screen.copy(prepared = it)) else model.cancelLater(it) }
            .onFailure { error = it.reason() }
    }

    fun swiped() {
        val ready = prepared ?: return
        busy = true
        error = null
        scope.launch {
            if (!confirmIdentity(title)) {
                error = "Not sent."
            } else {
                runCatching { model.send(ready) }
                    // Sent: neither this nor the filled form under it is a step back from Paid.
                    .onSuccess { tx -> nav.back(); if (nav.current is Screen.Send) nav.back(); nav.go(Screen.Paid(ready, tx)) }
                    .onFailure { error = it.reason() }
            }
            busy = false
        }
    }
    // Made again while the phone's lock was asked for: its answer comes here.
    LaunchedEffect(Unit) { if (identityPending()) swiped() }
    Page(bottom = {
        ErrorText(error)
        val label = when {
            busy -> "Sending…"
            prepared == null -> if (error == null) "Getting the fee…" else "Can't send"
            else -> split?.swipe ?: "Swipe to send ${prepared.amountText} XMR"
        }
        SwipeToSend(label, enabled = !busy && prepared != null, waiting = prepared == null && error == null) { swiped() }
    }) {
        TopBar(title, onBack = { if (!busy) nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Column(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text(receipt?.total ?: "$amountText XMR", style = display(52))
                if (receipt != null) Text("$amountText XMR", color = Fob.muted, fontSize = 18.sp)
                // Worth now, at the current price.
                else (prepared?.amountFiat ?: preview?.amountFiat)?.let { Text(it, color = Fob.muted, fontSize = 18.sp) }
                note?.let { Text(it, color = Fob.muted, fontSize = 15.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(top = 6.dp)) }
            }
            Card {
                if (split != null) InfoRow("To", split.to) else InfoRow("To", short(to, 2), mono = true)
                Divider()
                InfoRow("Network fee", prepared?.let { "${it.feeText} XMR" + (it.feeFiat?.let { f -> " · $f" } ?: "") })
                Divider()
                InfoRow("Total", prepared?.let { "${it.totalText} XMR" + (it.totalFiat?.let { f -> " · $f" } ?: "") }, bold = true)
            }
            // A split's pieces; its address is this wallet's own, with no pattern to compare.
            if (split != null) Card {
                Text(split.piecesText, fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(start = 20.dp, top = 14.dp, bottom = 4.dp))
                split.pieces.forEach { InfoRow(it.label, it.amountText) }
            } else Card(padding = PaddingValues(horizontal = 20.dp, vertical = 16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    AddressPattern(to, 54.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text("Address pattern", fontSize = 16.sp, fontWeight = FontWeight.Medium)
                        Text("Check it matches the pattern on their screen. If it differs, don't send.", color = Fob.muted, fontSize = 14.sp)
                    }
                }
            }
            receipt?.let { Card { Text("Receipt · ${it.itemCount} items", fontSize = 16.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(start = 20.dp, top = 14.dp)); ReceiptLines(it) } }
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
