package com.monokulo.fob.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
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
import com.monokulo.fob.ui.AmountField
import com.monokulo.fob.ui.Avatar
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.launch
import uniffi.fob.AmountEntry
import uniffi.fob.CodeKind
import uniffi.fob.ContactRow
import uniffi.fob.PaidBefore
import uniffi.fob.PaymentRequest
import uniffi.fob.classifyCode
import uniffi.fob.parsePayment
import uniffi.fob.shortAddress

/** A torch: the lamp's head and its handle. */
private const val TORCH = "M8 3h8l-2 6h-4L8 3Z M10 9v11h4V9"
/** A clipboard. */
private const val CLIPBOARD = "M9 4h6v3H9z M7 5H5v15h14V5h-2"
/** The viewfinder's four corners, on a 280 × 280 grid. */
private const val CORNERS = "M8 64V24a16 16 0 0 1 16-16h40 M216 8h40a16 16 0 0 1 16 16v40 M272 216v40a16 16 0 0 1-16 16h-40 M64 272H24a16 16 0 0 1-16-16v-40"

/**
 * Send's form as it was filled: kept on the back stack under Confirm so
 * Back returns to it. [read] is the code that filled it, not taken again
 * if the camera still sees it.
 */
data class SendDraft(
    val to: String,
    val amount: String = "",
    val inFiat: Boolean = false,
    val note: String = "",
    val request: PaymentRequest? = null,
    val read: String? = null,
)

/** The sheet's movement, as the board's. */
private val SHEET_EASING = CubicBezierEasing(0.2f, 0f, 0f, 1f)

/**
 * Send (Round 3 › Send B): the camera fills the screen under a sheet.
 * Folded, the sheet offers Paste an address, the people paid before and
 * what's ready to spend; pulled up (dragged or tapped), it's the form, the
 * camera a small live frame above it, still scanning. A payment code with
 * an amount goes straight to the confirm screen; one without, or an
 * address, fills the form. [code] is a payment code from a link, if any;
 * [draft] opens the form filled.
 */
