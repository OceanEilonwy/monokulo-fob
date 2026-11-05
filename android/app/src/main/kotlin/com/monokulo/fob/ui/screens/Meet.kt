package com.monokulo.fob.ui.screens

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.GraphicsLayerScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.monokulo.fob.Met
import com.monokulo.fob.Nav
import com.monokulo.fob.Nfc
import com.monokulo.fob.Screen
import com.monokulo.fob.SwapRunner
import com.monokulo.fob.TouchPhones
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.AddressPattern
import com.monokulo.fob.ui.Avatar
import com.monokulo.fob.ui.BarIcons
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.EdgeButton
import com.monokulo.fob.ui.EdgeIconButton
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.Warning
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.openNfcSettings
import com.monokulo.fob.ui.reason
import com.monokulo.fob.ui.rememberTick
import com.monokulo.fob.ui.rememberTouchState
import com.monokulo.fob.ui.rememberWavePhase
import com.monokulo.fob.ui.short
import com.monokulo.fob.ui.TouchState
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import uniffi.fob.Contact
import uniffi.fob.Profile
import uniffi.fob.ProfileShare
import uniffi.fob.ScannedProfile
import uniffi.fob.Swapped
import uniffi.fob.TouchSwap
import uniffi.fob.meetCodeNeedsName
import uniffi.fob.meetCodeProblem
import uniffi.fob.nameMeetCode
import uniffi.fob.qrCode

/** What Meet is showing. */
private sealed interface Stage {
    /** The tabs: Touch, or your code with Scan their code. */
    data object Show : Stage
    /** The camera, reading their code. */
    data object Scanning : Stage
    /** Their first code was scanned: yours is shown for them to scan back. */
    data class Replied(val contact: Contact) : Stage
    /** Both ways done: Met. */
    data class Done(val contact: Contact, val viaTouch: Boolean) : Stage
}

/**
 * Meet (Round 3 designs): two Fobs swap names and addresses, so each saves
 * the other as a contact and each is the other's send address. Touch (the
 * default, listening as it opens) swaps both ways in one touch, photos
 * included. QR code does it with two scans: the first phone shows its code
 * and the second scans it, then the second shows its code (marked as a
 * reply) and the first scans that. Either way ends on Met. No profile is
 * needed: a code without a name is only an address, and whoever gets it
 * chooses what to call you.
 */
