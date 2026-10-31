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
import com.monokulo.fob.Nfc
import com.monokulo.fob.SwapRunner
import com.monokulo.fob.TouchPhones
import com.monokulo.fob.nfcState
import kotlinx.coroutines.flow.first
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
import androidx.compose.material3.IconButton
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
                .onSuccess { if (had == true) nav.back() else nav.replace(Screen.Profile) }
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
                Row(Modifier.fillMaxWidth().height(64.dp).padding(start = 20.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
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
                        "A name and photo let other Fob users save you in one scan. They go in your payment codes, nowhere else.",
                        color = Fob.muted, fontSize = 16.sp, lineHeight = 23.sp,
                    )
                }
                Box(Modifier.fillMaxWidth().padding(top = 8.dp), contentAlignment = Alignment.Center) {
                    PhotoButton(name, remember(name) { contactInitial(name) }, photo, 128.dp) { choosing = true }
                }
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    LabelledInput("Your name", name, { name = it }, "A name, any name", fontSize = 17, capitalization = KeyboardCapitalization.Words)
                    Text(
                        "Anything you like; it doesn't have to be your real name. Change it any time in Settings.",
                        color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, modifier = Modifier.padding(start = 4.dp),
                    )
                }
            }
        }
        if (choosing) {
            PhotoSheet(
                "Your photo", photo != null, chooser,
                "Cropped to a circle and kept small, in this wallet's file. It leaves the phone only in your profile's code, when you share it.",
                onRemove = { photo = null }, onDismiss = { choosing = false },
            )
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
            PrimaryButton("Share my profile", {
                busy = true
                error = null
                scope.launch {
                    // Kept as a share now, however briefly its code shows.
                    runCatching { model.shareProfile() }.onSuccess { nav.go(Screen.ProfileShare(it.id, fresh = true)) }.onFailure { error = it.reason() }
                    busy = false
                }
            }, enabled = !busy, leading = Icons.request)
            ErrorText(error)
            Card {
                SettingRow(
                    "Name in payment requests", "Requests carry your name; only this profile code carries the photo",
                    checked = p.nameInRequests,
                    onChecked = { on -> scope.launch { runCatching { wallet?.setNameInRequests(on) }.onSuccess { profile = wallet?.profile() } } },
                )
            }
            Card {
                Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 10.dp, bottom = 2.dp)) {
                    Text("Your shares", fontSize = 14.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f).semantics { heading() })
                    Text("each its own address", color = Fob.muted, fontSize = 14.sp)
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
 * Share my profile (Round 6 › "Share my profile: code, or touch phones"):
 * the share's code, and while it's open with NFC on, touch phones: this
 * phone answers as a card and now and then reads, so another Fob on this
 * screen held to it swaps profiles, on this share's address. The strip
 * opens Touch phones, which says how; either way a swap ends on Swapped,
 * with a buzz. Only a share just made ([fresh]) that Rust says can still
 * swap offers it: one opened again from Your shares or Activity shows its
 * code alone, so its address goes to no second person.
 */
@Composable
fun ProfileShareScreen(model: WalletModel, nav: Nav, id: ULong, fresh: Boolean) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    val view = LocalView.current
    var share by remember { mutableStateOf<ProfileShare?>(null) }
    var profile by remember { mutableStateOf<Profile?>(null) }
    var touching by remember { mutableStateOf(false) }
    var swapped by remember { mutableStateOf<Swapped?>(null) }
    LaunchedEffect(wallet) {
        val w = wallet ?: return@LaunchedEffect
        share = w.shareCode(id) ?: run { nav.back(); return@LaunchedEffect }
        profile = w.profile()
    }
    val touch = fresh && share?.canTouch == true
    val nfc = rememberTouchPhones(model, id, allowed = touch, running = swapped == null) { done ->
        swapped = done
        view.performHapticFeedback(if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.LONG_PRESS)
    }
    BackHandler(enabled = touching && swapped == null) { touching = false }
    val done = swapped
    when {
        done != null -> SwappedView(model, nav, profile, done)
        touching && touch -> TouchView(profile, onBack = { touching = false })
        else -> ShareCode(
            model, nav, id, share, profile, if (touch) nfc else Nfc.NONE, onTouch = { touching = true },
            // A link out ends the swap (Rust): the strip goes.
            onWentOut = { scope.launch { wallet?.shareCode(id)?.let { share = it } } },
        )
    }
}

/**
 * Touch phones while Share my profile is open: NFC as it is now (looked at
 * again on coming back, from its settings say), the swap once it's on and
 * [allowed], answering and reading only while the screen is in front and
 * [running], and [onSwapped] once theirs came. Leaving ends the swap, kept
 * as a share when ours went out.
 */
