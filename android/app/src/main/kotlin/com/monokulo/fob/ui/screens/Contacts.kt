package com.monokulo.fob.ui.screens

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
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
import androidx.core.content.ContextCompat
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.AddressPattern
import com.monokulo.fob.ui.Avatar
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
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
import uniffi.fob.contactPhotoFits
import uniffi.fob.foundSendAddress
import uniffi.fob.sendAddressScan
import java.io.ByteArrayOutputStream

/** A photo's sides tried in turn, and the WebP qualities at each, until Rust keeps it (decision 4 of the plan). */
private val PHOTO_SIZES = listOf(96, 72, 64)
private val PHOTO_QUALITIES = listOf(80, 70, 60, 50, 40, 30, 20)

/**
 * Contacts (Round 4 › "Contacts"): the search, everyone saved by name with
 * a line about the last payment, then the addresses paid but not saved,
 * each with Save. Every line is Rust's.
 */
@Composable
fun ContactsScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val activity by model.activity.collectAsState()
    var search by rememberSaveable { mutableStateOf("") }
    var saved by remember { mutableStateOf<List<ContactRow>?>(null) }
    var any by remember { mutableStateOf(false) }
    var unsaved by remember { mutableStateOf<List<PaidBefore>>(emptyList()) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(wallet, activity, search) {
        val found = model.contacts(search)
        any = if (search.isBlank()) found.isNotEmpty() else model.contacts().isNotEmpty()
        saved = found
        unsaved = model.paidNotSaved()
    }
    Page {
        TopBar("Contacts", onBack = { nav.back() }) {
            IconButton(onClick = { nav.go(Screen.ContactEdit()) }, modifier = Modifier.semantics { contentDescription = "Add a contact" }) {
                StrokeIcon(Icons.plus, size = 22.dp)
            }
        }
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            if (any) SearchField(search) { search = it }
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
            Text(
                "Contacts live in this wallet's file, on this phone only. Monero doesn't say who paid you, so contacts are for sending.",
                color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, modifier = Modifier.padding(horizontal = 8.dp),
            )
        }
    }
}