@Composable
fun MeetScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val scope = rememberCoroutineScope()
    val touch = rememberTouchState(model)
    val hasNfc = touch.nfc != Nfc.NONE
    var tab by rememberSaveable { mutableStateOf(0) }
    // No NFC on this phone: the QR code is the only way.
    val shownTab = if (hasNfc) tab else 1
    var profile by remember { mutableStateOf<Profile?>(null) }
    var share by remember { mutableStateOf<ProfileShare?>(null) }
    var replyCode by remember { mutableStateOf<String?>(null) }
    var stage by remember { mutableStateOf<Stage>(Stage.Show) }
    // A scanned or touched profile that would change a contact: asked about before anything is.
    var asking by remember { mutableStateOf<Pair<String, ScannedProfile>?>(null) }
    // Their code came with no name, and how it came: asked for before anything is saved.
    var naming by remember { mutableStateOf<Pair<String, Boolean>?>(null) }
    var namingError by remember { mutableStateOf<String?>(null) }
    var notice by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var lastRead by remember { mutableStateOf<String?>(null) }
    val tick = rememberTick()

    // Opening Meet makes one share, an address for whoever you meet (kept as a share, as Share my profile did).
    LaunchedEffect(wallet) {
        val w = wallet ?: return@LaunchedEffect
        profile = w.profile()
        if (share == null) share = runCatching { model.shareProfile() }.onFailure { error = it.reason() }.getOrNull()
    }

    /** Their profile was saved: a reply (their phone has ours already) or their touch is Met; a first code is answered. */
    fun saved(contact: Contact, found: ScannedProfile, viaTouch: Boolean = false, typedName: Boolean = false) {
        asking = null
        tick()
        // By code, or by a touch that brought no name, the share is kept as met so Activity lists it (the swap keeps a named touch).
        if (!viaTouch || typedName) share?.let { sh -> scope.launch { runCatching { wallet?.shareMet(sh.id, contact.name) }; model.refresh() } }
        stage = if (viaTouch || found.reply) Stage.Done(contact, viaTouch) else Stage.Replied(contact)
    }

    fun meet(text: String, viaTouch: Boolean, typedName: Boolean = false) {
        if (meetCodeNeedsName(network, text)) {
            namingError = null
            naming = text to viaTouch
            stage = Stage.Show
            return
        }
        scope.launch {
            runCatching { model.meetSave(text) }
                .onSuccess { r ->
                    when (r) {
                        is Met.Saved -> saved(r.contact, r.found, viaTouch, typedName)
                        is Met.Ask -> { asking = text to r.found; stage = Stage.Show }
                    }
                }
                .onFailure { notice = it.reason(); error = it.reason() }
        }
    }

    // Touch: the swap runs while Meet is open on its Touch tab, answering and reading as phones touch.
    val touching = shownTab == 0 && stage is Stage.Show && asking == null && naming == null
    RememberSwap(model, share?.id, allowed = touching && touch.meetActive && share?.canTouch == true) { done -> meet(done.text, viaTouch = true) }

    BackHandler(enabled = stage is Stage.Scanning) { stage = Stage.Show }
    BackHandler(enabled = asking != null) { asking = null }

    val reply = share?.id
    LaunchedEffect(wallet, reply, stage) {
        if (stage is Stage.Replied && reply != null) replyCode = wallet?.shareReplyCode(reply)
    }

    when (val s = stage) {
        is Stage.Scanning -> ScanView(
            network = network, notice = notice, onClose = { stage = Stage.Show },
            onText = { text ->
                if (text == lastRead) return@ScanView
                lastRead = text
                val problem = meetCodeProblem(network, text)
                if (problem != null) { notice = problem; return@ScanView }
                notice = null
                meet(text, viaTouch = false)
            },
        )
        is Stage.Done -> MetView(model, nav, profile, share, s.contact, s.viaTouch)
        else -> Box(Modifier.fillMaxSize()) {
            val replied = s as? Stage.Replied
            Page(bottom = {
                ErrorText(error)
                if (replied != null) PrimaryButton("Done", { stage = Stage.Done(replied.contact, false) })
                else if (shownTab == 1) OutlineButton("Scan their code", { lastRead = null; notice = null; stage = Stage.Scanning }, Modifier.fillMaxWidth(), leading = Icons.scan)
            }) {
                TopBar("Meet", onBack = { nav.back() })
                if (replied == null && hasNfc) MeetTabs(shownTab) { tab = it }
                when {
                    replied != null -> YourCode(profile, replied.contact, replyCode, savedLine = "${replied.contact.name} saved", hint = "Let ${replied.contact.name} scan this, and you're done.")
                    shownTab == 0 -> TouchTab(profile, loaded = share != null, touch, onSettings = { nav.go(Screen.Settings) })
                    else -> YourCode(
                        profile, null, share?.code, savedLine = null,
                        hint = "Let them scan this, then tap Scan their code." + if (profile == null && share != null) " Your name isn't shared: they choose what to call you." else "",
                    )
                }
            }
            naming?.let { (text, viaTouch) ->
                NameThem(
                    namingError,
                    onDismiss = { naming = null; lastRead = null },
                    onSave = { typed ->
                        runCatching { nameMeetCode(network, text, typed) }
                            .onSuccess { named -> naming = null; meet(named, viaTouch, typedName = true) }
                            .onFailure { namingError = it.reason() }
                    },
                )
            }
            asking?.let { (text, found) ->
                Box(Modifier.fillMaxSize().background(Fob.scrim).clickable(onClickLabel = "Close", indication = null, interactionSource = null) { asking = null })
                Box(Modifier.align(Alignment.BottomCenter)) {
                    ProfileFound(
                        model, nav, text, found,
                        onPay = { nav.replace(Screen.Send(draft = SendDraft(found.address, code = text, read = text))) },
                        onClose = { asking = null }, startAsking = true, met = true,
                        openContact = { id -> scope.launch { wallet?.contact(id)?.let { saved(it, found, viaTouch = false) } } },
                    )
                }
            }
        }
    }
}