@Composable
private fun rememberTouchPhones(model: WalletModel, id: ULong, allowed: Boolean, running: Boolean, onSwapped: (Swapped) -> Unit): Nfc {
    val context = LocalContext.current
    val activity = LocalActivity.current
    val scope = rememberCoroutineScope()
    val swappedNow by rememberUpdatedState(onSwapped)
    var nfc by remember { mutableStateOf(nfcState(context)) }
    var swap by remember { mutableStateOf<TouchSwap?>(null) }
    LifecycleResumeEffect(Unit) {
        nfc = nfcState(context)
        onPauseOrDispose {}
    }
    // Made once NFC is on, on the share's own address.
    LaunchedEffect(nfc, allowed) { if (nfc == Nfc.ON && allowed && swap == null) swap = runCatching { model.startSwap(id) }.getOrNull() }
    val s = swap
    DisposableEffect(s) { onDispose { if (s != null) model.endSwapLater(s) } }
    if (s != null && activity != null && running && allowed) {
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
    return nfc
}

/** "Or touch phones", on the white of the code: opens Touch phones, or NFC's settings while it's off. */
@Composable
private fun TouchStrip(nfc: Nfc, onClick: () -> Unit) {
    Row(
        Modifier.padding(horizontal = 24.dp).clip(RoundedCornerShape(20.dp)).background(Fob.qrStrip)
            .clickable(role = Role.Button, onClick = onClick).semantics(mergeDescendants = true) {}
            .heightIn(min = 48.dp).padding(start = 12.dp, end = 16.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        StrokeIcon(Icons.touch, tint = Fob.qrAccent, size = 28.dp, width = 1.8f)
        Column {
            Text("Or touch phones", color = Fob.qrDark, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Text(
                if (nfc == Nfc.ON) "Both on this screen, backs together: you swap" else "NFC is off. Tap to turn it on, then come back.",
                color = Fob.qrMuted, fontSize = 12.5.sp, lineHeight = 17.sp,
            )
        }
    }
}

/**
 * Touch phones (Round 6 › "Touch phones"): rings round your photo, how to
 * hold the phones, why the last touch took nothing, and the way back to
 * the code. The swap runs as on the code.
 */
@Composable
private fun TouchView(profile: Profile?, onBack: () -> Unit) {
    val problem by TouchPhones.problem.collectAsState()
    Column(Modifier.fillMaxSize().background(Fob.paper).safeDrawingPadding()) {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onBack, modifier = Modifier.semantics { contentDescription = "Back to the code" }) { StrokeIcon(Icons.close, size = 20.dp) }
        }
        Column(
            Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(start = 28.dp, end = 28.dp, bottom = 60.dp),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(28.dp, Alignment.CenterVertically),
        ) {
            Box(Modifier.size(220.dp), contentAlignment = Alignment.Center) {
                Box(Modifier.matchParentSize().border(2.dp, Fob.line, CircleShape))
                Box(Modifier.matchParentSize().padding(30.dp).border(2.dp, Fob.track, CircleShape))
                Box(Modifier.matchParentSize().padding(60.dp).clip(CircleShape).background(Fob.highlight))
                profile?.let { Avatar(it.name, it.initial, it.photo, 88.dp) }
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Touch phones", style = display(26), textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() })
                Text(
                    "Hold the backs of the two phones together. The other person needs Fob open on Share my profile too.",
                    color = Fob.muted, fontSize = 16.sp, lineHeight = 23.sp, textAlign = TextAlign.Center,
                )
            }
            problem?.let { Text(it, fontSize = 14.sp, lineHeight = 20.sp, textAlign = TextAlign.Center, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
            Box(Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(24.dp)).clickable(role = Role.Button, onClick = onBack).padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
                Text("No NFC? Scan each other's code instead.", color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.Center)
            }
        }
    }
}

/**
 * Swapped (Round 6 › "Swapped: both got each other"): both photos with a
 * tick, Rust's words, their name and address, then Not now (back to Your
 * profile) or Add contact, which asks first when it would change a
 * contact (decision 5), as a scanned profile does.
 */
@Composable
private fun SwappedView(model: WalletModel, nav: Nav, profile: Profile?, done: Swapped) {
    val scope = rememberCoroutineScope()
    val found = done.profile
    var asking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    BackHandler(enabled = asking) { asking = false }
    Box(Modifier.fillMaxSize().background(Fob.paper)) {
        Column(Modifier.fillMaxSize().safeDrawingPadding()) {
            Column(
                Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = 40.dp),
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(18.dp),
            ) {
                Box(Modifier.width(194.dp).height(80.dp)) {
                    profile?.let { Avatar(it.name, it.initial, it.photo, 80.dp, Modifier.align(Alignment.CenterStart), describe = true) }
                    Avatar(found.name, found.initial, found.photo, 80.dp, Modifier.align(Alignment.CenterEnd), describe = true)
                    Box(
                        Modifier.align(Alignment.Center).size(44.dp).clip(CircleShape).background(Fob.paper).padding(4.dp).clip(CircleShape).background(Fob.greenFill),
                        contentAlignment = Alignment.Center,
                    ) { StrokeIcon(Icons.check, tint = Fob.onGreen, size = 20.dp, width = 2.8f) }
                }
                Text(done.title, style = display(26), textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() })
                Text(done.line, color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp, textAlign = TextAlign.Center)
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(Fob.card).padding(horizontal = 20.dp, vertical = 16.dp)
                        .semantics(mergeDescendants = true) {},
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Avatar(found.name, found.initial, found.photo, 48.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(found.name, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            AddressPattern(found.address, 18.dp)
                            Text(found.addressShort, fontFamily = FontFamily.Monospace, fontSize = 13.sp, color = Fob.muted, maxLines = 1)
                        }
                    }
                }
                ErrorText(error)
            }
            Row(Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 20.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlineButton("Not now", { nav.back() }, Modifier.weight(1f))
                PrimaryButton(found.add, {
                    if (found.clash != null) asking = true
                    else scope.launch {
                        error = null
                        runCatching { model.changeContacts { it.addContactFromProfile(done.text, false) } }
                            .onSuccess { nav.replace(Screen.Contact(it.id)) }.onFailure { error = it.reason() }
                    }
                }, Modifier.weight(1f))
            }
        }
        if (asking) {
            Box(Modifier.fillMaxSize().background(Fob.scrim).clickable(onClickLabel = "Close", indication = null, interactionSource = null) { asking = false })
            Box(Modifier.align(Alignment.BottomCenter)) {
                ProfileFound(
                    model, nav, done.text, found,
                    onPay = { nav.replace(Screen.Send(draft = SendDraft(found.address, code = done.text, read = done.text))) },
                    onClose = { asking = false }, startAsking = true, openContact = { nav.replace(Screen.Contact(it)) },
                )
            }
        }
    }
}

