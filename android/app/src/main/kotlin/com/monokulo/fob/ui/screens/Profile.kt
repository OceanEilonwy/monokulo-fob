package com.monokulo.fob.ui.screens

import android.content.ClipData
import android.content.Intent
import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.border
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.monokulo.fob.SwapRunner
import com.monokulo.fob.TouchPhones
import com.monokulo.fob.nfcState
import kotlinx.coroutines.flow.first
import uniffi.fob.ProfileChangeKind
import uniffi.fob.Swapped
import uniffi.fob.TouchSwap
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import com.monokulo.fob.Nfc
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.openNfcSettings
import com.monokulo.fob.ui.rememberTouchState
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
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
import com.monokulo.fob.ui.BarIcons
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.EdgeButton
import com.monokulo.fob.ui.EdgeIconButton
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PhotoButton
import com.monokulo.fob.ui.PhotoSheet
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.SettingRow
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import com.monokulo.fob.ui.rememberPhotoChooser
import kotlinx.coroutines.launch
import uniffi.fob.Profile
import uniffi.fob.ProfileShare
import uniffi.fob.ScannedProfile
import uniffi.fob.ShareKind
import uniffi.fob.ShareLine
import uniffi.fob.contactInitial
import uniffi.fob.profileCodeProblem
import uniffi.fob.qrCode

/**
 * The profile's editor (Round 6 › "New wallet, last step: make it yours
 * (optional)"): a photo with the camera badge, a name, Continue. As a new
 * or restored wallet's last step it says which step, with Skip; from Your
 * profile's Edit it has a Back arrow and Save. Saved for the first time, it
 * opens Your profile.
 */
@Composable
fun ProfileEditScreen(model: WalletModel, nav: Nav, step: String?) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var photo by remember { mutableStateOf<ByteArray?>(null) }
    // Whether there was a profile before: saving the first opens Your profile.
    var had by remember { mutableStateOf<Boolean?>(null) }
    var choosing by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    // Asked once, as the last step ends: Meet and touch to pay need NFC on, which only Android's settings can do.
    val touch = rememberTouchState(model)
    var askingNfc by remember { mutableStateOf(false) }
    fun finish() { if (had == true) nav.back() else nav.replace(Screen.Profile) }
    LaunchedEffect(wallet) {
        val w = wallet ?: return@LaunchedEffect
        if (had != null) return@LaunchedEffect
        val p = w.profile()
        if (p != null) { name = p.name; photo = p.photo }
        had = p != null
    }
    val chooser = rememberPhotoChooser { made -> if (made != null) { photo = made; error = null } else error = "That photo couldn't be made small enough to keep." }

    fun save() {
        busy = true
        error = null
        scope.launch {
            runCatching { model.setProfile(name, photo) }
                .onSuccess { if (step != null && touch.nfc == Nfc.OFF && touch.meet) askingNfc = true else finish() }
                .onFailure { error = it.reason() }
            busy = false
        }
    }

    Box(Modifier.fillMaxSize()) {
        Page(bottom = {
            ErrorText(error)
            PrimaryButton(if (step != null) "Continue" else "Save", ::save, enabled = !busy && had != null)
        }) {
            if (step != null) {
                Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(start = 20.dp, end = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically) {
                    Text(step, color = Fob.muted, fontSize = 14.sp, modifier = Modifier.weight(1f))
                    TextButton("Skip", { nav.back() })
                }
            } else {
                TopBar("Edit your profile", onBack = { nav.back() })
            }
            Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 16.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (step != null) Text("Make it yours", style = display(30), modifier = Modifier.semantics { heading() })
                    Text(
                        "A name and photo let other Fob users save you in one scan.",
                        color = Fob.muted, fontSize = 16.sp, lineHeight = 23.sp,
                    )
                }
                Box(Modifier.fillMaxWidth().padding(top = 8.dp), contentAlignment = Alignment.Center) {
                    PhotoButton(name, remember(name) { contactInitial(name) }, photo, 128.dp) { choosing = true }
                }
                LabelledInput("Your name", name, { name = it }, "A name, any name", fontSize = 17, capitalization = KeyboardCapitalization.Words)
            }
        }
        if (choosing) {
            PhotoSheet("Your photo", photo != null, chooser, onRemove = { photo = null }, onDismiss = { choosing = false })
        }
        if (askingNfc) {
            val context = LocalContext.current
            BottomSheet("Meet and pay by NFC", onDismiss = { askingNfc = false; finish() }) {
                SheetHandle()
                StrokeIcon(Icons.touch, tint = Fob.orangeText, size = 36.dp, width = 1.8f)
                Text("Meet and pay by NFC", style = display(24), modifier = Modifier.semantics { heading() })
                Text(
                    "Turn NFC on to swap contacts and take payments by holding phones together. Fob only uses it while one of its screens is open.",
                    color = Fob.muted, fontSize = 15.sp, lineHeight = 22.sp,
                )
                PrimaryButton("Turn on NFC", { askingNfc = false; openNfcSettings(context); finish() })
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { TextButton("Not now", { askingNfc = false; finish() }) }
            }
        }
    }
}