/** Their code came with no name: what you'll call them, which stays on this phone. */
@Composable
private fun NameThem(error: String?, onDismiss: () -> Unit, onSave: (String) -> Unit) {
    var name by rememberSaveable { mutableStateOf("") }
    BottomSheet("What do you call them?", onDismiss) {
        SheetHandle()
        Column(Modifier.fillMaxWidth().padding(horizontal = 4.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("What do you call them?", style = display(24), modifier = Modifier.semantics { heading() })
            Text("They didn't share a name. Only you will see this one.", color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp)
            LabelledInput("Their name", name, { name = it }, "A name, any name", fontSize = 17, capitalization = KeyboardCapitalization.Words)
            ErrorText(error)
            PrimaryButton("Save", { onSave(name) }, enabled = name.isNotBlank())
        }
    }
}

/** Touch / QR code: a sunken pill, the one chosen lifted on the card's white. */
@Composable
private fun MeetTabs(selected: Int, onSelect: (Int) -> Unit) {
    Row(
        Modifier.padding(horizontal = 20.dp, vertical = 4.dp).fillMaxWidth().clip(RoundedCornerShape(26.dp)).background(Fob.sunken).padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        listOf(Icons.touch to "NFC", Icons.request to "QR code").forEachIndexed { i, (icon, label) ->
            val on = i == selected
            Row(
                Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(22.dp)).background(if (on) Fob.card else Color.Transparent)
                    .clickable(role = Role.Tab, onClickLabel = label) { onSelect(i) },
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
            ) {
                StrokeIcon(icon, tint = if (on) Fob.ink else Fob.muted, size = 20.dp, width = 1.8f)
                Text(label, color = if (on) Fob.ink else Fob.muted, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            }
        }
    }
}

/** Touch phones' rings round your photo, how to hold the phones, and a warning when NFC can't listen. */
@Composable
private fun TouchTab(profile: Profile?, loaded: Boolean, touch: TouchState, onSettings: () -> Unit) {
    val problem by TouchPhones.problem.collectAsState()
    val context = androidx.compose.ui.platform.LocalContext.current
    // The warning (when there is one) sits 16 dp under the tabs.
    val warned = touch.nfc == Nfc.OFF || !touch.meet
    Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = if (warned) 16.dp else 0.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        when {
            touch.nfc == Nfc.OFF -> Warning("Tap to turn it on, then come back.", title = "NFC is off", icon = Icons.touch, onClick = { openNfcSettings(context) })
            !touch.meet -> Warning("Turn on Meet under NFC in Settings to swap profiles by holding phones together.", title = "Meet by NFC is off", icon = Icons.touch, onClick = onSettings)
        }
    }
    // Off, the warning is all there is: no rings, photo or instructions for something that can't work.
    if (warned) return
    val phase = rememberWavePhase()
    Column(
        Modifier.fillMaxWidth().padding(top = 24.dp, start = 28.dp, end = 28.dp, bottom = 24.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(28.dp),
    ) {
        Box(Modifier.size(220.dp), contentAlignment = Alignment.Center) {
            // Read as the layer draws: the breathing recomposes nothing.
            val breathe: GraphicsLayerScope.() -> Unit = { val b = 0.97f + 0.03f * kotlin.math.sin(phase.value * 6.2831855f); scaleX = b; scaleY = b }
            Box(Modifier.matchParentSize().graphicsLayer(breathe).border2(Fob.line))
            Box(Modifier.matchParentSize().padding(30.dp).graphicsLayer(breathe).border2(Fob.track))
            Box(Modifier.matchParentSize().padding(60.dp).clip(CircleShape).background(Fob.highlight))
            profile?.let { Avatar(it.name, it.initial, it.photo, 88.dp) } ?: if (loaded) StrokeIcon(Icons.meet, tint = Fob.muted, size = 40.dp, width = 1.8f) else Unit
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Hold phones back to back", style = display(26), textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() })
            val nameless = if (profile == null && loaded) " Your name isn't shared: they choose what to call you." else ""
            Text("The other person opens Meet on their Fob too.$nameless", color = Fob.muted, fontSize = 20.sp, lineHeight = 29.sp, textAlign = TextAlign.Center)
        }
        problem?.let { Text(it, fontSize = 14.sp, lineHeight = 20.sp, textAlign = TextAlign.Center, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.size(9.dp).clip(CircleShape).background(Fob.greenDot))
            Text("Ready · NFC on", color = Fob.muted, fontSize = 14.sp)
        }
    }
}

private fun Modifier.border2(color: Color): Modifier = this.then(Modifier.border(2.dp, color, CircleShape))

/**
 * Your code for the other person to scan: the white card with your name and
 * "Scan with Fob to add me", as bright as the screen goes. [saved] is the
 * line over it after you scanned theirs ("Sam saved").
 */
@Composable
private fun YourCode(profile: Profile?, contact: Contact?, code: String?, savedLine: String?, hint: String) {
    FullBrightness()
    val qr = remember(code) { code?.let { runCatching { qrCode(it) }.getOrNull() } }
    Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(14.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        savedLine?.let {
            Row(
                Modifier.clip(RoundedCornerShape(20.dp)).background(Fob.greenFill.copy(alpha = 0.22f)).heightIn(min = 40.dp).padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                StrokeIcon(Icons.check, tint = Fob.green, size = 18.dp, width = 2.4f)
                Text(it, color = Fob.green, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        // The code on a white card as Receive's: 12 dp of white, and the code's own 2-module quiet zone, as wide as the page
        // allows (at most 360 dp of code).
        BoxWithConstraints(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            val side: Dp = (maxWidth - 24.dp).coerceAtMost(360.dp)
            Column(
                Modifier.clip(RoundedCornerShape(14.dp)).background(Fob.qrLight).padding(12.dp),
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (qr != null) QrImage(qr, side, description = "Your code", quiet = 2) else Box(Modifier.size(side))
            }
        }
        Text(hint, color = Fob.muted, fontSize = 14.sp, lineHeight = 20.sp, textAlign = TextAlign.Center)
    }
}

/** The camera on their code, with a close button and a word, as Scan a profile was. */
@Composable
private fun ScanView(network: uniffi.fob.Network, notice: String?, onClose: () -> Unit, onText: (String) -> Unit) {
    Box(Modifier.fillMaxSize().background(Fob.camera)) {
        CameraOrAsk(Modifier.fillMaxSize(), onText = onText)
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                EdgeIconButton(Icons.close, "Close", onClose, tint = Fob.onCamera)
                Text("Scan their code", color = Fob.onCamera, fontSize = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).semantics { heading() })
            }
            BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
                val side: Dp = minOf(260.dp, maxWidth - 48.dp, maxHeight - 140.dp).coerceAtLeast(0.dp)
                Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                    if (side > 24.dp) Viewfinder(side, 6f)
                    Text("Point at their code on Meet", color = Fob.onCamera, fontSize = 15.sp, lineHeight = 21.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 24.dp, vertical = 16.dp))
                    notice?.let { Text(it, color = Fob.cameraWarning, fontSize = 14.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 24.dp)) }
                }
            }
        }
    }
}

