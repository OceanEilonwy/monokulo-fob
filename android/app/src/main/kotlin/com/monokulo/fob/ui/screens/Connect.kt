package com.monokulo.fob.ui.screens

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import androidx.compose.ui.unit.sp
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.FilledIcon
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.FobMark
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.PrimaryButton
import com.monokulo.fob.ui.QrImage
import com.monokulo.fob.ui.SettingRow
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.Warning
import uniffi.fob.ConnectShares
import uniffi.fob.ConnectWallet
import uniffi.fob.ConnectWords
import uniffi.fob.ConnectedApp
import uniffi.fob.Identicon
import uniffi.fob.QrCode
import uniffi.fob.connectCount
import uniffi.fob.connectMore
import uniffi.fob.connectNewAccount
import uniffi.fob.connectWriteDown

// Connect to Fob's screens (boards B1 to B3, C2 and C3; decision 422). Each takes what it shows and says what was
// pressed: ConnectActivity runs the flow, and the screenshot test draws each one from fixed data. Every word is
// fob-words' (ConnectWords); nothing here is Till's own, the asking app's name and device coming from its request.

/** Choose a wallet's key for its first row, "New wallet". */
const val NEW_WALLET = "\u0000new"

/** The verified mark beside a known app's name: a green rosette with a tick. */
private const val ROSETTE = "M12 1.8l2.4 1.9 3-.3 1 2.9 2.7 1.5-.6 3 1.6 2.6-2.1 2.2.2 3-2.9.9-1.5 2.7-3-.6L12 22.2l-2.4-1.9-3 .6-1.5-2.7-2.9-.9.2-3L.3 12.1l1.6-2.6-.6-3 2.7-1.5 1-2.9 3 .3Z"
private const val ROSETTE_TICK = "M7.6 12.3l3 3 5.8-6"
/** A phone, before the asking device's name. */
private const val PHONE = "M9 2.5h6a2.5 2.5 0 0 1 2.5 2.5v14a2.5 2.5 0 0 1-2.5 2.5H9A2.5 2.5 0 0 1 6.5 19V5A2.5 2.5 0 0 1 9 2.5Z M10.5 18.5h3"
/** A padlock: the app can't spend. */
private const val LOCK = "M7 10.5h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2Z M8 10.5V7.5a4 4 0 0 1 8 0v3"

/** The connect screens' top bar: Back (or Close), "Connect Till", and Fob's mark at the end when [mark]. */
@Composable
private fun ConnectBar(title: String, onBack: () -> Unit, close: Boolean = false, mark: Boolean = false) {
    TopBar(title, onBack = onBack, closeIcon = close) {
        if (mark) {
            FobMark(28.dp)
            Spacer(Modifier.width(12.dp))
        }
    }
}

@Composable
private fun Heading(text: String, size: Int = 24) {
    Text(text, fontSize = size.sp, lineHeight = (size + 6).sp, fontWeight = FontWeight.Medium, modifier = Modifier.semantics { heading() })
}

@Composable
private fun Lead(text: String) {
    Text(text, color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp)
}

/** The screens' column: 20dp in from the sides, 14dp between its parts, as the boards. */
@Composable
private fun Body(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) { content() }
}

/** "Not now", "They don't match": the quiet way out under the main button. */
@Composable
private fun QuietButton(text: String, onClick: () -> Unit) {
    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { TextButton(text, onClick) }
}

/** The asking app's icon: Android's for an app on this phone, else its name's first letter on a tile. */
@Composable
fun AppIcon(app: String, icon: ImageBitmap?, size: Dp = 40.dp) {
    val shape = RoundedCornerShape(size / 4)
    if (icon != null) {
        Image(icon, contentDescription = null, modifier = Modifier.size(size).clip(shape))
    } else {
        Box(Modifier.size(size).clip(shape).background(Fob.orange), contentAlignment = Alignment.Center) {
            Text(app.take(1).uppercase(), color = Fob.onOrange, fontSize = (size.value * 0.45f).sp, fontWeight = FontWeight.Bold)
        }
    }
}

@Composable
private fun Verified() {
    Box(Modifier.size(18.dp).semantics { contentDescription = "Verified" }, contentAlignment = Alignment.Center) {
        FilledIcon(ROSETTE, tint = Fob.green, size = 18.dp)
        StrokeIcon(ROSETTE_TICK, tint = Color.White, size = 18.dp, width = 2.2f)
    }
}