/** Search names and notes, on a sunken pill. */
@Composable
private fun SearchField(value: String, onChange: (String) -> Unit) {
    val style = TextStyle(fontSize = 16.sp, color = Fob.ink)
    Row(
        Modifier.fillMaxWidth().height(48.dp).clip(RoundedCornerShape(24.dp)).background(Fob.sunken).padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        StrokeIcon(Icons.search, tint = Fob.muted, size = 18.dp)
        BasicTextField(
            value = value, onValueChange = onChange, singleLine = true, textStyle = style, cursorBrush = SolidColor(Fob.ink),
            modifier = Modifier.weight(1f).semantics { contentDescription = "Search contacts" },
            decorationBox = { field ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (value.isEmpty()) Text("Search names and notes", style = style.copy(color = Fob.muted))
                    field()
                }
            },
        )
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
private fun Chip(text: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Box(
        modifier.minimumInteractiveComponentSize().height(40.dp).clip(RoundedCornerShape(20.dp)).background(Fob.sunken)
            .clickable(role = Role.Button, onClick = onClick).padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) { Text(text, color = Fob.ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
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
                PrimaryButton("Send", {
                    nav.go(contact.sendAddress?.let { Screen.Send(draft = SendDraft(to = it)) } ?: Screen.Send())
                }, Modifier.weight(1f), height = 52.dp, leading = Icons.up)
                // Receive with them chosen; the request is made only on Share or Copy.
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
                Chip(c.sendButton, { nav.go(Screen.ContactScan(id)) })
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
    val context = LocalContext.current
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

    fun usePhoto(bitmap: Bitmap?) {
        bitmap ?: return
        scope.launch {
            val made = withContext(Dispatchers.Default) { contactPhoto(bitmap) }
            if (made != null) { photo = made; error = null } else error = "That photo couldn't be made small enough to keep."
        }
    }
    val takePhoto = rememberLauncherForActivityResult(ActivityResultContracts.TakePicturePreview()) { usePhoto(it) }
    // Fob holds the camera permission, so the system camera needs it granted too.
    val askCamera = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { if (it) takePhoto.launch(null) }
    val pickPhoto = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        scope.launch { usePhoto(withContext(Dispatchers.IO) { decode(context, uri) }) }
    }

    fun save() {
        busy = true
        error = null
        scope.launch {
            val draft = ContactDraft(name, note.ifBlank { null }, photo, address.ifBlank { null }, alsoPaidAt)
            runCatching { model.changeContacts { w -> if (id == null) w.addContact(draft) else w.editContact(id, draft) } }
                // A new contact opens its card (or, made from Share's sheet, goes back to Receive for them and
                // shares); an edited one goes back to it.
                .onSuccess { c ->
                    nav.back()
                    val receive = nav.current as? Screen.Receive
                    when {
                        forRequest && receive != null -> nav.replace(receive.copy(contact = c.id).also { it.shareOnOpen = true })
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
                    Modifier.padding(end = 4.dp).minimumInteractiveComponentSize().height(44.dp).clip(RoundedCornerShape(22.dp))
                        .background(if (loaded && !busy) Fob.ink else Fob.sunken).clickable(enabled = loaded && !busy, role = Role.Button) { save() }
                        .padding(horizontal = 16.dp),
                    contentAlignment = Alignment.Center,
                ) { Text("Save", color = if (loaded && !busy) Fob.paper else Fob.muted, fontSize = 15.sp, fontWeight = FontWeight.SemiBold) }
            }
            Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Column(Modifier.fillMaxWidth().padding(vertical = 4.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Box(
                        Modifier.size(88.dp).clickable(role = Role.Button, indication = null, interactionSource = null) { choosing = true }
                            .semantics(mergeDescendants = true) { contentDescription = if (photo != null) "Change photo" else "Add a photo" },
                    ) {
                        Avatar(name, remember(name) { contactInitial(name) }, photo, 88.dp)
                        // The camera badge on the circle's corner.
                        Box(
                            Modifier.align(Alignment.BottomEnd).offset(2.dp, 2.dp).size(32.dp).clip(CircleShape).background(Fob.ink)
                                .border(3.dp, Fob.paper, CircleShape),
                            contentAlignment = Alignment.Center,
                        ) { StrokeIcon(Icons.camera, tint = Fob.paper, size = 14.dp, width = 2.2f) }
                    }
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
            BottomSheet("Contact photo", onDismiss = { choosing = false }) {
                SheetHandle()
                Text("Photo", style = display(20), modifier = Modifier.padding(start = 4.dp, top = 4.dp, bottom = 2.dp).semantics { heading() })
                Card {
                    SheetRow(Icons.camera, "Take a photo") {
                        choosing = false
                        if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) takePhoto.launch(null)
                        else askCamera.launch(Manifest.permission.CAMERA)
                    }
                    SheetRow(Icons.picture, "Choose from your photos") {
                        choosing = false
                        pickPhoto.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                    }
                    if (photo != null) SheetRow(Icons.trash, "Remove photo", tint = Fob.red) { choosing = false; photo = null }
                }
                Text(
                    "Cropped to a circle and kept small, inside this wallet's file on this phone. Fob never sends it anywhere.",
                    color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, modifier = Modifier.padding(horizontal = 6.dp),
                )
            }
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
private fun LabelledInput(
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

/** A row of the photo sheet: an icon and what it does. */
@Composable
private fun SheetRow(icon: String, text: String, tint: Color = Fob.ink, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 20.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        StrokeIcon(icon, tint = tint, size = 22.dp)
        Text(text, color = tint, fontSize = 16.sp)
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
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(Fob.highlight).padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                StrokeIcon(Icons.warning, tint = Fob.orangeText, size = 18.dp)
                Text(it, fontSize = 13.5.sp, lineHeight = 19.sp)
            }
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
fun SendAddressScanner(network: Network, name: String?, current: String?, onUse: (String) -> Unit, onClose: () -> Unit) {
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
            Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Box(
                    Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button, onClick = onClose).semantics { contentDescription = "Close" },
                    contentAlignment = Alignment.Center,
                ) { StrokeIcon(Icons.close, tint = Fob.onCamera, size = 20.dp) }
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
    LaunchedEffect(wallet) { if (wallet != null) card = model.contactCard(id) ?: run { nav.back(); null } }
    val contact = card?.contact ?: return Box(Modifier.fillMaxSize().background(Fob.camera))
    SendAddressScanner(network, contact.name, contact.sendAddress, onUse = { address ->
        scope.launch {
            runCatching { model.changeContacts { it.setContactSendAddress(id, address) } }
            if (nav.current == Screen.ContactScan(id)) nav.back()
        }
    }, onClose = { nav.back() })
}

/** A picked photo, decoded small enough to crop (and turned as the photo says, from Android 9). */
private fun decode(context: Context, uri: Uri): Bitmap? = runCatching {
    if (Build.VERSION.SDK_INT >= 28) {
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, uri)) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.setTargetSampleSize((minOf(info.size.width, info.size.height) / 384).coerceAtLeast(1))
        }
    } else {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        val sample = Integer.highestOneBit((minOf(bounds.outWidth, bounds.outHeight) / 384).coerceAtLeast(1))
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample }) }
    }
}.getOrNull()

/**
 * A contact's photo from [source]: cropped to a centred square, scaled to
 * 96 × 96 and encoded by Android's WebP encoder, the quality stepped down
 * (then the size, to 72 and 64) until Rust keeps it; null if it never does.
 */
private fun contactPhoto(source: Bitmap): ByteArray? {
    val side = minOf(source.width, source.height)
    if (side <= 0) return null
    val square = Bitmap.createBitmap(source, (source.width - side) / 2, (source.height - side) / 2, side, side)
    val format = if (Build.VERSION.SDK_INT >= 30) Bitmap.CompressFormat.WEBP_LOSSY else @Suppress("DEPRECATION") Bitmap.CompressFormat.WEBP
    for (size in PHOTO_SIZES) {
        val scaled = Bitmap.createScaledBitmap(square, size, size, true)
        for (quality in PHOTO_QUALITIES) {
            val bytes = ByteArrayOutputStream().also { scaled.compress(format, quality, it) }.toByteArray()
            if (contactPhotoFits(bytes)) return bytes
        }
    }
    return null
}