/**
 * Your profile (Round 6 › "Your profile: every share listed"): the photo
 * and name with Edit, Share my profile, "Name in payment requests", and
 * every share, newest first, with what came to its address.
 */
@Composable
fun ProfileScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val activity by model.activity.collectAsState()
    val scope = rememberCoroutineScope()
    var profile by remember { mutableStateOf<Profile?>(null) }
    var shares by remember { mutableStateOf<List<ShareLine>>(emptyList()) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(wallet, activity) {
        val w = wallet ?: return@LaunchedEffect
        // None yet (Settings opens the editor then): set it up first.
        profile = w.profile() ?: run { nav.replace(Screen.ProfileEdit()); return@LaunchedEffect }
        shares = model.profileShares()
    }
    val p = profile ?: return Page { TopBar("Your profile", onBack = { nav.back() }) }
    Page {
        TopBar("Your profile", onBack = { nav.back() }) { TextButton("Edit", { nav.go(Screen.ProfileEdit()) }) }
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Avatar(p.name, p.initial, p.photo, 80.dp, describe = true)
                Text(p.name, style = display(26), textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() })
            }
            PrimaryButton("Meet", { nav.go(Screen.Meet) }, enabled = !busy, leading = Icons.meet)
            ErrorText(error)
            Card {
                SettingRow(
                    "Name in payment requests",
                    checked = p.nameInRequests,
                    onChecked = { on -> scope.launch { runCatching { wallet?.setNameInRequests(on) }.onSuccess { profile = wallet?.profile() } } },
                )
            }
            Card {
                Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 10.dp, bottom = 2.dp)) {
                    Text("Your shares", fontSize = 14.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f).semantics { heading() })
                }
                if (shares.isEmpty()) {
                    Text(
                        "None yet. Each Share my profile gives a new address, listed here with what's paid to it.",
                        color = Fob.muted, fontSize = 13.sp, lineHeight = 19.sp, modifier = Modifier.padding(horizontal = 20.dp, vertical = 12.dp),
                    )
                }
                shares.forEach { s -> ShareRow(s) { nav.go(Screen.ProfileShare(s.id)) } }
            }
        }
    }
}

/** A share on Your profile: how it went out, when, what came and its address's pattern. */
@Composable
private fun ShareRow(s: ShareLine, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button, onClickLabel = "Show its code", onClick = onClick)
            .semantics(mergeDescendants = true) {}.padding(horizontal = 20.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(32.dp).clip(CircleShape).background(Fob.sunken), contentAlignment = Alignment.Center) {
            when (s.kind) {
                is ShareKind.CodeShown -> StrokeIcon(Icons.request, size = 16.dp)
                is ShareKind.TouchedPhones -> StrokeIcon(Icons.touch, size = 16.dp, width = 1.8f)
                is ShareKind.Met -> StrokeIcon(Icons.meet, size = 16.dp, width = 1.8f)
                else -> StrokeIcon(Icons.link, size = 16.dp)
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(s.title, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(s.`when`, color = Fob.muted, fontSize = 12.sp)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(s.received, fontSize = 14.sp, fontWeight = FontWeight.Medium, color = if (s.any) Fob.green else Fob.muted)
            AddressPattern(s.address, 18.dp)
        }
    }
}

/**
 * A share's code again, from Your shares or Activity: white in both themes
 * with the screen at full brightness. Meet makes the shares and does the
 * touching; this only shows one.
 */
@Composable
fun ProfileShareScreen(model: WalletModel, nav: Nav, id: ULong) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    var share by remember { mutableStateOf<ProfileShare?>(null) }
    var profile by remember { mutableStateOf<Profile?>(null) }
    LaunchedEffect(wallet) {
        val w = wallet ?: return@LaunchedEffect
        share = w.shareCode(id) ?: run { nav.back(); return@LaunchedEffect }
        profile = w.profile()
    }
    ShareCode(model, nav, id, share, profile, onWentOut = { scope.launch { wallet?.shareCode(id)?.let { share = it } } })
}

/**
 * A share's code: white in both themes with the screen as bright as it
 * goes, the photo and name over a dense code as wide as the screen, the
 * touch phones strip when the phone has NFC, then Copy link and Share,
 * each kept as how the share went out. Close goes back.
 */