/**
 * Who's asking: the app's icon and name, the verified mark when Android said which app called and it's one Fob
 * knows, and under it [line] (the device, after a phone when [phone]).
 */
@Composable
fun RequesterCard(app: String, icon: ImageBitmap?, verified: Boolean, line: String, phone: Boolean, initialOf: String = app) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card).padding(horizontal = 14.dp, vertical = 12.dp).semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        AppIcon(initialOf, icon)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(app, fontSize = 16.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                if (verified) Verified()
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (phone) StrokeIcon(PHONE, tint = Fob.muted, size = 15.dp)
                Text(line, color = Fob.muted, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/**
 * The request's pattern (board VkIdenticon): 5 × 5 cells from fob-codes, ink on paper, the same pixels on both
 * devices, on a white tile in both themes.
 */
@Composable
fun IdenticonImage(icon: Identicon, size: Dp, modifier: Modifier = Modifier) {
    val ink = parseColour(icon.ink)
    val paper = parseColour(icon.paper)
    val pad = size * 0.1f
    val gap = max(2.dp, size / 40)
    Column(
        modifier.size(size).clip(RoundedCornerShape(size * 0.12f)).background(paper).border(1.dp, Fob.line, RoundedCornerShape(size * 0.12f))
            .padding(pad).semantics { contentDescription = "Connection pattern" },
        verticalArrangement = Arrangement.spacedBy(gap),
    ) {
        for (r in 0 until 5) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(gap)) {
                for (c in 0 until 5) {
                    val on = icon.cells.getOrElse(r * 5 + c) { false }
                    Box(Modifier.weight(1f).aspectRatio(1f).clip(RoundedCornerShape(gap)).background(if (on) ink else Color.Transparent))
                }
            }
        }
    }
}

private fun parseColour(hex: String): Color =
    runCatching { Color(android.graphics.Color.parseColor(hex)) }.getOrDefault(Color.Black)

/** A wallet on Choose a wallet: its key ([NEW_WALLET] for the first row), name, apps and the account it would make. */
data class PickRow(val key: String, val name: String, val apps: List<String>, val nextAccount: UInt) {
    companion object {
        fun of(wallet: ConnectWallet) = PickRow(wallet.file, wallet.name, wallet.apps, wallet.nextAccount)
    }
}

/**
 * B1, Choose a wallet: who's asking, then "New wallet" and every wallet on the request's network, one line each
 * with "N connected" under it. Nothing is chosen until the person picks; the chosen row opens the account Fob would
 * make in it and the apps already connected. No balances.
 */
@Composable
fun ConnectPickScreen(
    words: ConnectWords, app: String, label: String, icon: ImageBitmap?, verified: Boolean, wallets: List<PickRow>, picked: String?,
    busy: Boolean, error: String?, onPick: (String) -> Unit, onContinue: () -> Unit, onNotNow: () -> Unit, onBack: () -> Unit,
) {
    Page(bottom = {
        ErrorText(error)
        PrimaryButton(words.goOn, onContinue, enabled = picked != null && !busy)
        QuietButton(words.notNow, onNotNow)
    }) {
        ConnectBar(words.title, onBack, mark = true)
        Body {
            RequesterCard(app, icon, verified, words.deviceLine, phone = true)
            Spacer(Modifier.size(0.dp))
            Heading(words.choose)
            val rows = listOf(PickRow(NEW_WALLET, words.newWallet, emptyList(), 1u)) + wallets
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card).selectableGroup()) {
                rows.forEachIndexed { i, row ->
                    if (i > 0) Divider()
                    PickLine(words, row, label, chosen = row.key == picked, first = i == 0, last = i == rows.lastIndex) { onPick(row.key) }
                }
            }
        }
    }
}

