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
import androidx.compose.ui.platform.LocalConfiguration
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
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.SheetHandle
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.semantics.heading
import uniffi.fob.addressGroups
import uniffi.fob.fitReceiveNote
import uniffi.fob.qrCode
import com.monokulo.fob.ui.optionalOutline
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.focus.focusRequester
import com.monokulo.fob.ui.SearchField
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.ui.draw.rotate
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import com.monokulo.fob.Nfc
import com.monokulo.fob.ui.NfcBadge
import com.monokulo.fob.ui.openNfcSettings
import com.monokulo.fob.ui.rememberPayCard
import com.monokulo.fob.ui.rememberTick
import com.monokulo.fob.ui.rememberTouchState

/**
 * Receive's form as typed, kept on the back stack while New contact (from
 * Share's sheet) is open, so it comes back filled.
 */
data class ReceiveDraft(val amount: String, val inFiat: Boolean, val note: String)

/** What the code was made from. */
private data class Asked(val amount: String, val inFiat: Boolean, val note: String)

/**
 * Receive (Round 2 › Receive; Round 5 › "Share asks who it's for",
 * "Request for Sam"). Opening it makes nothing: its code is this wallet's
 * Receive address. Share (the icon in the top bar) makes a request (Rust: a new address for
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
    val asked = entry?.takeIf { it.forText == amount && it.forInFiat == inFiat }?.let { Asked(amount, it.inFiat, note) }
    // Keyed on the price too: a new one makes the code again, so it asks what the field and Share say.
    LaunchedEffect(wallet, address, asked, prices?.priceAt) {
        val w = wallet ?: return@LaunchedEffect
        val a = address ?: return@LaunchedEffect
        val q = asked ?: return@LaunchedEffect
        code = runCatching { w.receiveCode(a, q.amount, q.inFiat, q.note) }.getOrNull()
    }
    // A note typed for Receive's own code goes with its address, kept when the screen or the address changes.
    val latestNote by rememberUpdatedState(note)
    DisposableEffect(index) {
        val shown = index
        onDispose { if (shown > 0u && latestNote.isNotBlank()) model.rememberReceiveNote(shown, latestNote) }
    }
    val shownAddress = address

    // Share gives the code on screen (Receive's own address, with what's typed); it never makes a new address.
    fun share() { code?.let { context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, it.uri), null)) } }
    val qr = remember(code) { code?.let { runCatching { qrCode(it.uri) }.getOrNull() } }
    val groups = remember(shownAddress) { shownAddress?.let { addressGroups(it) } }
    // While the address comes, an unseen stand-in of the same length keeps its lines' room.
    val standIn = remember(wallet) { wallet?.let { addressGroups(it.primaryAddress()) } ?: emptyList() }
    val shownCode = code
    // Touch to pay, the card: whatever code is shown is what a phone on Send or Home reads, while NFC is on.
    val touch = rememberTouchState(model)
    val tick = rememberTick()
    rememberPayCard(touch, shownCode?.uri) { tick() }
    if (expanded && shownCode != null && qr != null) {
        ExpandedQr(shownCode, qr, shownAddress, { expanded = false })
        return
    }
    Box(Modifier.fillMaxSize()) {
        Page(bottom = { ErrorText(error) }) {
            TopBar("Receive", onBack = { nav.back() }) {
                // Shares the payment link for what's typed (and for whoever is chosen over the code).
                val ready = code != null && asked != null
                EdgeIconButton(Icons.share, "Share", { if (ready) share() }, tint = if (ready) Fob.ink else Fob.muted)
            }
            Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp)) {
                // The code on a white card as wide as the page (at most 360 dp of code, and a third of the screen's height); the round button on its
                // corner, or the code itself, opens it full screen. With the code's own 2-module quiet zone, 12 dp
                // leaves 4 modules or more of white beside it. The address's pattern sits across the card's top
                // edge, reaching into the quiet zone and stopping one module short of the code, whatever its module
                // size (a bare address's, version 6, until the code comes, so nothing moves as it does).
                BoxWithConstraints(Modifier.fillMaxWidth().padding(top = 8.dp), contentAlignment = Alignment.Center) {
                    // On a short screen the code gives way so Note and the buttons stay in view; it opens larger on tap.
                    val side = (maxWidth - 24.dp).coerceAtMost(360.dp).coerceAtMost((LocalConfiguration.current.screenHeightDp * 0.32f).dp)
                    val module = qrModule(qr?.size?.toInt() ?: 41, side)
                    Box(Modifier.padding(top = PatternBadgeSize / 2)) {
                        Box(
                            Modifier.clip(RoundedCornerShape(14.dp)).background(Fob.qrLight)
                                .clickable(enabled = qr != null, onClickLabel = "Show larger", role = Role.Button) { expanded = true }
                                .padding(
                                    start = 12.dp, end = 12.dp, bottom = 12.dp,
                                    top = PatternBadgeSize / 2 - module,
                                ),
                        ) {
                            if (qr != null) QrImage(qr, side) else Spacer(Modifier.size(side))
                        }
                        // The address's pattern and NFC, level on the card's top edge as one group, centred.
                        Row(
                            Modifier.align(Alignment.TopCenter).offset(y = -PatternBadgeSize / 2),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            if (qr != null && shownAddress != null) PatternBadge(shownAddress)
                            NfcBadge(touch, PatternBadgeSize, { if (touch.nfc == Nfc.OFF) openNfcSettings(context) else model.setPayNfc(true) })
                        }
                    }
                }
                AddressGroups(groups ?: standIn, shown = groups != null) {
                    shownAddress?.let { a -> scope.launch { clipboard.setClipEntry(ClipData.newPlainText("Monero address", a).toClipEntry()) } }
                }
                AmountField(
                    amount, { amount = it }, entry, placeholder = "Amount", showRate = false, optional = true,
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
                NoteField(note, { typed -> fitReceiveNote(typed).let { note = it.text; noteHint = it.hint } }, placeholder = "Note", hint = noteHint)
                WhoForField(forContact, loading = forId != null && forContact == null) { choosing = true }
            }
        }
        if (choosing) {
            WhoForPicker(
                people,
                onDismiss = { choosing = false },
                onPick = { p -> choosing = false; forId = p.id },
                onNew = {
                    choosing = false
                    // Back from New contact, Receive comes back filled, with them chosen.
                    nav.replace(Screen.Receive(forId, ReceiveDraft(heldMoney ?: amount, heldMoney != null || inFiat, note)))
                    nav.go(Screen.ContactEdit(forRequest = true))
                },
                onNoName = { choosing = false; forId = null; forContact = null },
            )
        }
    }
}

/**
 * "Who is it for?": a field in Amount's and Note's style above Amount; tapping opens the sheet. With someone chosen, their
 * avatar and name; otherwise the question in a placeholder's grey. [loading]: a contact Rust hasn't read yet, so the
 * field waits unseen rather than flashing "Who is it for?" and swapping.
 */