@Composable
private fun ShareCode(model: WalletModel, nav: Nav, id: ULong, share: ProfileShare?, profile: Profile?, onWentOut: () -> Unit) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    FullBrightness()
    // White in both themes: the bars' icons dark on it.
    BarIcons(darkIcons = true)
    val code = share?.code
    val qr = remember(code) { code?.let { runCatching { qrCode(it) }.getOrNull() } }

    /** Kept as how the share went out: Your shares says "Link copied" or "Link shared". */
    fun wentOut(kind: ShareKind) {
        scope.launch { runCatching { wallet?.shareWentOut(id, kind) }; onWentOut() }
    }

    Column(Modifier.fillMaxSize().background(Fob.qrLight).safeDrawingPadding()) {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically) {
            EdgeIconButton(Icons.close, "Close", { nav.back() }, tint = Fob.qrDark)
        }
        BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
            // As wide as the screen allows, leaving room for the words; the quiet zone is the code's own margin.
            val side: Dp = minOf(maxWidth - 16.dp, maxHeight - 150.dp).coerceAtLeast(160.dp)
            Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(18.dp, Alignment.CenterVertically),
            ) {
                profile?.let { p ->
                    Row(Modifier.padding(horizontal = 24.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Avatar(p.name, p.initial, p.photo, 52.dp)
                        Column(Modifier.weight(1f, fill = false)) {
                            Text(p.name, style = display(24), color = Fob.qrDark, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text("Scan with Fob to add me", color = Fob.qrMuted, fontSize = 14.sp)
                        }
                    }
                }
                if (qr != null) QrImage(qr, side, description = "Your profile's code", quiet = 4) else Spacer(Modifier.size(side))
            }
        }
        Row(Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 20.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlineButton("Copy link", {
                val c = code ?: return@OutlineButton
                scope.launch { clipboard.setClipEntry(ClipData.newPlainText("Fob profile", c).toClipEntry()) }
                wentOut(ShareKind.LinkCopied)
            }, Modifier.weight(1f), enabled = code != null, tint = Fob.qrDark, edge = Fob.qrMuted)
            PrimaryButton("Share", {
                val c = code ?: return@PrimaryButton
                context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, c), null))
                wentOut(ShareKind.LinkShared)
            }, Modifier.weight(1f), enabled = code != null)
        }
    }
}

/**
 * What another Fob's scanned profile shows (Round 6 › "Another Fob scans
 * the code"): the photo, name, the address's pattern and three parts, whose
 * the name and photo are, Pay (Send's form with To filled) and Add contact.
 * When a contact has this name or this address, Add contact first shows
 * what would change and asks (decision 5): Update, Keep as is, or Add as
 * new contact. Nothing changes without a press.
 */