/**
 * A share's code: white in both themes with the screen as bright as it
 * goes, the photo and name over a dense code as wide as the screen, the
 * touch phones strip when the phone has NFC, then Copy link and Share,
 * each kept as how the share went out. Close goes back.
 */
@Composable
private fun ShareCode(model: WalletModel, nav: Nav, id: ULong, share: ProfileShare?, profile: Profile?, nfc: Nfc, onTouch: () -> Unit, onWentOut: () -> Unit) {
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
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { nav.back() }, modifier = Modifier.semantics { contentDescription = "Close" }) { StrokeIcon(Icons.close, tint = Fob.qrDark, size = 20.dp) }
        }
        BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
            // As wide as the screen allows, leaving room for the words; the quiet zone is the code's own margin.
            val side: Dp = minOf(maxWidth - 16.dp, maxHeight - if (nfc == Nfc.NONE) 150.dp else 230.dp).coerceAtLeast(160.dp)
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
                // Hidden on a phone without NFC; while it's off, the strip opens NFC's settings.
                if (nfc != Nfc.NONE) {
                    TouchStrip(nfc) {
                        if (nfc == Nfc.ON) onTouch()
                        else runCatching { context.startActivity(Intent(android.provider.Settings.ACTION_NFC_SETTINGS)) }
                    }
                }
                Text(
                    "A new address for this share, never used again.", color = Fob.qrMuted, fontSize = 13.sp, lineHeight = 19.sp,
                    textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 32.dp),
                )
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
    startAsking: Boolean = false, openContact: (ULong) -> Unit = { nav.go(Screen.Contact(it)) },
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
                            Text(c.now, fontSize = 14.sp, color = Fob.muted, fontFamily = if (c.label == "Send address") FontFamily.Monospace else FontFamily.Default)
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                StrokeIcon(Icons.arrow, tint = Fob.orangeText, size = 14.dp)
                                Text(c.new, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, fontFamily = if (c.label == "Send address") FontFamily.Monospace else FontFamily.Default)
                            }
                        }
                    }
                }
            }
            Text(clash.ask, color = Fob.muted, fontSize = 13.5.sp, lineHeight = 19.sp, textAlign = TextAlign.Center)
            ErrorText(error)
            PrimaryButton(clash.update, {
                if (clash.changes.isEmpty()) { onClose(); openContact(clash.contact.id) }
                else open { it.updateContactFromProfile(clash.contact.id, text) }
            }, height = 52.dp)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlineButton(clash.keep, { asking = false }, Modifier.weight(1f), height = 52.dp)
                // Only for a name: a contact's address is never given to a second one (Rust).
                clash.addNew?.let { OutlineButton(it, { open { w -> w.addContactFromProfile(text, true) } }, Modifier.weight(1f), height = 52.dp) }
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
                PrimaryButton(found.add, { if (clash != null) asking = true else open { it.addContactFromProfile(text, false) } }, Modifier.weight(1f), height = 52.dp)
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
            Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Box(
                    Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button) { nav.back() }.semantics { contentDescription = "Close" },
                    contentAlignment = Alignment.Center,
                ) { StrokeIcon(Icons.close, tint = Fob.onCamera, size = 20.dp) }
                Text("Scan a profile", color = Fob.onCamera, fontSize = 20.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f).semantics { heading() })
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
