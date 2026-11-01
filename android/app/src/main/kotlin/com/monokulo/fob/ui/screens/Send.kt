package com.monokulo.fob.ui.screens

import androidx.activity.compose.BackHandler
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
import androidx.compose.ui.MotionDurationScale
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.lerp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.AddressPattern
import com.monokulo.fob.ui.AmountField
import com.monokulo.fob.ui.Avatar
import com.monokulo.fob.ui.EdgeButton
import com.monokulo.fob.ui.EdgeIconButton
import com.monokulo.fob.ui.bottomSpace
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.fob.CodeKind
import uniffi.fob.ContactRow
import uniffi.fob.PaidBefore
import uniffi.fob.PaymentPreview
import uniffi.fob.ScannedProfile
import uniffi.fob.classifyCode
import uniffi.fob.isProfileCode
import uniffi.fob.parsePayment
import uniffi.fob.shortAddress
import kotlin.math.abs
import kotlin.math.roundToInt

/** A torch: the lamp's head and its handle. */
private const val TORCH = "M8 3h8l-2 6h-4L8 3Z M10 9v11h4V9"
/** A clipboard. */
private const val CLIPBOARD = "M9 4h6v3H9z M7 5H5v15h14V5h-2"
/** The viewfinder's four corners, on a 280 × 280 grid. */
private const val CORNERS = "M8 64V24a16 16 0 0 1 16-16h40 M216 8h40a16 16 0 0 1 16 16v40 M272 216v40a16 16 0 0 1-16 16h-40 M64 272H24a16 16 0 0 1-16-16v-40"

/**
 * Send's form as it was filled: kept on the back stack under Confirm so
 * Back returns to it, its note the one Confirm showed. [code] is the
 * payment code it was filled from, whose name and receipt Rust keeps with
 * the payment as long as they still apply; [read] is the last code the
 * camera saw, not taken again while it still sees it.
 */
data class SendDraft(
    val to: String,
    val amount: String = "",
    val inFiat: Boolean = false,
    val note: String = "",
    val code: String? = null,
    val read: String? = null,
)

/**
 * A payment Confirm opens on before its fee is in: what Rust checked of
 * the form ([preview]), and the form as filled, which Confirm prepares.
 */
