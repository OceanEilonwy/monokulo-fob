package com.monokulo.fob.ui.screens

import androidx.compose.ui.graphics.lerp
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.Animatable
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.AddressPattern
import com.monokulo.fob.ui.Avatar
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.EdgeButton
import com.monokulo.fob.ui.EdgeIconButton
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.SearchField
import com.monokulo.fob.ui.PhotoButton
import com.monokulo.fob.ui.PhotoSheet
import com.monokulo.fob.ui.rememberPhotoChooser
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.Warning
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.launch
import uniffi.fob.ContactCard
import uniffi.fob.ContactDraft
import uniffi.fob.ContactRow
import uniffi.fob.FoundAddress
import uniffi.fob.Network
import uniffi.fob.PaidBefore
import uniffi.fob.contactAddress
import uniffi.fob.contactEditTexts
import uniffi.fob.contactInitial
import uniffi.fob.contactPaidAtNote
import uniffi.fob.foundSendAddress
import uniffi.fob.sendAddressScan

/**
 * Contacts (Round 4 › "Contacts"): the search, everyone saved by name with
 * a line about the last payment, then the addresses paid but not saved,
 * each with Save. Every line is Rust's.
 */
@Composable
fun ContactsScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    var search by rememberSaveable { mutableStateOf("") }
    // Everyone and "Paid, not saved" are the model's, read with the wallet, so the screen opens whole; only a
    // search asks Rust as it's typed, the last answer kept meanwhile.
    val all by model.contactList.collectAsState()
    val unsaved by model.unsaved.collectAsState()
    var found by remember { mutableStateOf<List<ContactRow>?>(null) }
    LaunchedEffect(all, search) { found = if (search.isBlank()) null else model.contacts(search) }
    val any = !all.isNullOrEmpty()
    val saved = if (search.isBlank()) all else found
    val scope = rememberCoroutineScope()
    Page {
        TopBar("Contacts", onBack = { nav.back() }) {
            // Another Fob's profile: add them in one scan.
            EdgeIconButton(Icons.scan, "Scan a profile", { nav.go(Screen.ProfileScan) })
            EdgeIconButton(Icons.plus, "Add a contact", { nav.go(Screen.ContactEdit()) })
        }
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            if (any) SearchField(search, { search = it }, "Search names and notes", "Search contacts")
            val rows = saved
            when {
                rows == null -> {}
                rows.isNotEmpty() -> Card { rows.forEach { c -> ContactListRow(c) { nav.go(Screen.Contact(c.id)) } } }
                any -> Text("No contact has that in its name or note.", color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 8.dp))
                else -> Card(padding = PaddingValues(horizontal = 20.dp, vertical = 16.dp)) {
                    Text(
                        if (unsaved.isEmpty()) "No contacts yet. Tap + to add someone you pay." else "No contacts yet. Tap + to add someone you pay, or save someone you paid below.",
                        color = Fob.muted, fontSize = 14.sp,
                    )
                }
            }
            if (unsaved.isNotEmpty()) {
                Text(
                    "Paid, not saved", color = Fob.muted, fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.padding(start = 8.dp, top = 4.dp).semantics { heading() },
                )
                Card {
                    unsaved.forEach { p ->
                        // Filled as the Paid screen would have been: a code's name makes it an address they were paid at.
                        UnsavedRow(p) { scope.launch { wallet?.newContactFor(p.address)?.let { nav.go(Screen.ContactEdit(prefill = it)) } } }
                    }
                }
            }
        }
    }
}

