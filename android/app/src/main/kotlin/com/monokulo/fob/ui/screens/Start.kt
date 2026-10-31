package com.monokulo.fob.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.text.KeyboardOptions
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.BarIcons
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.FobMark
import com.monokulo.fob.ui.IconBadge
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.InfoRow
import com.monokulo.fob.ui.NoScreenshots
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrScanner
import com.monokulo.fob.ui.Spacing
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.Warning
import com.monokulo.fob.ui.display
import com.monokulo.fob.ui.reason
import com.monokulo.fob.ui.short
import kotlinx.coroutines.launch
import uniffi.fob.CodeKind
import uniffi.fob.Network
import uniffi.fob.classifyCode
import uniffi.fob.previewWalletCode
import java.text.DateFormat
import java.util.Date

@Composable
fun fieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = Fob.ink, unfocusedBorderColor = Fob.outline, focusedContainerColor = Fob.card, unfocusedContainerColor = Fob.card,
    cursorColor = Fob.ink, focusedLabelColor = Fob.muted, unfocusedLabelColor = Fob.muted,
)

@Composable
fun Field(
    value: String, onChange: (String) -> Unit, label: String, mono: Boolean = false, lines: Int = 1, hint: String? = null,
    placeholder: String? = null, prose: Boolean = false,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        OutlinedTextField(
            value = value, onValueChange = onChange, label = { Text(label) }, minLines = lines, singleLine = lines == 1,
            placeholder = placeholder?.let { { Text(it, color = Fob.muted) } },
            shape = RoundedCornerShape(16.dp), colors = fieldColors(), modifier = Modifier.fillMaxWidth(),
            textStyle = androidx.compose.ui.text.TextStyle(fontSize = 16.sp, fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default),
            keyboardOptions = if (prose) KeyboardOptions(capitalization = KeyboardCapitalization.Sentences) else KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
        )
        if (hint != null) Text(hint, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 4.dp))
    }
}

/** "On stagenet, with test coins · Use mainnet": which network a new wallet is made on. */
@Composable
fun NetworkLine(model: WalletModel) {
    val network by model.network.collectAsState()
    val other = if (network == Network.STAGENET) Network.MAINNET else Network.STAGENET
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
        Text(if (network == Network.STAGENET) "On stagenet, with test coins" else "On mainnet, with real money", color = Fob.muted, fontSize = 13.sp)
        TextButton(if (other == Network.MAINNET) "Use mainnet" else "Use stagenet", { model.switchNetwork(other) })
    }
}

@Composable
fun WelcomeScreen(model: WalletModel, nav: Nav) {
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val wallet by model.wallet.collectAsState()
    LaunchedEffect(wallet) { if (wallet != null && !busy) nav.reset(Screen.Home) }
    Page(bottom = {
        ErrorText(error)
        PrimaryButton("Create a wallet", enabled = !busy, onClick = {
            busy = true
            scope.launch {
                runCatching { model.create() }
                    .onSuccess { nav.reset(Screen.Home); nav.go(Screen.Backup) }
                    .onFailure { error = it.reason() }
                busy = false
            }
        })
        OutlineButton("Restore a wallet", { nav.go(Screen.RestoreChoose) }, Modifier.fillMaxWidth())
        NetworkLine(model)
    }) {
        Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 72.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(18.dp)) {
            FobMark(112.dp)
            Text("Fob", style = display(36))
            Text("A Monero wallet for everyday paying. Quick to catch up, and it keeps your receipts.", color = Fob.muted, fontSize = 17.sp, lineHeight = 24.sp, textAlign = TextAlign.Center)
        }
        Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 40.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
            Point("M12 5a8 8 0 1 0 0 16a8 8 0 1 0 0-16 M12 9v4l2.5 2.5 M10 2.5h4", "Ready in seconds", "Use it while it catches up. Money in shows straight away.")
            Point(Icons.receipt, "Keeps your receipts", "Shops that include one in the payment code send it with the bill.")
            Point("M7 10.5h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2Z M8 10.5V7.5a4 4 0 0 1 8 0v3", "Yours alone", "Talks only to the node you pick. Receipts stay on this phone.")
        }
    }
}