data class PendingSend(val preview: PaymentPreview, val draft: SendDraft)

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
    // The payment code the form came from, as read: Rust keeps its name while To is its address, its receipt while
    // the amount is its too.
    var filledFrom by remember { mutableStateOf(draft?.code) }
    // A code the camera saw while the form held a To already: asked about, never taken unasked.
    var pending by remember { mutableStateOf<Pair<String, String>?>(null) }
    // What a scan or paste found that isn't a payment code; under the viewfinder folded, under To pulled up.
    var notice by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    // The camera reads a code many times a second: each one is taken once, a link's too after the screen is made again.
    var lastRead by rememberSaveable { mutableStateOf(draft?.read) }
    // A scanned profile, with the code it came from: shown on its own sheet in place of Send's.
    var profile by remember { mutableStateOf<Pair<String, ScannedProfile>?>(null) }
    // Contacts with a send address once there are any contacts; until then, who was paid before. The model's, read
    // with the wallet, so the folded sheet is its full height on the first frame.
    val people by model.sendPeople.collectAsState()
    val contacts = people.contacts
    val paid = people.paid
    val hasContacts = people.anyContact

    // The amount in money (Rust converts it to XMR) while there's a price, as on Receive. A form filled before keeps
    // its unit; an empty one opens as Rust has it already (in money while there's a price), drawn whole at once.
    val blank = prices?.blankEntry
    val filled = draft != null && draft.amount.isNotBlank()
    var inFiat by remember { mutableStateOf(if (filled) draft.inFiat else blank?.inFiat ?: false) }
    var entry by remember { mutableStateOf(blank?.takeIf { it.inFiat == inFiat }) }
    var defaulted by remember { mutableStateOf(filled || blank?.canSwitch == true) }
    // Money typed when the price went, kept aside until one comes.
    var heldMoney by remember { mutableStateOf<String?>(null) }
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
    // Rust's word on what the field holds now; none while it's behind (the chip just tapped), so Review waits.
    val current = entry?.takeIf { it.forText == amount && it.forInFiat == inFiat }

    // The price toast's cog opens Price from over this: Back finds the form as filled, pulled up.
    DisposableEffect(nav) {
        val keep = {
            val money = heldMoney
            if (open || to.isNotBlank() || amount.isNotBlank() || money != null) {
                nav.replace(Screen.Send(draft = SendDraft(to, money ?: amount, money != null || inFiat, note, filledFrom, lastRead)))
            }
        }
        nav.keepForm = keep
        onDispose { if (nav.keepForm === keep) nav.keepForm = null }
    }

    val toValid = remember(to, network) { to.isNotBlank() && runCatching { parsePayment(network, to) }.getOrNull()?.address == to.trim() }

    fun review(address: String, typed: String, fiat: Boolean, withNote: String?) {
        busy = true
        error = null
        scope.launch {
            // Checked without asking the node, so Confirm opens at once (Round 9 › 3b) and the fee comes in on it.
            runCatching { model.preview(address, typed, fiat, filledFrom) }
                .onSuccess {
                    if (nav.current is Screen.Send) {
                        // Under Confirm, the form as filled, with the note Confirm shows: Back returns to it, not Home.
                        val filled = SendDraft(address, typed, fiat, withNote ?: "", filledFrom, lastRead)
                        nav.back(); nav.go(Screen.Send(draft = filled)); nav.go(Screen.Confirm(note = withNote, pending = PendingSend(it, filled)))
                    }
                }
                .onFailure { error = it.reason(); open = true }
            busy = false
        }
    }

    /**
     * A scanned, pasted or linked code: a profile shows who it is first (Pay or Add contact); an address or a
     * payment code without an amount fills the form; with one, straight to confirm.
     */
    fun take(text: String, asProfile: Boolean = true) {
        if (busy) return
        if (asProfile && isProfileCode(network, text)) {
            scope.launch { runCatching { wallet?.scannedProfile(text) }.onSuccess { s -> if (s != null) { profile = text to s; notice = null; open = false } }.onFailure { notice = it.reason() } }
            return
        }
        when (classifyCode(network, text)) {
            CodeKind.WALLET -> { notice = "That's a wallet code, not a payment code."; return }
            CodeKind.UNKNOWN -> { notice = runCatching { parsePayment(network, text) }.exceptionOrNull()?.reason() ?: "That isn't a payment code."; return }
            CodeKind.PAYMENT -> {}
        }
        val r = runCatching { parsePayment(network, text) }.getOrElse { notice = it.reason(); return }
        notice = null
        error = null
        pending = null
        to = r.address
        // The note follows the code: its description, or none.
        note = r.description ?: ""
        filledFrom = text
        val fixed = r.amountText
        if (fixed != null) {
            // Filled too, so a payment that can't be prepared (not enough ready) shows what was asked.
            amount = fixed
            inFiat = false
            defaulted = true
            review(r.address, fixed, false, r.description)
        } else {
            open = true
        }
    }

    /**
     * What the camera read. Taken only while getting the fee isn't under way, and once. With the form pulled up
     * and To filled, a code is never taken unasked (one on the counter beside the shop's, say): one to another
     * address is asked about, anything else left alone.
     */
    fun read(text: String) {
        if (busy || text == lastRead) return
        lastRead = text
        if (open && to.isNotBlank() && !isProfileCode(network, text)) {
            val found = runCatching { parsePayment(network, text) }.getOrNull()
            if (found != null && found.address != to.trim()) pending = text to found.address
            return
        }
        take(text)
    }

    fun paste() {
        scope.launch {
            val text = clipboard.getClipEntry()?.clipData?.getItemAt(0)?.text?.toString()
            if (text.isNullOrBlank()) notice = "Nothing to paste." else take(text)
        }
    }

    /** The sheet pulled up or folded by hand: a reason shown for the other half goes. */
    fun pull(up: Boolean) {
        open = up
        error = null
        notice = null
    }

    LaunchedEffect(code) { code?.let { read(it) } }
    // Back folds the sheet first, then closes Send.
    BackHandler(enabled = open) { pull(false) }
    // A profile's sheet closes first; the camera doesn't take the same code again.
    BackHandler(enabled = profile != null) { profile = null }

    val density = LocalDensity.current
    BoxWithConstraints(Modifier.fillMaxSize().background(Fob.camera)) {
        val safe = WindowInsets.safeDrawing.asPaddingValues()
        val barBottom = safe.calculateTopPadding() + 64.dp
        val screenPx = constraints.maxHeight.toFloat()
        val barPx = with(density) { barBottom.toPx() }
        // Folded, at most 62% of the screen, so a landscape phone keeps a strip of camera to aim with; pulled up, at
        // most up to the top bar, the form scrolling above Review only then.
        val foldCap = minOf(maxHeight - barBottom - 16.dp, maxHeight * 0.62f).coerceAtLeast(0.dp)
        val openCap = (maxHeight - barBottom).coerceAtLeast(0.dp)

        // Each stop's height as last measured (px): the sheet is exactly as tall as what it holds there.
        var foldMeasured by remember { mutableStateOf<Float?>(null) }
        var openMeasured by remember { mutableStateOf<Float?>(null) }
        val openStop = openMeasured ?: with(density) { openCap.toPx() }
        // Not measured yet (Send opened on the form): a guess the settle corrects once the folded sheet is drawn.
        val foldStop = foldMeasured ?: (minOf(with(density) { foldCap.toPx() }, openStop) * 0.5f)
        // Read when used, so a settle under way eases to a stop measured after it began.
        val openNow by rememberUpdatedState(openStop)
        val foldNow by rememberUpdatedState(foldStop)
        fun stop(up: Boolean) = if (up) openNow else foldNow

        // While dragged or settling the sheet is [live] tall (px); at rest, its stop's height, following its content.
        var live by remember { mutableFloatStateOf(0f) }
        var moving by remember { mutableStateOf(false) }
        var dragging by remember { mutableStateOf(false) }
        var settle by remember { mutableStateOf<Job?>(null) }
        // A scanned profile's sheet, in place of Send's: the camera's room is above it.
        var profileHeight by remember { mutableIntStateOf(0) }
        // However the sheet is pulled up or folded (the handle, Back, a scan, a contact), it moves to its stop: until
        // the move begins, it stays at the one it was shown at.
        var shownOpen by remember { mutableStateOf(open) }
        val height = if (moving) live else stop(shownOpen)

        /**
         * From [from] to the stop [open] names, eased over 300 ms (as Android's animator duration scale stretches it,
         * at once with "Remove animations"); a stop measured meanwhile is eased to.
         */
        fun settleFrom(from: Float) {
            settle?.cancel()
            live = from
            moving = true
            settle = scope.launch {
                val scale = coroutineContext[MotionDurationScale]?.scaleFactor ?: 1f
                if (scale > 0f) {
                    val span = 300_000_000f * scale
                    val start = withFrameNanos { it }
                    do {
                        val t = ((withFrameNanos { it } - start) / span).coerceIn(0f, 1f)
                        live = from + (stop(open) - from) * SHEET_EASING.transform(t)
                    } while (t < 1f)
                }
                moving = false
            }
        }
        LaunchedEffect(open) {
            if (open == shownOpen) return@LaunchedEffect
            val from = if (moving) live else stop(shownOpen)
            shownOpen = open
            if (!dragging) settleFrom(from)
        }
        // Dragged above its folded height the sheet shows the form, so what it reveals is what it opens to.
        val form = open || (dragging && live > foldStop + 0.5f)

        val sheetPx = if (profile != null) profileHeight.toFloat() else height
        val sheetTop = maxHeight - with(density) { sheetPx.toDp() }

        // What sits over the camera (the viewfinder, its words, Allow the camera) is placed by the sheet's height,
        // measured only once the sheet is drawn: until then it's left undrawn rather than drawn at a guess and
        // jumping a frame later.
        val placed = profile != null || (if (shownOpen) openMeasured else foldMeasured) != null
        val overCamera = Modifier.alpha(if (placed) 1f else 0f)
        CameraOrAsk(
            Modifier.fillMaxSize().then(overCamera), torch = torch, onHasTorch = { hasTorch = it },
            askPadding = PaddingValues(top = barBottom, bottom = maxHeight - sheetTop.coerceAtLeast(barBottom)),
            onText = ::read,
        )

        // The viewfinder: large over the camera while folded, centred in the room above the form pulled up, and
        // between the two as the sheet moves; none where there's too little room (landscape, the keyboard up).
        fun foldedFrame(room: Dp) = (room - 76.dp).coerceIn(0.dp, 260.dp)
        fun openFrame(room: Dp) = (room - 32.dp).coerceAtMost(200.dp).let { if (it < 64.dp) 0.dp else it }
        val foldRoom = with(density) { (screenPx - foldStop - barPx).toDp() }
        val openRoom = with(density) { (screenPx - openStop - barPx).toDp() }
        val toward = if (openStop - foldStop > 1f) ((height - foldStop) / (openStop - foldStop)).coerceIn(0f, 1f) else if (open) 1f else 0f
        // Over a scanned profile's sheet, the large frame, eased as its sheet comes.
        val profileFinder by animateDpAsState(foldedFrame(sheetTop - barBottom), tween(280, easing = SHEET_EASING), label = "finder")
        val finder = if (profile != null) profileFinder else lerp(foldedFrame(foldRoom), openFrame(openRoom), toward)
        Column(
            Modifier.fillMaxWidth().padding(top = barBottom).height((sheetTop - barBottom).coerceAtLeast(0.dp)).then(overCamera),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
        ) {
            if (finder > 24.dp) Viewfinder(finder, if (profile != null) 6f else 6f + 4f * toward)
            // In a landscape strip without the frame, the words take less room, and none where there's none.
            if (!form && sheetTop - barBottom >= 40.dp) {
                Text(
                    notice ?: "Point at a payment code", color = Fob.onCamera, fontSize = 15.sp, textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))
                        .padding(horizontal = 24.dp, vertical = if (finder > 24.dp) 16.dp else 8.dp),
                )
            }
        }

        // Back, the title and the torch, below the status bar and clear of a cutout.
        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
                .heightIn(min = 64.dp).padding(horizontal = EdgeButton.inset),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            EdgeIconButton(Icons.back, "Back", { nav.back() }, tint = Fob.onCamera)
            Text("Send", color = Fob.onCamera, fontSize = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).semantics { heading() })
            if (hasTorch) {
                Box(
                    Modifier.size(EdgeButton.target).clip(CircleShape).background(if (torch) Fob.cameraCard else Color.Transparent)
                        .toggleable(torch, role = Role.Switch) { torch = it }
                        .semantics { contentDescription = "Torch"; stateDescription = if (torch) "On" else "Off" },
                    contentAlignment = Alignment.Center,
                ) { StrokeIcon(TORCH, tint = if (torch) Fob.cameraWarning else Fob.onCamera, size = EdgeButton.icon) }
            }
        }

        // The sheet follows the finger on its handle, between its folded and pulled-up heights; let go, it settles
        // to the nearer, or the way it was flung.
        val fling = with(density) { 125.dp.toPx() }
        val drag = rememberDraggableState { delta ->
            if (dragging) live = (live - delta).coerceIn(minOf(foldNow, openNow), maxOf(foldNow, openNow))
        }
        if (profile == null) {
            // Its height is the finger's or the settle's while moving; at rest, what it holds, measured in the same
            // layout pass (from state it would be a frame late: a flash as Send opens, Review under the rising
            // keyboard). What it holds keeps its own height, top first, clipped below.
            Layout(
                modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                    .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(Fob.paper),
                content = {
                Column(
                    Modifier.fillMaxWidth()
                        .heightIn(max = if (form) openCap else foldCap)
                        .onSizeChanged { if (form) openMeasured = it.height.toFloat() else foldMeasured = it.height.toFloat() }
                        .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
                        .padding(start = 20.dp, end = 20.dp, bottom = 16.dp),
                ) {
                    // The handle: drag it, or tap it; 48 dp to touch, its bar small.
                    Box(
                        Modifier.fillMaxWidth().height(48.dp)
                            .draggable(
                                drag, Orientation.Vertical,
                                onDragStarted = {
                                    settle?.cancel()
                                    live = if (moving) live else stop(open)
                                    moving = true
                                    dragging = true
                                },
                                onDragStopped = { velocity ->
                                    dragging = false
                                    val up = if (abs(velocity) > fling) velocity < 0f else live > (foldNow + openNow) / 2f
                                    shownOpen = up
                                    if (up != open) pull(up)
                                    settleFrom(live)
                                },
                            )
                            .clickable(role = Role.Button, onClickLabel = if (open) "Back to the camera" else "Type the payment instead") { pull(!open) }
                            .semantics { contentDescription = if (open) "Back to the camera" else "Type the payment instead"; stateDescription = if (open) "Expanded" else "Collapsed" },
                        contentAlignment = Alignment.Center,
                    ) { Box(Modifier.width(36.dp).height(4.dp).clip(RoundedCornerShape(2.dp)).background(Fob.outline)) }
                    if (form) {
                        // As tall as the form, Review straight under it; scrolling only where it doesn't fit, Review kept.
                        Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                            Labelled("To") {
                                ToField(to, toValid, onChange = { typed ->
                                    // A whole payment code typed or pasted in is read as one.
                                    if (typed.trim().startsWith("monero:", ignoreCase = true)) take(typed.trim())
                                    // A code still in view isn't taken again for an edit; cleared, To takes the next one read.
                                    else { to = typed; pending = null; notice = null; if (typed.isBlank()) lastRead = null }
                                }, onPaste = ::paste)
                                ErrorText(notice)
                                pending?.let { (seen, address) -> OtherCode(address, onUse = { take(seen) }, onKeep = { pending = null }) }
                            }
                            Labelled("Amount") {
                                AmountField(
                                    amount, { amount = it }, entry,
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
                            }
                            Labelled("Note (optional)") { NoteField(note) { note = it } }
                            balance?.let { Text(it.readyLine, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(start = 4.dp)) }
                        }
                        // The price toast rises above Review (and the sheet's 16 dp under it).
                        Column(Modifier.bottomSpace(16.dp).padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            ErrorText(error)
                            PrimaryButton(if (busy) "Getting the fee…" else "Review", enabled = !busy && toValid && current?.sendable == true, onClick = {
                                val e = current ?: return@PrimaryButton
                                review(to.trim(), amount, e.inFiat, note.trim().ifEmpty { null })
                            })
                        }
                    } else {
                        Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                            // No room over the camera for what a scan found (a short landscape window): it shows here.
                            if (sheetTop - barBottom < 40.dp) ErrorText(notice)
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
                                    contacts.forEach { c -> ContactSendRow(c) { c.sendAddress?.let { to = it }; filledFrom = null; pending = null; notice = null; open = true } }
                                }
                            } else if (paid.isNotEmpty()) {
                                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                    Text(
                                        "PAID BEFORE", color = Fob.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.4.sp,
                                        modifier = Modifier.padding(start = 4.dp, bottom = 4.dp).semantics { heading() },
                                    )
                                    paid.forEach { p -> PaidRow(p) { to = p.address; filledFrom = null; pending = null; notice = null; open = true } }
                                }
                            }
                            balance?.let { Text(it.readyLine, color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth()) }
                        }
                    }
                }
                },
            ) { measurables, constraints ->
                val content = measurables.single().measure(constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity))
                // Just pulled up or folded, before the settle's first frame: still the stop it's leaving, not a flash
                // of the other one.
                val sheet = if (moving) live.roundToInt() else if (open != shownOpen) stop(shownOpen).roundToInt() else content.height
                layout(constraints.maxWidth, sheet.coerceIn(constraints.minHeight, constraints.maxHeight)) { content.place(0, 0) }
            }
        }

        // A profile scanned (Round 6 › "Another Fob scans the code"): Pay fills the form as its code would.
        profile?.let { (text, found) ->
            // Over the camera as the folded sheet is, the large frame above it.
            Box(Modifier.align(Alignment.BottomCenter).onSizeChanged { profileHeight = it.height }) {
                ProfileFound(model, nav, text, found, onPay = { profile = null; take(text, asProfile = false) }, onClose = { profile = null })
            }
        }
    }
}

/**
 * A code the camera saw while To was filled: its address's pattern and ends, and whether to use it in place of
 * what's there. Nothing changes until Use it.
 */
@Composable
private fun OtherCode(address: String, onUse: () -> Unit, onKeep: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Fob.sunken).padding(start = 14.dp, end = 8.dp, top = 10.dp, bottom = 4.dp)
            .semantics { liveRegion = LiveRegionMode.Polite },
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            AddressPattern(address, 24.dp)
            Text("The camera found another code, to ${shortAddress(address)}. Use it instead?", fontSize = 14.sp, lineHeight = 19.sp, modifier = Modifier.weight(1f))
        }
        Row(Modifier.align(Alignment.End), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            TextButton("Keep", onKeep)
            TextButton("Use it", onUse)
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
                if (value.isEmpty()) Text("What it's for", style = style.copy(color = Fob.muted))
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
        Text("Paste an address", fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
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