@Composable
private fun WhoForField(contact: Contact?, loading: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(60.dp).clip(RoundedCornerShape(16.dp)).background(Fob.card).optionalOutline(contact != null)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = contact?.let { "For ${it.name}. Change who it's for" } ?: "Who is it for? Choose a contact" }
            .padding(start = 12.dp, end = 16.dp).alpha(if (loading) 0f else 1f),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (contact != null) Avatar(contact.name, contact.initial, contact.photo, 36.dp)
        else Box(Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(Icons.person, tint = Fob.muted, size = 18.dp, width = 1.8f) }
        if (contact != null) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text("Who is it for?", color = Fob.muted, fontSize = 12.sp)
                Text(contact.name, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        } else {
            Text("Who is it for?", color = Fob.muted, fontSize = 17.sp, fontWeight = FontWeight.Medium, maxLines = 1, modifier = Modifier.weight(1f))
        }
        Box(Modifier.rotate(90f)) { StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp) }
    }
}

/**
 * "Who is it for?": a full-screen contact picker over Receive, the keyboard up at once. Contacts whose name or line
 * match what's typed, then New contact; No one in particular is pinned under the list.
 */
@Composable
private fun WhoForPicker(people: List<ContactRow>, onDismiss: () -> Unit, onPick: (ContactRow) -> Unit, onNew: () -> Unit, onNoName: () -> Unit) {
    BackHandler(onBack = onDismiss)
    var search by remember { mutableStateOf("") }
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    val shown = remember(people, search) { people.filter { search.isBlank() || it.name.contains(search.trim(), ignoreCase = true) || it.line.contains(search.trim(), ignoreCase = true) } }
    Column(Modifier.fillMaxSize().background(Fob.paper).safeDrawingPadding()) {
        TopBar("Who is it for?", onBack = onDismiss, closeIcon = true)
        Column(Modifier.weight(1f).padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            SearchField(search, { search = it }, "Search contacts", "Search contacts", Modifier.focusRequester(focus))
            Card(Modifier.weight(1f, fill = false)) {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    shown.forEach { p ->
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
                    if (shown.isEmpty() && search.isNotBlank()) Text("No contact matches", color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 16.dp))
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button, onClick = onNew).padding(horizontal = 20.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Box(Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(Icons.plus, size = 18.dp) }
                        Text("New contact", fontSize = 16.sp, fontWeight = FontWeight.Medium)
                    }
                }
            }
        }
        OutlineButton("No one in particular", onNoName, Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 16.dp), height = 52.dp)
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
}