@Composable
fun SendScreen(model: WalletModel, nav: Nav, code: String?, draft: SendDraft? = null) {
    val network by model.network.collectAsState()
    val wallet by model.wallet.collectAsState()
    val balance by model.balance.collectAsState()
    val prices by model.prices.collectAsState()
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var open by rememberSaveable { mutableStateOf(draft != null) }
    var torch by remember { mutableStateOf(false) }
    var hasTorch by remember { mutableStateOf(false) }
    var to by remember { mutableStateOf(draft?.to ?: "") }
    var amount by remember { mutableStateOf(draft?.amount ?: "") }
    var note by remember { mutableStateOf(draft?.note ?: "") }
    // The payment code the form came from: its receipt goes with the payment until the address or amount is changed.
    var request by remember { mutableStateOf(draft?.request) }
    // What a scan or paste found that isn't a payment code; under the viewfinder folded, under To pulled up.
    var notice by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    // The camera reads a code many times a second: each one is taken once.
    var lastRead by remember { mutableStateOf(draft?.read) }
    var paid by remember { mutableStateOf<List<PaidBefore>>(emptyList()) }
    // Contacts with a send address once there are any contacts; until then, who was paid before.
    var contacts by remember { mutableStateOf<List<ContactRow>>(emptyList()) }
    var hasContacts by remember { mutableStateOf(false) }
    LaunchedEffect(wallet, balance) {
        hasContacts = model.contacts().isNotEmpty()
        contacts = model.contactsOnSend()
        paid = if (hasContacts) emptyList() else model.paidBefore()
    }

    // The amount in money (Rust converts it to XMR) while there's a price, as on Receive.
    var entry by remember { mutableStateOf<AmountEntry?>(null) }
    // A form filled before keeps its unit; an empty one opens in money while there's a price.
    var inFiat by remember { mutableStateOf(draft?.inFiat ?: false) }
    var defaulted by remember { mutableStateOf(draft != null && draft.amount.isNotBlank()) }
    LaunchedEffect(wallet, prices, balance, amount, inFiat) {
        val e = wallet?.amountEntry(amount, inFiat) ?: return@LaunchedEffect
        if (e.canSwitch && !defaulted && amount.isBlank() && !inFiat) { defaulted = true; inFiat = true; return@LaunchedEffect }
        if (e.canSwitch) defaulted = true
        if (!e.canSwitch && inFiat) { inFiat = false; amount = ""; return@LaunchedEffect }
        entry = e
    }

    val toValid = remember(to, network) { to.isNotBlank() && runCatching { parsePayment(network, to) }.getOrNull()?.address == to.trim() }

    fun review(address: String, typed: String, fiat: Boolean, receiptJson: String?, withNote: String?) {
        busy = true
        error = null
        scope.launch {
            // The code's name goes with it while the form is the code's.
            runCatching { model.prepare(address, typed, fiat, receiptJson, withNote, request?.recipientName) }
                .onSuccess {
                    if (nav.current is Screen.Send) {
                        // Under Confirm, the form as filled: Back returns to it, not Home.
                        val filled = SendDraft(address, typed, fiat, note, request, lastRead)
                        nav.back(); nav.go(Screen.Send(draft = filled)); nav.go(Screen.Confirm(it, withNote))
                    }
                }
                .onFailure { error = it.reason(); open = true }
            busy = false
        }
    }

    /** A scanned, pasted or linked code: an address or a payment code without an amount fills the form; with one, straight to confirm. */
    fun take(text: String) {
        if (busy) return
        when (classifyCode(network, text)) {
            CodeKind.WALLET -> { notice = "That's a wallet code, not a payment code."; return }
            CodeKind.UNKNOWN -> { notice = runCatching { parsePayment(network, text) }.exceptionOrNull()?.reason() ?: "That isn't a payment code."; return }
            CodeKind.PAYMENT -> {}
        }
        val r = runCatching { parsePayment(network, text) }.getOrElse { notice = it.reason(); return }
        notice = null
        error = null
        to = r.address
        r.description?.let { note = it }
        request = r
        val fixed = r.amountText
        if (fixed != null) {
            // Filled too, so a payment that can't be prepared (not enough ready) shows what was asked.
            amount = fixed
            inFiat = false
            defaulted = true
            review(r.address, fixed, false, r.receiptJson, r.description)
        } else {
            open = true
        }
    }

    fun read(text: String) {
        if (text == lastRead) return
        lastRead = text
        take(text)
    }

    fun paste() {
        scope.launch {
            val text = clipboard.getClipEntry()?.clipData?.getItemAt(0)?.text?.toString()
            if (text.isNullOrBlank()) notice = "Nothing to paste." else take(text)
        }
    }

    LaunchedEffect(code) { code?.let { read(it) } }
    // Back folds the sheet first, then closes Send.
    BackHandler(enabled = open) { open = false }

    val density = LocalDensity.current
    BoxWithConstraints(Modifier.fillMaxSize().background(Fob.camera)) {
        val safe = WindowInsets.safeDrawing.asPaddingValues()
        val barBottom = safe.calculateTopPadding() + 64.dp
        // The small live frame only where there's room for it (not in landscape on most phones).
        val roomy = maxHeight >= 560.dp
        val openTop = barBottom + if (roomy) 128.dp else 8.dp
        var sheetHeight by remember { mutableIntStateOf(0) }
        val sheetTop = maxHeight - with(density) { sheetHeight.toDp() }

        CameraOrAsk(
            Modifier.fillMaxSize(), torch = torch, onHasTorch = { hasTorch = it },
            askPadding = PaddingValues(top = barBottom, bottom = maxHeight - sheetTop.coerceAtLeast(barBottom)),
            onText = ::read,
        )

        // The viewfinder: large over the camera while folded, small above the form.
        val finder by animateDpAsState(
            if (open) (if (roomy) 100.dp else 0.dp) else (sheetTop - barBottom - 76.dp).coerceIn(0.dp, 260.dp),
            tween(280, easing = SHEET_EASING), label = "finder",
        )
        Column(
            Modifier.fillMaxWidth().padding(top = barBottom).height((sheetTop - barBottom).coerceAtLeast(0.dp)),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
        ) {
            if (finder > 24.dp) Viewfinder(finder, if (open) 10f else 6f)
            if (!open) {
                Text(
                    notice ?: "Point at a payment code", color = Fob.onCamera, fontSize = 15.sp, textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal)).padding(horizontal = 24.dp, vertical = 16.dp),
                )
            }
        }

        // Close, the title and the torch, below the status bar and clear of a cutout.
        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
                .height(64.dp).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Box(
                Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button) { nav.back() }.semantics { contentDescription = "Close" },
                contentAlignment = Alignment.Center,
            ) { StrokeIcon(Icons.close, tint = Fob.onCamera, size = 20.dp) }
            Text("Send", color = Fob.onCamera, fontSize = 20.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f).semantics { heading() })
            if (hasTorch) {
                Box(
                    Modifier.size(48.dp).clip(CircleShape).background(if (torch) Fob.cameraCard else Color.Transparent)
                        .toggleable(torch, role = Role.Switch) { torch = it }
                        .semantics { contentDescription = "Torch"; stateDescription = if (torch) "On" else "Off" },
                    contentAlignment = Alignment.Center,
                ) { StrokeIcon(TORCH, tint = if (torch) Fob.cameraWarning else Fob.onCamera, size = 20.dp) }
            }
        }

        // The sheet: folded to what it holds, or pulled up to under the small frame.
        var dragged by remember { mutableFloatStateOf(0f) }
        val threshold = with(density) { 40.dp.toPx() }
        val drag = rememberDraggableState { delta ->
            dragged += delta
            if (dragged < -threshold && !open) { open = true; dragged = 0f }
            if (dragged > threshold && open) { open = false; dragged = 0f }
        }
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                // Folded, at most 62% of the screen, so a landscape phone keeps a strip of camera to aim with.
                .then(if (open) Modifier.height(maxHeight - openTop) else Modifier.heightIn(max = minOf(maxHeight - barBottom - 16.dp, maxHeight * 0.62f)))
                .animateContentSize(tween(280, easing = SHEET_EASING))
                .onSizeChanged { sheetHeight = it.height }
                .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(Fob.paper)
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
                .padding(start = 20.dp, end = 20.dp, bottom = 16.dp),
        ) {
            // The handle: drag it, or tap it.
            Box(
                Modifier.fillMaxWidth().height(28.dp)
                    .draggable(drag, Orientation.Vertical, onDragStopped = { dragged = 0f })
                    .clickable(role = Role.Button, onClickLabel = if (open) "Back to the camera" else "Type the payment instead") { open = !open }
                    .semantics { contentDescription = if (open) "Back to the camera" else "Type the payment instead"; stateDescription = if (open) "Expanded" else "Collapsed" },
                contentAlignment = Alignment.Center,
            ) { Box(Modifier.width(36.dp).height(4.dp).clip(RoundedCornerShape(2.dp)).background(Fob.outline)) }
            if (open) {
                Column(Modifier.weight(1f).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    Labelled("To") {
                        ToField(to, toValid, onChange = { typed ->
                            // A whole payment code typed or pasted in is read as one.
                            if (typed.trim().startsWith("monero:", ignoreCase = true)) take(typed.trim())
                            else { to = typed; request = null; lastRead = null; notice = null }
                        }, onPaste = ::paste)
                        ErrorText(notice)
                    }
                    Labelled("Amount") {
                        AmountField(
                            amount, { amount = it; if (request?.amount != null) request = null }, entry,
                            onSwitch = { scope.launch { val toFiat = !inFiat; amount = wallet?.convertAmount(amount, toFiat) ?: ""; inFiat = toFiat } },
                        )
                    }
                    Labelled("Note (optional)") { NoteField(note) { note = it } }
                    balance?.let { Text(it.readyLine, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(start = 4.dp)) }
                }
                Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    ErrorText(error)
                    PrimaryButton(if (busy) "Getting the fee…" else "Review", enabled = !busy && toValid && entry?.sendable == true, onClick = {
                        val r = request
                        review(to.trim(), amount, entry?.inFiat == true, r?.receiptJson, note.trim().ifEmpty { null })
                    })
                }
            } else {
                Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    PasteRow(::paste)
                    if (busy) Text("Getting the fee…", color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(start = 4.dp))
                    ErrorText(error)
                    // Decision 95's spot: CONTACTS once there are any, "All" opening the Contacts screen; PAID BEFORE until then.
                    if (hasContacts) {
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Row(Modifier.fillMaxWidth().padding(start = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    "CONTACTS", color = Fob.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.4.sp,
                                    modifier = Modifier.weight(1f).semantics { heading() },
                                )
                                TextButton("All", { nav.go(Screen.Contacts) })
                            }
                            if (contacts.isEmpty()) {
                                Text("Give a contact a send address to pay them from here.", color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(start = 4.dp, bottom = 4.dp))
                            }
                            contacts.forEach { c -> ContactSendRow(c) { c.sendAddress?.let { to = it }; request = null; notice = null; open = true } }
                        }
                    } else if (paid.isNotEmpty()) {
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(
                                "PAID BEFORE", color = Fob.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.4.sp,
                                modifier = Modifier.padding(start = 4.dp, bottom = 4.dp).semantics { heading() },
                            )
                            paid.forEach { p -> PaidRow(p) { to = p.address; request = null; notice = null; open = true } }
                        }
                    }
                    balance?.let { Text(it.readyLine, color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth()) }
                }
            }
        }
    }
}