/** A saved contact: avatar, name, Rust's line and a chevron. */
@Composable
private fun ContactListRow(c: ContactRow, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) {}.padding(horizontal = 20.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Avatar(c.name, c.initial, c.photo, 36.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(c.name, fontSize = 16.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(c.line, color = Fob.muted, fontSize = 12.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
    }
}

/** An address paid that's no contact's: its pattern, its ends, the last payment, and Save. */
@Composable
private fun UnsavedRow(p: PaidBefore, onSave: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).padding(start = 20.dp, end = 12.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        AddressPattern(p.address, 36.dp)
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(p.name, fontFamily = FontFamily.Monospace, fontSize = 14.sp, maxLines = 1)
            Text(p.line, color = Fob.muted, fontSize = 12.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Chip("Save", onSave, Modifier.semantics { contentDescription = "Save ${p.name} as a contact" })
    }
}

/** A small sunken button, 40 dp drawn in a 48 dp touch area. */
@Composable
private fun Chip(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, highlight: Float = 0f) {
    Box(
        modifier.minimumInteractiveComponentSize().height(40.dp).clip(RoundedCornerShape(20.dp)).background(lerp(Fob.sunken, Fob.orange, highlight))
            .clickable(role = Role.Button, onClick = onClick).padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) { Text(text, color = lerp(Fob.ink, Fob.onOrange, highlight), fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
}

/** A white section of a contact's card, rounder-cornered than a [Card] as the board's. */
@Composable
private fun Section(label: String, content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card).semantics { contentDescription = label }.padding(vertical = 4.dp),
        content = content,
    )
}

/** A section's heading, with a quieter note at its end. */
@Composable
private fun SectionHead(title: String, aside: String? = null) {
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 2.dp)) {
        Text(title, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f).semantics { heading() })
        aside?.let { Text(it, color = Fob.muted, fontSize = 14.sp) }
    }
}

/**
 * A contact's card (Round 5 › "Sam: send address, requests, also paid
 * at"): who they are, Send (to their send address) and Request (Receive
 * with them chosen), the send address with Change, the requests they pay,
 * the addresses of codes named for them that were paid, and every payment
 * to and from them. Edit in the top bar.
 */