/**
 * The address in full, in Rust's groups of five; the first, middle and last
 * stand out. Not [shown]: laid out unseen and unheard, keeping its room.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AddressGroups(groups: List<AddressGroup>, shown: Boolean = true, onCopy: () -> Unit) {
    FlowRow(
        Modifier.fillMaxWidth().then(
            if (shown) Modifier.clip(RoundedCornerShape(8.dp)).clickable(onClickLabel = "Copy address", role = Role.Button, onClick = onCopy).semantics(mergeDescendants = true) {}
            else Modifier.alpha(0f).clearAndSetSemantics {},
        ),
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        groups.forEach { g ->
            Text(
                g.text, fontFamily = FontFamily.Monospace, fontSize = 12.sp, lineHeight = 17.sp,
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
            Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 4.dp, vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        ) {
            if (code.headline != null || code.detail != null) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    // Rust writes both: "$20.00" over "0.1204 XMR · note" for money, "0.5 XMR" over the note for XMR.
                    code.headline?.let { Text(it, style = display(34), color = Fob.qrDark, textAlign = TextAlign.Center) }
                    code.detail?.let { Text(it, color = Fob.qrMuted, fontSize = 16.sp, textAlign = TextAlign.Center) }
                }
            }
            // As wide as the page lets it be: this is the view for scanning, so it is never smaller than Receive's. As on
            // Receive's card, the address's pattern sits across the top edge, ringed in the page's white and scaled by how
            // much larger this code is than the card's, so it keeps its proportion. It reaches into the code's quiet zone
            // (3 modules here) and stops one module short of the code; where that
            // takes more than the zone, white is added above the code.
            BoxWithConstraints(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                val side = this.maxWidth.coerceAtMost(720.dp)
                val config = LocalConfiguration.current
                val cardSide = (config.screenWidthDp.dp - 40.dp - 24.dp).coerceAtMost(360.dp).coerceAtMost((config.screenHeightDp * 0.32f).dp)
                val scale = (side / cardSide).coerceIn(1f, 2.2f)
                val quiet = 3
                val module = side / (qr.size.toInt() + 2 * quiet)
                val half = PatternBadgeSize * scale / 2
                val extra = (half - module * (quiet - 1)).coerceAtLeast(0.dp)
                val onTop = address != null
                val reserve = if (onTop) half else 0.dp
                val density = LocalDensity.current
                Box(Modifier.padding(top = reserve)) {
                    Box(Modifier.padding(top = if (onTop) extra else 0.dp)) { QrImage(qr, side, quiet = quiet) }
                    // The identicon across the code's top edge, ringed in the page's white and scaled like the code.
                    address?.let {
                        CompositionLocalProvider(LocalDensity provides Density(density.density * scale, density.fontScale)) {
                            PatternBadge(it, Modifier.align(Alignment.TopCenter).offset(y = -PatternBadgeSize / 2), ring = Fob.qrLight, inverted = true)
                        }
                    }
                }
            }
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

/** Note, in Amount's style: an icon and the text on a 60 dp card, its outline dashed until something is written; Rust's hint under it. */
@Composable
private fun NoteField(value: String, onChange: (String) -> Unit, placeholder: String, hint: String?) {
    val style = androidx.compose.ui.text.TextStyle(fontSize = 17.sp, fontWeight = FontWeight.Medium, color = Fob.ink)
    // Focused, the whole field and its hint show above the keyboard once it is up, as Amount's do.
    val whole = remember { androidx.compose.foundation.relocation.BringIntoViewRequester() }
    val scope = rememberCoroutineScope()
    Column(Modifier.bringIntoViewRequester(whole), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            Modifier.fillMaxWidth().height(60.dp).clip(RoundedCornerShape(16.dp)).background(Fob.card).optionalOutline(value.isNotBlank()).padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            StrokeIcon(Icons.lines, tint = Fob.muted, size = 20.dp, width = 1.8f)
            androidx.compose.foundation.text.BasicTextField(
                value = value, onValueChange = onChange, singleLine = true, textStyle = style,
                cursorBrush = androidx.compose.ui.graphics.SolidColor(Fob.ink),
                keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(capitalization = androidx.compose.ui.text.input.KeyboardCapitalization.Sentences),
                modifier = Modifier.weight(1f).semantics { contentDescription = "Note, what it's for, shown to the payer" }
                    .onFocusChanged { if (it.isFocused) scope.launch { kotlinx.coroutines.delay(350); whole.bringIntoView() } },
                decorationBox = { field ->
                    Box(contentAlignment = Alignment.CenterStart) {
                        if (value.isEmpty()) Text(placeholder, style = style.copy(color = Fob.muted))
                        field()
                    }
                },
            )
        }
        if (hint != null) Text(hint, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 4.dp))
    }
}