@Composable
private fun Point(path: String, title: String, text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        StrokeIcon(path, tint = Fob.orangeText, size = 28.dp, width = 1.8f)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
            Text(text, color = Fob.muted, fontSize = 15.sp)
        }
    }
}

/** The seed phrase: after creating a wallet, or from Settings. */
@Composable
fun BackupScreen(model: WalletModel, nav: Nav, afterCreate: Boolean) {
    NoScreenshots()
    val wallet by model.wallet.collectAsState()
    val seed = remember(wallet) { wallet?.seedPhrase() }
    val scope = rememberCoroutineScope()
    // Home's reminder brings you here until the words are said to be written down.
    var pending by remember { mutableStateOf(false) }
    LaunchedEffect(wallet) { pending = wallet?.backupReminder() != null }
    fun written(then: () -> Unit) {
        scope.launch { runCatching { wallet?.backupDone() }; then() }
    }
    Page(bottom = {
        // A new wallet's last step is its profile (optional, with Skip).
        when {
            afterCreate -> PrimaryButton("I've written them down", { written { nav.replace(Screen.ProfileEdit(NEW_WALLET_STEP)) } })
            pending -> PrimaryButton("I've written them down", { written { nav.back() } })
            else -> PrimaryButton("Done", { nav.back() })
        }
    }) {
        TopBar(if (afterCreate) "Back up your wallet" else "Seed phrase", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (seed == null) {
                Text("This wallet only watches, so it has no seed phrase.", color = Fob.muted, fontSize = 16.sp, lineHeight = 22.sp)
            } else {
                Warning("These ${seed.words.size} words are your wallet. Write them down in order and keep them somewhere safe: anyone with them can spend your money, and without them it can't be restored.")
                Card(padding = androidx.compose.foundation.layout.PaddingValues(16.dp)) { SeedWords(seed.words) }
            }
        }
    }
}

/**
 * Seed words in two columns, numbered down each one (1–8 then 9–16; 13 and
 * 12 for a 25-word seed), never wrapping a word. At a very large font size,
 * one column.
 */
@Composable
fun SeedWords(words: List<String>) {
    val columns = if (LocalDensity.current.fontScale >= 1.6f) 1 else 2
    val perColumn = (words.size + columns - 1) / columns
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        words.withIndex().chunked(perColumn).forEach { column ->
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                column.forEach { (i, word) ->
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Fob.sunken).padding(horizontal = 10.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Text("${i + 1}", color = Fob.muted, fontSize = 13.sp, textAlign = TextAlign.End, maxLines = 1, softWrap = false, modifier = Modifier.widthIn(min = 20.dp))
                        Text(word, fontSize = 16.sp, fontWeight = FontWeight.Medium, maxLines = 1, softWrap = false)
                    }
                }
            }
        }
    }
}

@Composable
fun RestoreChooseScreen(nav: Nav) {
    Page {
        TopBar(null, onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(24.dp)) {
            Column(Modifier.padding(horizontal = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Restore a wallet", style = display(30))
                Text("Bring a wallet you already have. Scanning its code is quickest.", color = Fob.muted, fontSize = 16.sp, lineHeight = 22.sp)
            }
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Way(Icons.scan, "Scan a wallet code", "From Fob, the Monero GUI or Cake Wallet on another device", quickest = true) { nav.go(Screen.RestoreScan) }
                Way(Icons.lines, "Type your seed phrase", "16 words (Polyseed) or 25 words") { nav.go(Screen.RestoreSeed) }
                Way(Icons.eye, "View-only wallet", "Address and private view key. Sees payments, can't spend.") { nav.go(Screen.RestoreViewOnly) }
            }
        }
    }
}