@Composable
fun ContactScreen(model: WalletModel, nav: Nav, id: ULong) {
    val wallet by model.wallet.collectAsState()
    val activity by model.activity.collectAsState()
    var card by remember { mutableStateOf<ContactCard?>(null) }
    LaunchedEffect(wallet, activity) {
        if (wallet == null) return@LaunchedEffect
        val shown = model.contactCard(id)
        // Deleted elsewhere: nothing to show.
        if (shown == null && nav.current == Screen.Contact(id)) nav.back() else card = shown
    }
    // Send tapped with no send address to send to: the Add chip lights at once, then fades out.
    val addGlow = remember { Animatable(0f) }
    val scope = rememberCoroutineScope()
    val c = card ?: return Page { TopBar(null, onBack = { nav.back() }) }
    val contact = c.contact
    Page {
        TopBar(null, onBack = { nav.back() }) { TextButton("Edit", { nav.go(Screen.ContactEdit(id)) }) }
        Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                Avatar(contact.name, contact.initial, contact.photo, 56.dp, describe = true)
                Column(Modifier.weight(1f)) {
                    Text(contact.name, style = display(24), modifier = Modifier.semantics { heading() })
                    contact.note?.let { Text(it, color = Fob.muted, fontSize = 14.sp) }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                val sendTo = contact.sendAddress
                // Without a send address Send is grey but still tappable: it points at Add instead of opening Send.
                PrimaryButton(
                    "Send", { if (sendTo != null) nav.go(Screen.Send(draft = SendDraft(to = sendTo))) else scope.launch { addGlow.snapTo(1f); addGlow.animateTo(0f, tween(1200)) } },
                    Modifier.weight(1f), dim = sendTo == null, height = 52.dp, leading = Icons.up,
                )
                // Receive with them chosen; the request is made only on Share.
                OutlineButton("Request", { nav.go(Screen.Receive(contact = id)) }, Modifier.weight(1f), height = 52.dp, leading = Icons.down)
            }
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card).padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                contact.sendAddress?.let { AddressPattern(it, 36.dp) }
                Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(1.dp)) {
                    Text(c.sendLabel, color = Fob.muted, fontSize = 13.sp)
                    c.sendShort?.let { Text(it, fontFamily = FontFamily.Monospace, fontSize = 14.sp, maxLines = 1) }
                }
                Chip(c.sendButton, { nav.go(Screen.ContactScan(id)) }, highlight = addGlow.value)
            }
            if (c.requests.isNotEmpty()) {
                Section("Requests") {
                    SectionHead(c.requestsTitle, c.requestsNote)
                    c.requests.forEach { r ->
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button) { nav.go(Screen.Request(r.id)) }
                                .semantics(mergeDescendants = true) {}.padding(horizontal = 16.dp, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            AddressPattern(r.address, 30.dp)
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                                Text(r.note, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text(r.line, color = Fob.muted, fontSize = 12.sp)
                            }
                            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(1.dp)) {
                                Text(r.state, fontSize = 14.sp, fontWeight = FontWeight.Medium, color = if (r.received) Fob.green else Fob.muted)
                                r.asked?.let { Text(it, color = Fob.muted, fontSize = 12.sp) }
                            }
                        }
                    }
                }
            }
            if (c.alsoPaidAt.isNotEmpty()) {
                Section("Also paid at") {
                    SectionHead("Also paid at", c.alsoNote)
                    c.alsoPaidAt.forEach { a ->
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 52.dp).semantics(mergeDescendants = true) {}.padding(horizontal = 16.dp, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            AddressPattern(a.address, 30.dp)
                            Column(Modifier.weight(1f)) {
                                Text(a.short, fontFamily = FontFamily.Monospace, fontSize = 13.5.sp, maxLines = 1)
                                Text(a.line, color = Fob.muted, fontSize = 12.sp)
                            }
                        }
                    }
                }
            }
            Section(c.paidTitle) {
                SectionHead(c.paidTitle)
                c.paidEmpty?.let { Text(it, color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp)) }
                c.paid.forEach { p ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 52.dp)
                            .clickable(role = Role.Button) { nav.go(if (p.activity.receipt != null) Screen.Receipt(p.activity) else Screen.Payment(p.activity)) }
                            .semantics(mergeDescendants = true) {}.padding(horizontal = 16.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(p.title, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(p.`when`, color = Fob.muted, fontSize = 12.sp)
                        }
                        Column(horizontalAlignment = Alignment.End) {
                            // From them, to a request: green, as in Activity.
                            Text(p.amountText, fontSize = 15.sp, fontWeight = FontWeight.Medium, color = if (p.activity.outgoing) Fob.ink else Fob.green)
                            p.fiatText?.let { Text(it, color = Fob.muted, fontSize = 12.sp) }
                        }
                    }
                }
            }
        }
    }
}

/**
 * The edit card (Round 5 › "Edit: photo, name, note, send address"), or
 * New contact: the photo (taken, chosen or removed, then made small for
 * Rust), name, note and send address (scanned or pasted, optional). Save
 * in the top bar; Delete contact asks first.
 */