/**
 * Met: both of you, a swap between your avatars, and the two addresses
 * each way, as Send and Receive show them. A QR meet has no photo, which a
 * line says how to get.
 */
@Composable
private fun MetView(model: WalletModel, nav: Nav, profile: Profile?, share: ProfileShare?, contact: Contact, viaTouch: Boolean) {
    val touch = rememberTouchState(model)
    Page(bottom = {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlineButton("Edit", { nav.replace(Screen.ContactEdit(contact.id)) }, Modifier.weight(1f))
            PrimaryButton("Done", { nav.back() }, Modifier.weight(1f))
        }
    }) {
        Column(
            Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 72.dp),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                Avatar(profile?.name ?: "You", profile?.initial ?: "?", profile?.photo, 80.dp, describe = true)
                Box(Modifier.size(44.dp).clip(CircleShape).background(Fob.greenFill), contentAlignment = Alignment.Center) { StrokeIcon(Icons.swap, tint = Fob.onGreen, size = 22.dp, width = 2.4f) }
                Avatar(contact.name, contact.initial, contact.photo, 80.dp, describe = true)
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Met ${contact.name}", style = display(34), textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.semantics { heading() })
                Text("Saved to Contacts. ${contact.name} has your address too.", color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp, textAlign = TextAlign.Center)
            }
            Card {
                contact.sendAddress?.let { AddressRow(Icons.up, "Send to ${contact.name}", it) }
                if (contact.sendAddress != null && share != null) Divider()
                share?.let { AddressRow(Icons.down, "Receive from ${contact.name}", it.address) }
            }
            if (contact.photo == null && touch.nfc != Nfc.NONE) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    StrokeIcon(Icons.touch, tint = Fob.orangeText, size = 18.dp, width = 1.8f)
                    Text("No photo yet. Meet by NFC to get it.", color = Fob.muted, fontSize = 13.sp)
                }
            }
        }
    }
}