@Composable
private fun PickLine(words: ConnectWords, row: PickRow, label: String, chosen: Boolean, first: Boolean, last: Boolean, onClick: () -> Unit) {
    val corner = 20.dp
    val shape = RoundedCornerShape(topStart = if (first) corner else 0.dp, topEnd = if (first) corner else 0.dp, bottomStart = if (last) corner else 0.dp, bottomEnd = if (last) corner else 0.dp)
    Column(
        Modifier.fillMaxWidth()
            .then(if (chosen) Modifier.background(Fob.selectedFill, shape).border(2.dp, Fob.orangeText, shape) else Modifier)
            .selectable(selected = chosen, role = Role.RadioButton, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(Modifier.heightIn(min = 36.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            Radio(chosen)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(row.name, fontSize = 16.sp, fontWeight = if (chosen) FontWeight.Medium else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
                connectCount(row.apps.size.toUInt())?.let { Text(it, color = Fob.muted, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            }
        }
        if (chosen) {
            Column(Modifier.padding(start = 34.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(words.newAccount, color = Fob.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                Text(connectNewAccount(row.nextAccount, label), fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (row.apps.isNotEmpty()) {
                    val shown = words.appsShown.toInt()
                    Column(Modifier.padding(top = 6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(words.connectedApps, color = Fob.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                        row.apps.take(shown).forEach { Text(it, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                        if (row.apps.size > shown) Text(connectMore((row.apps.size - shown).toUInt()), color = Fob.muted, fontSize = 14.sp)
                    }
                }
            }
        }
    }
}

/** Fob's own radio, ringed in Fob's orange text colour when chosen, as the boards' rows. */
@Composable
private fun Radio(chosen: Boolean) {
    val ring = if (chosen) Fob.orangeText else Fob.outline
    Box(Modifier.size(20.dp).border(2.dp, ring, androidx.compose.foundation.shape.CircleShape), contentAlignment = Alignment.Center) {
        if (chosen) Box(Modifier.size(10.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Fob.orangeText))
    }
}

/**
 * B2, a new wallet for the app: Fob's own create flow on one screen. The name comes from the request; Show the words
 * makes the wallet and shows its seed (the words hidden until then), and Make the wallet waits until they're said
 * to be written down.
 */
@Composable
fun ConnectNewScreen(
    words: ConnectWords, name: String, onName: (String) -> Unit, seed: List<String>?, written: Boolean, onWritten: (Boolean) -> Unit,
    busy: Boolean, error: String?, onShow: () -> Unit, onMake: () -> Unit, onBack: () -> Unit, wordCount: Int = 16,
) {
    Page(bottom = {
        ErrorText(error)
        PrimaryButton(words.makeWallet, onMake, enabled = seed != null && written && !busy)
    }) {
        ConnectBar(words.newWalletTitle, onBack)
        Body {
            Field(name, onName, words.walletName)
            Spacer(Modifier.size(0.dp))
            Heading(connectWriteDown((seed?.size ?: wordCount).toUInt()), size = 22)
            Lead(words.whyWrite)
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card).padding(16.dp)) {
                if (seed != null) SeedWords(seed) else HiddenWords(wordCount)
            }
            if (seed == null) {
                Box(Modifier.padding(start = 0.dp)) { TextButton(words.showWords, { if (!busy) onShow() }) }
            }
            WrittenBox(words.written, written, enabled = seed != null, onWritten)
        }
    }
}

/** The words' places before they're shown: their numbers, and dots where each word goes. */
@Composable
private fun HiddenWords(count: Int) {
    val perColumn = (count + 1) / 2
    Row(horizontalArrangement = Arrangement.spacedBy(18.dp), modifier = Modifier.semantics(mergeDescendants = true) { contentDescription = "$count words, hidden" }) {
        (0 until count).chunked(perColumn).forEach { column ->
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                column.forEach { i ->
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text("${i + 1}", color = Fob.muted, fontSize = 15.sp, textAlign = androidx.compose.ui.text.style.TextAlign.End, modifier = Modifier.width(20.dp))
                        Text("••••••", color = Fob.outline, fontSize = 15.sp, letterSpacing = 2.sp)
                    }
                }
            }
        }
    }
}

/** Fob's checkbox row, as Node's "Accept self-signed certificates". */
@Composable
private fun WrittenBox(text: String, checked: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp))
            .toggleable(value = checked, enabled = enabled, role = Role.Checkbox, onValueChange = onChange),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val box = RoundedCornerShape(6.dp)
        val edge = when {
            checked -> Fob.ink
            enabled -> Fob.outline
            else -> Fob.track
        }
        Box(
            Modifier.size(22.dp).clip(box).background(if (checked) Fob.ink else Color.Transparent).border(2.dp, edge, box),
            contentAlignment = Alignment.Center,
        ) {
            if (checked) StrokeIcon(Icons.check, tint = Fob.paper, size = 14.dp, width = 3.4f)
        }
        Text(text, fontSize = 16.sp, color = if (enabled) Fob.ink else Fob.muted)
    }
}

/**
 * B3, the consent screen: the question, the account's name (from the request, editable), what the app can and
 * can't do, the wallet-wide warning for a wallet that looks personal, and the switches for what else it asked for.
 * On a reconnect, its account can be given again.
 */
@Composable
fun ConnectConsentScreen(
    words: ConnectWords, wallet: String, label: String, onLabel: (String) -> Unit, personal: Boolean,
    before: ConnectedApp?, useAgain: Boolean, onUseAgain: (Boolean) -> Unit,
    shares: ConnectShares, wantsNode: Boolean, wantsPrices: Boolean,
    shareNode: Boolean, onShareNode: (Boolean) -> Unit, sharePrices: Boolean, onSharePrices: (Boolean) -> Unit,
    busy: Boolean, error: String?, onGive: () -> Unit, onNotNow: () -> Unit, onBack: () -> Unit,
) {
    Page(bottom = {
        ErrorText(error)
        PrimaryButton(words.give, onGive, enabled = label.isNotBlank() && !busy)
        QuietButton(words.notNow, onNotNow)
    }) {
        ConnectBar(words.title, onBack)
        Body {
            Heading(words.giveAccess, size = 26)
            Text(
                buildAnnotatedString {
                    append(words.getsBefore)
                    withStyle(SpanStyle(color = Fob.ink, fontWeight = FontWeight.Medium)) { append(wallet) }
                    append(words.getsAfter)
                },
                color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp,
            )
            Field(label, onLabel, words.accountName)
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card)) {
                AccessRow(Icons.eye, words.canSee)
                Divider()
                AccessRow(LOCK, words.cantSpend)
            }
            if (personal) Warning(words.walletWide)
            val node = shares.node?.takeIf { wantsNode }
            val prices = shares.prices?.takeIf { wantsPrices }
            if (before != null || node != null || prices != null) {
                Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.card)) {
                    var rows = 0
                    if (before != null) {
                        SettingRow(words.useAgain, connectNewAccount(before.account, before.label ?: label), checked = useAgain, onChecked = onUseAgain)
                        rows++
                    }
                    if (node != null) {
                        if (rows++ > 0) Divider()
                        SettingRow(words.shareNode, node, checked = shareNode, onChecked = onShareNode)
                    }
                    if (prices != null) {
                        if (rows > 0) Divider()
                        SettingRow(words.sharePrices, prices, checked = sharePrices, onChecked = onSharePrices)
                    }
                }
            }
        }
    }
}