@Composable
private fun Way(path: String, title: String, text: String, quickest: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(Fob.card).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 20.dp, vertical = 18.dp),
        horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(48.dp).clip(CircleShape).background(if (quickest) Fob.orange else Fob.sunken), contentAlignment = Alignment.Center) {
            StrokeIcon(path, tint = if (quickest) Fob.onOrange else Fob.ink, size = 24.dp)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(title, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                if (quickest) Text("Quickest", color = Fob.orangeText, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.clip(RoundedCornerShape(10.dp)).background(Fob.highlight).padding(horizontal = 8.dp, vertical = 2.dp))
            }
            Text(text, color = Fob.muted, fontSize = 14.sp, lineHeight = 19.sp)
        }
    }
}

/** A new wallet: Create, the seed phrase, then the profile. */
private const val NEW_WALLET_STEP = "Step 3 of 3 · optional"
/** Restoring takes two to four screens, depending on the way chosen. */
private const val RESTORED_STEP = "Last step · optional"

private const val START_HINT = "A date (YYYY-MM-DD) or block height from before the wallet's first payment. Earlier is safer; later is quicker."

@Composable
private fun Restoring(model: WalletModel, nav: Nav, label: String, enabled: Boolean, action: suspend WalletModel.() -> Unit, content: @Composable () -> Unit) {
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    Page(bottom = {
        ErrorText(error)
        PrimaryButton(label, enabled = enabled && !busy, onClick = {
            busy = true
            error = null
            scope.launch {
                // A restored wallet is offered a profile too, as its last step.
                runCatching { model.action() }.onSuccess { nav.reset(Screen.Home); nav.go(Screen.ProfileEdit(RESTORED_STEP)) }.onFailure { error = it.reason() }
                busy = false
            }
        })
    }) { content() }
}

@Composable
fun RestoreSeedScreen(model: WalletModel, nav: Nav) {
    var words by remember { mutableStateOf("") }
    var start by remember { mutableStateOf("") }
    val count = words.trim().split(Regex("\\s+")).count { it.isNotEmpty() }
    Restoring(model, nav, "Restore wallet", count == 16 || count == 25, { restoreWords(words, start.ifBlank { null }) }) {
        TopBar("Type your seed phrase", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Field(words, { words = it }, "Seed phrase", lines = 4, hint = "$count of 16 or 25 words")
            Field(start, { start = it }, "Start from (optional)", hint = if (count == 16) "A 16-word seed knows when it was made; leave this empty." else START_HINT)
        }
    }
}

@Composable
fun RestoreViewOnlyScreen(model: WalletModel, nav: Nav) {
    var address by remember { mutableStateOf("") }
    var key by remember { mutableStateOf("") }
    var start by remember { mutableStateOf("") }
    Restoring(model, nav, "Watch this wallet", address.isNotBlank() && key.isNotBlank(), { restoreViewOnly(address.trim(), key.trim(), start.ifBlank { null }) }) {
        TopBar("View-only wallet", onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Field(address, { address = it }, "Primary address", mono = true, lines = 3)
            Field(key, { key = it }, "Private view key", mono = true, lines = 2)
            Field(start, { start = it }, "Start from (optional)", hint = START_HINT)
        }
    }
}