/** The viewfinder's corners, [size] across, in the camera's light colour; [width] on the 280 grid. */
@Composable
fun Viewfinder(size: Dp, width: Float) {
    val corners = remember { PathParser().parsePathString(CORNERS).toPath() }
    val colour = Fob.onCamera
    Canvas(Modifier.size(size).clearAndSetSemantics {}) {
        val s = this.size.width / 280f
        withTransform({ scale(s, s, pivot = Offset.Zero) }) {
            drawPath(corners, colour, style = Stroke(width, cap = StrokeCap.Round))
        }
    }
}

/** A small label over a field, as the board's. */
@Composable
private fun Labelled(label: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, color = Fob.muted, fontSize = 14.sp, modifier = Modifier.padding(start = 4.dp))
        content()
    }
}

/**
 * Who's paid: the address's pattern, the address (its first and last five
 * while not being edited, from Rust) and Paste. The pattern shows once
 * it's an address of this network.
 */
@Composable
private fun ToField(value: String, valid: Boolean, onChange: (String) -> Unit, onPaste: () -> Unit) {
    var focused by remember { mutableStateOf(false) }
    val shortened = valid && !focused
    val style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 15.sp, color = if (shortened) Color.Transparent else Fob.ink)
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).clip(RoundedCornerShape(16.dp)).background(Fob.card)
            .border(1.dp, if (focused) Fob.ink else Fob.outline, RoundedCornerShape(16.dp)).padding(start = 12.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (valid) AddressPattern(value.trim(), 30.dp)
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            BasicTextField(
                value = value, onValueChange = onChange, singleLine = true, textStyle = style, cursorBrush = SolidColor(Fob.ink),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
                modifier = Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused }.semantics { contentDescription = "To" },
                decorationBox = { field ->
                    Box(contentAlignment = Alignment.CenterStart) {
                        if (value.isEmpty()) Text("Address", style = style.copy(color = Fob.muted, fontFamily = FontFamily.Default))
                        field()
                    }
                },
            )
            // The field stays under it, so a tap still edits the whole address.
            if (shortened) Text(shortAddress(value), style = style.copy(color = Fob.ink), maxLines = 1, modifier = Modifier.clearAndSetSemantics {})
        }
        Box(
            Modifier.minimumInteractiveComponentSize().height(40.dp).clip(RoundedCornerShape(12.dp)).background(Fob.sunken)
                .clickable(role = Role.Button, onClick = onPaste).padding(horizontal = 12.dp),
            contentAlignment = Alignment.Center,
        ) { Text("Paste", color = Fob.ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
    }
}