@Composable
private fun AccessRow(icon: String, text: String) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        StrokeIcon(icon, size = 22.dp, width = 1.8f)
        Text(text, fontSize = 15.sp, lineHeight = 21.sp)
    }
}

/**
 * C2, after Fob's scanner read a request: who it's from, and the request's pattern to hold against the other
 * device's. A swapped code shows another pattern.
 */
@Composable
fun ConnectScanScreen(words: ConnectWords, app: String, icon: ImageBitmap?, identicon: Identicon, onMatch: () -> Unit, onNoMatch: () -> Unit, onClose: () -> Unit) {
    Page(bottom = {
        PrimaryButton(words.theyMatch, onMatch)
        QuietButton(words.noMatch, onNoMatch)
    }) {
        ConnectBar(words.title, onClose, close = true, mark = true)
        Body {
            RequesterCard(words.fromDevice, icon, verified = false, line = words.fromCode, phone = false, initialOf = app)
            Spacer(Modifier.size(0.dp))
            Heading(words.samePattern)
            Box(Modifier.fillMaxWidth().padding(top = 6.dp), contentAlignment = Alignment.Center) { IdenticonImage(identicon, 200.dp) }
            Lead(words.different)
        }
    }
}

/** C3, the sealed answer as a code for the other device, with the small pattern of the request it answers. */
@Composable
fun ConnectAnswerScreen(words: ConnectWords, qr: QrCode, identicon: Identicon, onDone: () -> Unit, onBack: (() -> Unit)?) {
    Page(bottom = { PrimaryButton(words.done, onDone) }) {
        ConnectBar(words.title, onBack ?: onDone)
        Body {
            Heading(words.showThis, size = 26)
            Lead(words.showHow)
            Box(Modifier.fillMaxWidth().padding(top = 10.dp), contentAlignment = Alignment.Center) {
                Box(Modifier.clip(RoundedCornerShape(16.dp)).background(Fob.qrLight).padding(18.dp)) {
                    QrImage(qr, 300.dp, description = words.showThis)
                }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
                IdenticonImage(identicon, 48.dp)
                Text(words.forPattern, color = Fob.muted, fontSize = 15.sp)
            }
        }
    }
}