@Composable
fun ProfileFound(
    model: WalletModel, nav: Nav, text: String, found: ScannedProfile, onPay: () -> Unit, onClose: () -> Unit,
    startAsking: Boolean = false, met: Boolean = false, openContact: (ULong) -> Unit = { nav.go(Screen.Contact(it)) },
) {
    val scope = rememberCoroutineScope()
    var asking by remember(found) { mutableStateOf(startAsking) }
    var error by remember(found) { mutableStateOf<String?>(null) }

    fun open(change: suspend (uniffi.fob.FobWallet) -> uniffi.fob.Contact) {
        error = null
        scope.launch {
            runCatching { model.changeContacts(change) }.onSuccess { onClose(); openContact(it.id) }.onFailure { error = it.reason() }
        }
    }

    val clash = found.clash
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(Fob.paper)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
            .padding(start = 20.dp, end = 20.dp, top = 0.dp, bottom = 20.dp)
            .semantics { contentDescription = "${found.name}'s profile" },
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // The handle: tap it to close, as Back does.
        Box(
            Modifier.fillMaxWidth().height(28.dp).clickable(role = Role.Button, onClickLabel = "Close", onClick = onClose),
            contentAlignment = Alignment.Center,
        ) { Box(Modifier.width(36.dp).height(4.dp).clip(RoundedCornerShape(2.dp)).background(Fob.outline)) }
        if (asking && clash != null) {
            Text(clash.why, style = display(20), textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() })
            if (clash.photoChanges) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Avatar(clash.contact.name, clash.contact.initial, clash.contact.photo, 56.dp, describe = true)
                    StrokeIcon(Icons.arrow, tint = Fob.muted, size = 20.dp)
                    Avatar(found.name, found.initial, found.photo, 56.dp, describe = true)
                }
            }
            if (clash.changes.isNotEmpty()) {
                Card(Modifier.fillMaxWidth(), padding = PaddingValues(horizontal = 16.dp, vertical = 10.dp)) {
                    clash.changes.forEach { c ->
                        Column(Modifier.fillMaxWidth().padding(vertical = 6.dp).semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(c.label, color = Fob.muted, fontSize = 12.5.sp)
                            Text(c.now, fontSize = 14.sp, color = Fob.muted, fontFamily = if (c.kind == ProfileChangeKind.SEND_ADDRESS) FontFamily.Monospace else FontFamily.Default)
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                StrokeIcon(Icons.arrow, tint = Fob.orangeText, size = 14.dp)
                                Text(c.new, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, fontFamily = if (c.kind == ProfileChangeKind.SEND_ADDRESS) FontFamily.Monospace else FontFamily.Default)
                            }
                        }
                    }
                }
            }
            Text(clash.ask, color = Fob.muted, fontSize = 13.5.sp, lineHeight = 19.sp, textAlign = TextAlign.Center)
            ErrorText(error)
            PrimaryButton(clash.update, {
                if (clash.changes.isEmpty()) { onClose(); openContact(clash.contact.id) }
                else open { it.updateContactFromProfile(clash.contact.id, text, met) }
            }, height = 52.dp)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlineButton(clash.keep, { asking = false }, Modifier.weight(1f), height = 52.dp)
                // Only for a name: a contact's address is never given to a second one (Rust).
                clash.addNew?.let { OutlineButton(it, { open { w -> w.addContactFromProfile(text, true, met) } }, Modifier.weight(1f), height = 52.dp) }
            }
        } else {
            Avatar(found.name, found.initial, found.photo, 88.dp, describe = true)
            Text(found.name, style = display(26), textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                AddressPattern(found.address, 24.dp)
                Text(found.addressShort, fontFamily = FontFamily.Monospace, fontSize = 13.5.sp, color = Fob.muted, maxLines = 1)
            }
            Text(found.note, color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, textAlign = TextAlign.Center)
            ErrorText(error)
            Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlineButton(found.pay, onPay, Modifier.weight(1f), height = 52.dp)
                PrimaryButton(found.add, { if (clash != null) asking = true else open { it.addContactFromProfile(text, false, met) } }, Modifier.weight(1f), height = 52.dp)
            }
        }
    }
}

/**
 * Scanning another Fob's profile, from Contacts: the camera full screen,
 * close and title below the status bar, the profile on a sheet above the
 * gesture bar. A code that isn't a profile says why.
 */
@Composable
fun ProfileScanScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val network by model.network.collectAsState()
    val scope = rememberCoroutineScope()
    var found by remember { mutableStateOf<Pair<String, ScannedProfile>?>(null) }
    var notice by remember { mutableStateOf<String?>(null) }
    // The camera reads a code many times a second: each one is taken once.
    var lastRead by remember { mutableStateOf<String?>(null) }
    // Closing the sheet keeps the camera from taking the same code again until another is read.
    BackHandler(enabled = found != null) { found = null }
    Box(Modifier.fillMaxSize().background(Fob.camera)) {
        CameraOrAsk(Modifier.fillMaxSize(), onText = { text ->
            if (found != null || text == lastRead) return@CameraOrAsk
            lastRead = text
            val problem = profileCodeProblem(network, text)
            if (problem != null) { notice = problem; return@CameraOrAsk }
            scope.launch { runCatching { wallet?.scannedProfile(text) }.onSuccess { s -> if (s != null) { found = text to s; notice = null } }.onFailure { notice = it.reason() } }
        })
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                EdgeIconButton(Icons.close, "Close", { nav.back() }, tint = Fob.onCamera)
                Text("Scan a profile", color = Fob.onCamera, fontSize = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).semantics { heading() })
            }
            if (found == null) {
                BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
                    val side: Dp = minOf(260.dp, maxWidth - 48.dp, maxHeight - 140.dp).coerceAtLeast(0.dp)
                    Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                        if (side > 24.dp) Viewfinder(side, 6f)
                        Text(
                            "Point at the code on another Fob's Share my profile", color = Fob.onCamera, fontSize = 15.sp, lineHeight = 21.sp,
                            textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 24.dp, vertical = 16.dp),
                        )
                        notice?.let { Text(it, color = Fob.cameraWarning, fontSize = 14.sp, textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 24.dp)) }
                    }
                }
            }
        }
        found?.let { (text, s) ->
            Box(Modifier.align(Alignment.BottomCenter)) {
                ProfileFound(
                    model, nav, text, s,
                    // Send's form, filled with their address and the code's name, the camera not taking the code again.
                    onPay = { nav.replace(Screen.Send(draft = SendDraft(s.address, code = text, read = text))) },
                    onClose = { found = null },
                )
            }
        }
    }
}