@Composable
fun ContactEditScreen(model: WalletModel, nav: Nav, id: ULong?, prefill: ContactDraft?, forRequest: Boolean = false) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboard.current
    var loaded by remember { mutableStateOf(id == null) }
    var name by remember { mutableStateOf(prefill?.name ?: "") }
    var note by remember { mutableStateOf(prefill?.note ?: "") }
    var photo by remember { mutableStateOf(prefill?.photo) }
    var address by remember { mutableStateOf(prefill?.sendAddress ?: "") }
    val alsoPaidAt = prefill?.alsoPaidAt ?: emptyList()
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var choosing by remember { mutableStateOf(false) }
    var confirming by remember { mutableStateOf(false) }
    var scanning by remember { mutableStateOf(false) }
    // A pasted address that would replace the one set, waiting to be confirmed.
    var pasted by remember { mutableStateOf<FoundAddress?>(null) }
    LaunchedEffect(wallet, id) {
        if (id == null || loaded) return@LaunchedEffect
        val w = wallet ?: return@LaunchedEffect
        val c = w.contact(id) ?: run { nav.back(); return@LaunchedEffect }
        name = c.name; note = c.note ?: ""; photo = c.photo; address = c.sendAddress ?: ""
        loaded = true
    }
    val texts = remember(name, id) { contactEditTexts(name, id == null) }

    val chooser = rememberPhotoChooser { made -> if (made != null) { photo = made; error = null } else error = "That photo couldn't be made small enough to keep." }

    fun save() {
        busy = true
        error = null
        scope.launch {
            val draft = ContactDraft(name, note.ifBlank { null }, photo, address.ifBlank { null }, alsoPaidAt)
            runCatching { model.changeContacts { w -> if (id == null) w.addContact(draft) else w.editContact(id, draft) } }
                // A new contact opens its card (or, made from Share's sheet, goes back to Receive for them); an edited one goes back to it.
                .onSuccess { c ->
                    nav.back()
                    val receive = nav.current as? Screen.Receive
                    when {
                        forRequest && receive != null -> nav.replace(receive.copy(contact = c.id))
                        id == null -> nav.go(Screen.Contact(c.id))
                    }
                }
                .onFailure { error = it.reason() }
            busy = false
        }
    }

    fun paste() {
        scope.launch {
            val text = clipboard.getClipEntry()?.clipData?.getItemAt(0)?.text?.toString()
            if (text.isNullOrBlank()) { error = "Nothing to paste."; return@launch }
            runCatching { foundSendAddress(network, name.ifBlank { null }, address.ifBlank { null }, text) }
                .onSuccess { f -> error = null; if (f.warning != null) pasted = f else address = f.address }
                .onFailure { error = it.reason() }
        }
    }

    if (scanning) {
        SendAddressScanner(network, name.ifBlank { null }, address.ifBlank { null }, onUse = { address = it; scanning = false; error = null }, onClose = { scanning = false })
        return
    }
    Box(Modifier.fillMaxSize()) {
        Page {
            TopBar(texts.title, onBack = { nav.back() }, closeIcon = true) {
                Box(
                    Modifier.minimumInteractiveComponentSize().height(44.dp).clip(RoundedCornerShape(22.dp))
                        .background(if (loaded && !busy) Fob.ink else Fob.sunken).clickable(enabled = loaded && !busy, role = Role.Button) { save() }
                        .padding(horizontal = 16.dp),
                    contentAlignment = Alignment.Center,
                ) { Text("Save", color = if (loaded && !busy) Fob.paper else Fob.muted, fontSize = 15.sp, fontWeight = FontWeight.SemiBold) }
            }
            Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Column(Modifier.fillMaxWidth().padding(vertical = 4.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    PhotoButton(name, remember(name) { contactInitial(name) }, photo, 88.dp) { choosing = true }
                    Text(if (photo != null) "Tap to change" else "Add a photo", color = Fob.muted, fontSize = 13.sp)
                }
                LabelledInput("Name", name, { name = it }, "Who this is", fontSize = 17, capitalization = KeyboardCapitalization.Words)
                LabelledInput("Note (optional)", note, { note = it }, "What you pay them for")
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Send address (where you pay them)", color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(start = 4.dp))
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 60.dp).clip(RoundedCornerShape(16.dp)).background(Fob.card)
                            .border(1.dp, Fob.outline, RoundedCornerShape(16.dp)).padding(start = 12.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        if (address.isNotBlank()) {
                            AddressPattern(address, 30.dp)
                            Text(remember(address) { contactAddress(address) }, fontFamily = FontFamily.Monospace, fontSize = 14.sp, maxLines = 1, modifier = Modifier.weight(1f))
                        } else {
                            Text("None yet, optional", color = Fob.muted, fontSize = 15.sp, modifier = Modifier.weight(1f))
                        }
                        Box(
                            Modifier.size(44.dp).clip(RoundedCornerShape(12.dp)).background(Fob.sunken).clickable(role = Role.Button) { scanning = true }
                                .semantics { contentDescription = "Scan their code" },
                            contentAlignment = Alignment.Center,
                        ) { StrokeIcon(Icons.scan, size = 20.dp) }
                        Box(
                            Modifier.height(44.dp).clip(RoundedCornerShape(12.dp)).background(Fob.sunken).clickable(role = Role.Button, onClick = ::paste)
                                .padding(horizontal = 12.dp),
                            contentAlignment = Alignment.Center,
                        ) { Text("Paste", fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
                    }
                    Text(texts.addressHint, color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, modifier = Modifier.padding(start = 4.dp))
                    // Saved after paying a code named for them: that address stays theirs, but not to pay.
                    remember(alsoPaidAt) { contactPaidAtNote(alsoPaidAt) }?.let {
                        Text(it, color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, modifier = Modifier.padding(start = 4.dp))
                    }
                }
                ErrorText(error)
                if (id != null) {
                    OutlineButton("Delete contact", { confirming = true }, Modifier.fillMaxWidth(), height = 52.dp, leading = Icons.trash, tint = Fob.red, edge = Fob.red)
                }
            }
        }
        if (choosing) {
            PhotoSheet("Contact photo", photo != null, chooser, onRemove = { photo = null }, onDismiss = { choosing = false })
        }
        if (confirming && id != null) {
            BottomSheet(texts.deleteTitle, onDismiss = { confirming = false }, background = Fob.card) {
                Column(Modifier.padding(start = 4.dp, end = 4.dp, top = 14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(texts.deleteTitle, style = display(22), modifier = Modifier.semantics { heading() })
                    Text(texts.deleteBody, color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp)
                    Box(
                        Modifier.fillMaxWidth().height(52.dp).clip(RoundedCornerShape(26.dp)).background(Fob.red).clickable(role = Role.Button) {
                            confirming = false
                            scope.launch {
                                runCatching { model.changeContacts { it.deleteContact(id) } }
                                    .onSuccess { nav.back(); if (nav.current == Screen.Contact(id)) nav.back() }
                                    .onFailure { error = it.reason() }
                            }
                        },
                        contentAlignment = Alignment.Center,
                    ) { Text("Delete", color = Fob.onRed, fontSize = 16.sp, fontWeight = FontWeight.SemiBold) }
                    OutlineButton("Keep", { confirming = false }, Modifier.fillMaxWidth(), height = 52.dp)
                }
            }
        }
        pasted?.let { f ->
            BottomSheet(f.title, onDismiss = { pasted = null }) {
                SheetHandle()
                FoundContent(f, "Cancel", onOther = { pasted = null }, onUse = { address = f.address; pasted = null })
            }
        }
    }
}