/** What it's for, kept on this phone with the payment. */
@Composable
private fun NoteField(value: String, onChange: (String) -> Unit) {
    var focused by remember { mutableStateOf(false) }
    val style = TextStyle(fontSize = 16.sp, color = Fob.ink)
    BasicTextField(
        value = value, onValueChange = onChange, singleLine = true, textStyle = style, cursorBrush = SolidColor(Fob.ink),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        modifier = Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused }.semantics { contentDescription = "Note (optional)" },
        decorationBox = { field ->
            Box(
                Modifier.fillMaxWidth().height(56.dp).clip(RoundedCornerShape(16.dp)).background(Fob.card)
                    .border(1.dp, if (focused) Fob.ink else Fob.outline, RoundedCornerShape(16.dp)).padding(horizontal = 16.dp),
                contentAlignment = Alignment.CenterStart,
            ) {
                if (value.isEmpty()) Text("What it's for, kept on this phone", style = style.copy(color = Fob.muted))
                field()
            }
        },
    )
}

/** Paste an address: the clipboard is read only on this tap, so Android shows no paste notice before. */
@Composable
private fun PasteRow(onPaste: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 60.dp).clip(RoundedCornerShape(18.dp)).background(Fob.card)
            .border(1.dp, Fob.outline, RoundedCornerShape(18.dp)).clickable(role = Role.Button, onClick = onPaste)
            .padding(start = 14.dp, end = 12.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(36.dp).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(CLIPBOARD, tint = Fob.ink, size = 18.dp) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("Paste an address", fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Text("Read from the clipboard only when you tap", color = Fob.muted, fontSize = 13.sp)
        }
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
    }
}

/** A contact on the sheet: their avatar, name and last payment; tapping fills To with their send address. */
@Composable
private fun ContactSendRow(c: ContactRow, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) {}.padding(4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Avatar(c.name, c.initial, c.photo, 30.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(c.name, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(c.line, color = Fob.muted, fontSize = 12.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
    }
}

/** Someone paid before: their pattern, name and last payment; tapping fills the form. */
@Composable
private fun PaidRow(p: PaidBefore, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) {}.padding(4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        AddressPattern(p.address, 30.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(p.name, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(p.line, color = Fob.muted, fontSize = 12.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        StrokeIcon(Icons.chevron, tint = Fob.muted, size = 16.dp)
    }
}