/** An address one way: the arrow, what it's for, the address in two parts, and its pattern. */
@Composable
private fun AddressRow(arrow: String, label: String, address: String) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 68.dp).padding(horizontal = 20.dp, vertical = 8.dp).semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Box(Modifier.size(32.dp).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) { StrokeIcon(arrow, size = 18.dp, width = 2.2f) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(label, fontSize = 16.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(short(address, 2), fontFamily = FontFamily.Monospace, fontSize = 13.sp, color = Fob.muted, maxLines = 1)
        }
        AddressPattern(address, 32.dp)
    }
}

/**
 * The swap while Meet's Touch tab is open and [allowed]: made on the share's
 * own address, answering and reading only while the screen is in front, and
 * [onSwapped] once theirs came. Leaving ends it, kept as a share when ours
 * went out.
 */
@Composable
private fun RememberSwap(model: WalletModel, id: ULong?, allowed: Boolean, onSwapped: (Swapped) -> Unit) {
    val activity = LocalActivity.current
    val scope = rememberCoroutineScope()
    val swappedNow by rememberUpdatedState(onSwapped)
    var swap by remember { mutableStateOf<TouchSwap?>(null) }
    LaunchedEffect(allowed, id) { if (allowed && id != null && swap == null) swap = runCatching { model.startSwap(id) }.getOrNull() }
    val s = swap
    DisposableEffect(s) { onDispose { if (s != null) model.endSwapLater(s) } }
    if (s != null && activity != null && allowed) {
        LifecycleResumeEffect(s) {
            val runner = SwapRunner(activity, s, scope)
            runner.start()
            onPauseOrDispose { runner.stop() }
        }
    }
    LaunchedEffect(s) {
        if (s == null) return@LaunchedEffect
        TouchPhones.swapped.first { it === s }
        runCatching { model.endSwap(s) }.getOrNull()?.let { swappedNow(it) }
    }
}