@Composable
fun RestoreScanScreen(model: WalletModel, nav: Nav) {
    val network by model.network.collectAsState()
    var notice by remember { mutableStateOf<String?>(null) }
    Box(Modifier.fillMaxSize().background(Fob.camera)) {
        CameraOrAsk(Modifier.fillMaxSize()) { text ->
            when (classifyCode(network, text)) {
                CodeKind.WALLET -> if (nav.current == Screen.RestoreScan) nav.go(Screen.RestoreFound(text))
                CodeKind.PAYMENT -> notice = "That's a payment code. Scan the wallet code from your other wallet's settings."
                CodeKind.UNKNOWN -> notice = "That isn't a wallet code."
            }
        }
        // Clear of the status bar, cutout and gesture bar, though the camera fills the screen.
        Column(Modifier.fillMaxSize().safeDrawingPadding()) {
            TopBar("Scan a wallet code", onBack = { nav.back() }, tint = Fob.onCamera)
            Spacer(Modifier.weight(1f))
            Column(Modifier.fillMaxWidth().padding(horizontal = 32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(notice ?: "Point at the wallet code", color = Fob.onCamera, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center)
                Text("In your other wallet it's under Settings, often called Export wallet or Show keys as QR.", color = Fob.cameraHint, fontSize = 15.sp, textAlign = TextAlign.Center)
            }
            Warning(
                "A wallet code can hold the keys to spend your money. Scan only your own, somewhere no one can see the screen.",
                Modifier.padding(16.dp), surface = Fob.cameraCard, ink = Fob.onCamera, mark = Fob.cameraWarning,
            )
            Spacing(16.dp)
        }
    }
}

@Composable
fun RestoreFoundScreen(model: WalletModel, nav: Nav, code: String) {
    val network by model.network.collectAsState()
    val preview = remember(code) { runCatching { previewWalletCode(network, code) } }
    var start by remember { mutableStateOf("") }
    Restoring(model, nav, "Restore wallet", preview.isSuccess, { restoreCode(code, start.ifBlank { null }) }) {
        TopBar(null, onBack = { nav.back() })
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            val p = preview.getOrNull()
            if (p == null) {
                Text("Can't restore from this code", style = display(24))
                ErrorText(preview.exceptionOrNull()?.reason())
                return@Column
            }
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(44.dp).clip(CircleShape).background(Fob.greenFill), contentAlignment = Alignment.Center) { StrokeIcon(Icons.check, tint = Fob.onGreen, size = 24.dp) }
                Column {
                    Text("Wallet code found", style = display(24))
                    Text(if (p.canSpend) "Full wallet · can spend" else "View-only · sees payments", color = Fob.muted, fontSize = 14.sp)
                }
            }
            Card {
                InfoRow("Address", short(p.address, 2), mono = true)
                Divider()
                InfoRow("Keys", if (p.canSpend) "Spend + view · hidden" else "View key only")
                Divider()
                InfoRow("Starts from", p.restoreTime?.let { DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(it.toLong() * 1000)) } ?: "Not in the code")
            }
            if (p.restoreHeight == null) Field(start, { start = it }, "Start from", hint = START_HINT)
        }
    }
}

/**
 * The camera, once allowed; until then, a button to allow it, centred in
 * what [askPadding] leaves (clear of what's drawn over the camera). The
 * light as for [QrScanner]. The status bar's icons are light meanwhile.
 */
@Composable
fun CameraOrAsk(
    modifier: Modifier, torch: Boolean = false, onHasTorch: (Boolean) -> Unit = {}, askPadding: PaddingValues = PaddingValues(),
    onText: (String) -> Unit,
) {
    val context = androidx.compose.ui.platform.LocalContext.current
    var granted by remember {
        mutableStateOf(androidx.core.content.ContextCompat.checkSelfPermission(context, android.Manifest.permission.CAMERA) == android.content.pm.PackageManager.PERMISSION_GRANTED)
    }
    val ask = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.RequestPermission()) { granted = it }
    LaunchedEffect(Unit) { if (!granted) ask.launch(android.Manifest.permission.CAMERA) }
    // Allowed in Android's settings meanwhile: the camera shows on coming back.
    androidx.lifecycle.compose.LifecycleResumeEffect(Unit) {
        if (!granted) granted = androidx.core.content.ContextCompat.checkSelfPermission(context, android.Manifest.permission.CAMERA) == android.content.pm.PackageManager.PERMISSION_GRANTED
        onPauseOrDispose {}
    }
    // Light status bar icons over the dark camera, whatever the theme.
    BarIcons(darkIcons = false, statusOnly = true)
    if (granted) {
        QrScanner(onText, modifier, torch, onHasTorch)
    } else {
        Box(modifier.padding(askPadding), contentAlignment = Alignment.Center) {
            OutlineButton("Allow the camera", { ask.launch(android.Manifest.permission.CAMERA) }, tint = Fob.onCamera)
        }
    }
}