/** A labelled single-line field, as on Send. */
@Composable
internal fun LabelledInput(
    label: String, value: String, onChange: (String) -> Unit, placeholder: String, fontSize: Int = 16,
    capitalization: KeyboardCapitalization = KeyboardCapitalization.Sentences,
) {
    var focused by remember { mutableStateOf(false) }
    val style = TextStyle(fontSize = fontSize.sp, color = Fob.ink)
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(start = 4.dp))
        BasicTextField(
            value = value, onValueChange = onChange, singleLine = true, textStyle = style, cursorBrush = SolidColor(Fob.ink),
            keyboardOptions = KeyboardOptions(capitalization = capitalization),
            modifier = Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused }.semantics { contentDescription = label },
            decorationBox = { field ->
                Box(
                    Modifier.fillMaxWidth().height(56.dp).clip(RoundedCornerShape(16.dp)).background(Fob.card)
                        .border(1.dp, if (focused) Fob.ink else Fob.outline, RoundedCornerShape(16.dp)).padding(horizontal = 16.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    if (value.isEmpty()) Text(placeholder, style = style.copy(color = Fob.muted))
                    field()
                }
            },
        )
    }
}

/** What was found for a send address: its pattern and ends, a warning if it replaces one, and the two choices. */
@Composable
private fun FoundContent(f: FoundAddress, other: String, onOther: () -> Unit, onUse: () -> Unit) {
    Column(Modifier.padding(start = 4.dp, end = 4.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            AddressPattern(f.address, 48.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(f.title, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text(f.short, fontFamily = FontFamily.Monospace, fontSize = 13.5.sp, color = Fob.muted, maxLines = 1)
            }
        }
        f.warning?.let {
            Warning(it)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            OutlineButton(other, onOther, Modifier.weight(1f), height = 52.dp)
            PrimaryButton("Use it", onUse, Modifier.weight(1f), height = 52.dp)
        }
    }
}

/**
 * Scanning a send address (Round 5 › "Scan their code for the send
 * address"): the camera full screen, the close button and title below the
 * status bar, what was found on a sheet above the gesture bar. An address
 * or a payment code (its address is taken) is found; anything else says
 * why not. [current] is the address it would replace.
 */
@Composable
fun SendAddressScanner(network: Network, name: String?, current: String?, onUse: (String) -> Unit, onClose: () -> Unit, error: String? = null) {
    val texts = remember(name) { sendAddressScan(name) }
    var found by remember { mutableStateOf<FoundAddress?>(null) }
    var notice by remember { mutableStateOf<String?>(null) }
    // The camera reads a code many times a second: each one is taken once.
    var lastRead by remember { mutableStateOf<String?>(null) }
    BackHandler(onBack = onClose)
    Box(Modifier.fillMaxSize().background(Fob.camera)) {
        CameraOrAsk(Modifier.fillMaxSize(), onText = { text ->
            if (found == null && text != lastRead) {
                lastRead = text
                runCatching { foundSendAddress(network, name, current, text) }
                    .onSuccess { found = it; notice = null }
                    .onFailure { notice = it.reason() }
            }
        })
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                EdgeIconButton(Icons.close, "Close", onClose, tint = Fob.onCamera)
                Text(texts.title, color = Fob.onCamera, fontSize = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).semantics { heading() })
            }
            if (found == null) {
                BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
                    val side: Dp = minOf(260.dp, maxWidth - 48.dp, maxHeight - 140.dp).coerceAtLeast(0.dp)
                    Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                        if (side > 24.dp) Viewfinder(side, 6f)
                        Text(texts.hint, color = Fob.onCamera, fontSize = 15.sp, lineHeight = 21.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 24.dp, vertical = 16.dp))
                        notice?.let { Text(it, color = Fob.cameraWarning, fontSize = 14.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 24.dp)) }
                    }
                }
            }
        }
        found?.let { f ->
            Column(
                Modifier.align(Alignment.BottomCenter).fillMaxWidth().clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(Fob.paper)
                    .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
                    .padding(start = 16.dp, end = 16.dp, top = 14.dp, bottom = 20.dp)
                    .semantics { contentDescription = "Found" },
            ) {
                FoundContent(f, "Scan again", onOther = { found = null; lastRead = null }, onUse = { onUse(f.address) })
                // Why keeping it didn't work: the sheet stays.
                ErrorText(error)
            }
        }
    }
}

/** Change on a contact's card: scan their send address and keep it at once. */
@Composable
fun ContactScanScreen(model: WalletModel, nav: Nav, id: ULong) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val scope = rememberCoroutineScope()
    var card by remember { mutableStateOf<ContactCard?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(wallet) { if (wallet != null) card = model.contactCard(id) ?: run { nav.back(); null } }
    val contact = card?.contact ?: return Box(Modifier.fillMaxSize().background(Fob.camera))
    SendAddressScanner(network, contact.name, contact.sendAddress, onUse = { address ->
        error = null
        scope.launch {
            // Kept: back to the card. Refused (deleted meanwhile, or not saved): say so and stay.
            runCatching { model.changeContacts { it.setContactSendAddress(id, address) } }
                .onSuccess { if (nav.current == Screen.ContactScan(id)) nav.back() }
                .onFailure { error = it.reason() }
        }
    }, onClose = { nav.back() }, error = error)
}
