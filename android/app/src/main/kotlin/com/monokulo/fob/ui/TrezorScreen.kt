package com.monokulo.fob.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.TrezorAsk
import com.monokulo.fob.WalletModel
import uniffi.fob.TrezorCard
import uniffi.fob.TrezorPassphrase
import uniffi.fob.TrezorStep
import uniffi.fob.trezorCardButton
import uniffi.fob.trezorCardLooking
import uniffi.fob.trezorCardPassphrase
import uniffi.fob.trezorCardPin
import uniffi.fob.trezorCardProgress
import uniffi.fob.trezorStepFor
import uniffi.fob.trezorSteps
import uniffi.fob.trezorWords

/**
 * The whole screen while Fob talks to a Trezor: a card that mirrors the
 * device and says what to do on it, the steps of connecting counted off at
 * its corner, the one question of Fob's own (passphrase or not) asked
 * before the device would, and a problem said with Try again. The X in the
 * top bar gives the device up. Every word is Rust's, shared with the desktop.
 */
@Composable
fun TrezorScreen(model: WalletModel) {
    val asks = model.trezorAsks
    val busy by asks.busy.collectAsState()
    val ask by asks.ask.collectAsState()
    val hello by asks.hello.collectAsState()
    val address by asks.address.collectAsState()
    if (!busy) return
    val words = remember { trezorWords() }
    var typed by remember { mutableStateOf(false) }
    val a = ask
    // A fresh question closes the field, so a later passphrase ask begins at the question again.
    if (a !is TrezorAsk.Passphrase) typed = false

    Column(Modifier.fillMaxSize().background(Fob.paper).safeDrawingPadding()) {
        TopBar(words.yourTrezor, onBack = asks::cancel, closeIcon = true)
        Column(Modifier.weight(1f).padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            val (card, step, screen, problem) = when (a) {
                null, TrezorAsk.Looking, is TrezorAsk.Hello -> Shown(trezorCardLooking(), null, Screen.Dark, null)
                is TrezorAsk.Button -> Shown(trezorCardButton(a.what), trezorStepFor(a.what), screenFor(a.what, address), null)
                TrezorAsk.Pin -> Shown(trezorCardPin(), TrezorStep.UNLOCK, Screen.Pin, null)
                is TrezorAsk.Passphrase -> Shown(trezorCardPassphrase(typed), TrezorStep.WALLET, Screen.Passphrase, null)
                is TrezorAsk.Progress -> Shown(trezorCardProgress(a.done, a.total), null, Screen.Syncing(if (a.total == 0u) 0f else a.done.toFloat() / a.total.toFloat()), null)
                is TrezorAsk.Problem -> Shown(a.problem.card, null, Screen.Dark, a.problem)
            }
            val steps = hello?.let { trezorSteps(it.pinProtection, it.passphraseProtection) } ?: emptyList()
            Card(padding = androidx.compose.foundation.layout.PaddingValues(horizontal = 16.dp, vertical = 20.dp)) {
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    // The header: the eyebrow, and the steps at the right.
                    Row(Modifier.fillMaxWidth().height(24.dp), verticalAlignment = Alignment.CenterVertically) {
                        val tone = if (problem == null) Fob.orangeText else Fob.red
                        Text(card.eyebrow, color = tone, fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 0.6.sp, modifier = Modifier.weight(1f))
                        if (step != null && steps.isNotEmpty()) {
                            val at = steps.indexOf(step)
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    steps.forEachIndexed { i, _ ->
                                        Box(Modifier.size(8.dp).clip(CircleShape).background(if (i < at) Fob.green else if (i == at) Fob.orange else Fob.line))
                                    }
                                }
                                Text("${at + 1} of ${steps.size}", color = Fob.muted, fontSize = 12.sp)
                            }
                        }
                    }
                    if (problem == null) DeviceArt(screen) else ProblemMark(problem.stop)
                    Text(card.title, fontSize = 18.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center, lineHeight = 24.sp)
                    if (a is TrezorAsk.Button && a.what.contains("address", ignoreCase = true)) {
                        address?.let { AddressBlock(it) }
                    }
                    Text(card.body, color = Fob.muted, fontSize = 14.sp, textAlign = TextAlign.Center, lineHeight = 20.sp)
                    when (a) {
                        null, TrezorAsk.Looking, is TrezorAsk.Hello -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            CircularProgressIndicator(Modifier.size(18.dp), color = Fob.orangeText, trackColor = Fob.line, strokeWidth = 2.5.dp)
                            Text(words.lookingHint, color = Fob.muted, fontSize = 13.sp)
                        }
                        is TrezorAsk.Progress -> {
                            ProgressBar(if (a.total == 0u) 0f else a.done.toFloat() / a.total.toFloat())
                            Text(words.keptIfCancelled, color = Fob.muted, fontSize = 12.sp)
                        }
                        is TrezorAsk.Passphrase -> if (typed) PassphraseField(onUse = { asks.answerPassphrase(TrezorPassphrase.Typed(it)) })
                        TrezorAsk.Pin -> PinPad(onDone = asks::answerPin)
                        else -> {}
                    }
                }
            }
            Spacer(Modifier.weight(1f))
            // The actions, at the foot of the screen as every Fob action is.
            when (a) {
                is TrezorAsk.Passphrase -> if (!typed) {
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        if (a.onDevice) OutlineButton(words.onTheTrezor, { asks.answerPassphrase(TrezorPassphrase.OnDevice) }, Modifier.fillMaxWidth())
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            OutlineButton(words.typeHere, { typed = true }, Modifier.weight(1f))
                            PrimaryButton(words.noPassphrase, { asks.answerPassphrase(TrezorPassphrase.Typed("")) }, Modifier.weight(1f))
                        }
                    }
                } else {
                    TextButton(words.enterOnTrezorInstead, { typed = false })
                }
                is TrezorAsk.Problem -> if (a.problem.retry) PrimaryButton(words.tryAgain, asks::retry, Modifier.fillMaxWidth())
                else -> {}
            }
            Spacing(24.dp)
        }
    }
}

private data class Shown(val card: TrezorCard, val step: TrezorStep?, val screen: Screen, val problem: uniffi.fob.TrezorProblem?)

/** What the drawn device shows on its screen. */
private sealed interface Screen {
    data object Dark : Screen
    data object Pin : Screen
    data object Passphrase : Screen
    data class Confirm(val lines: List<String>) : Screen
    /** The first page of the address, as the device shows it. */
    data class Address(val address: String) : Screen
    data class Syncing(val fraction: Float) : Screen
}

private fun screenFor(what: String, address: String?): Screen {
    val w = what.lowercase()
    return when {
        w.contains("pin") -> Screen.Pin
        w.contains("address") -> Screen.Address(address.orEmpty())
        w.contains("watch") || w.contains("export") -> Screen.Confirm(listOf("Export watch-only", "credentials?"))
        w.contains("key_image") || w.contains("sync") -> Screen.Confirm(listOf("Sync key images?"))
        else -> Screen.Confirm(listOf("Confirm"))
    }
}

/**
 * The Safe 3, as the device is drawn everywhere: a dark body, its screen,
 * two flat buttons under it, the wordmark at the foot. A dark object in
 * both themes.
 */
@Composable
private fun DeviceArt(screen: Screen) {
    val measurer = rememberTextMeasurer()
    val white = Color.White
    val grey = Color(0xFF9A9A9A)
    val green = Color(0xFF7FD49A)
    Canvas(Modifier.size(150.dp, 232.dp).semantics { contentDescription = "The Trezor" }) {
        val w = size.width
        val h = size.height
        drawRoundRect(Color(0xFF1A1917), Offset.Zero, size, CornerRadius(22.dp.toPx()))
        drawRoundRect(Color(0xFF5A564C), Offset.Zero, size, CornerRadius(22.dp.toPx()), style = Stroke(2.dp.toPx()))
        val sx = 15.dp.toPx(); val sy = 22.dp.toPx(); val sw = w - 30.dp.toPx(); val sh = 78.dp.toPx()
        drawRoundRect(Color.Black, Offset(sx, sy), Size(sw, sh), CornerRadius(5.dp.toPx()))
        drawRoundRect(Color(0xFF333333), Offset(sx, sy), Size(sw, sh), CornerRadius(5.dp.toPx()), style = Stroke(1.dp.toPx()))
        val by = sy + sh + 10.dp.toPx()
        for (x in listOf(sx, sx + sw - 50.dp.toPx())) {
            drawRoundRect(Color(0xFF333333), Offset(x, by), Size(50.dp.toPx(), 20.dp.toPx()), CornerRadius(4.dp.toPx()))
            drawRoundRect(Color(0xFF555555), Offset(x, by), Size(50.dp.toPx(), 20.dp.toPx()), CornerRadius(4.dp.toPx()), style = Stroke(1.dp.toPx()))
        }
        fun centred(text: String, y: Float, size: androidx.compose.ui.unit.TextUnit, color: Color, mono: Boolean = false) {
            val laid = measurer.measure(text, TextStyle(color = color, fontSize = size, fontWeight = FontWeight.Bold, fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default))
            drawText(laid, topLeft = Offset(sx + (sw - laid.size.width) / 2f, y))
        }
        val mark = measurer.measure("T R E Z O R", TextStyle(color = Color(0xFF8A8A8A), fontSize = 10.sp, fontWeight = FontWeight.Bold))
        drawText(mark, topLeft = Offset((w - mark.size.width) / 2f, h - 16.dp.toPx() - mark.size.height))
        when (screen) {
            Screen.Dark -> {}
            Screen.Pin -> {
                centred("Enter PIN", sy + 10.dp.toPx(), 11.sp, white)
                for (row in 0 until 2) for (column in 0 until 3) {
                    drawRoundRect(Color(0xFF777777), Offset(sx + 36.dp.toPx() + column * 16.dp.toPx(), sy + 32.dp.toPx() + row * 13.dp.toPx()), Size(12.dp.toPx(), 9.dp.toPx()), CornerRadius(1.dp.toPx()), style = Stroke(1.dp.toPx()))
                }
            }
            Screen.Passphrase -> {
                centred("Enter passphrase", sy + 10.dp.toPx(), 11.sp, white)
                drawRoundRect(Color(0xFF777777), Offset(sx + 15.dp.toPx(), sy + 34.dp.toPx()), Size(sw - 30.dp.toPx(), 12.dp.toPx()), CornerRadius(2.dp.toPx()), style = Stroke(1.dp.toPx()))
                centred("empty = standard", sy + 54.dp.toPx(), 8.sp, grey)
            }
            is Screen.Confirm -> {
                var y = sy + 12.dp.toPx()
                for (line in screen.lines) { centred(line, y, 10.sp, white); y += 14.dp.toPx() }
                centred("Confirm", sy + sh - 20.dp.toPx(), 10.sp, green)
            }
            is Screen.Address -> {
                centred("Address 1/3", sy + 6.dp.toPx(), 7.sp, grey)
                // The device's first page: its first characters, in its groups.
                val chunks = screen.address.chunked(10).take(3)
                chunks.forEachIndexed { i, chunk ->
                    val text = if (chunk.length > 5) chunk.substring(0, 5) + " " + chunk.substring(5) else chunk
                    centred(text, sy + (22 + 16 * i).dp.toPx(), 9.sp, white, mono = true)
                }
            }
            is Screen.Syncing -> {
                centred("Syncing…", sy + 14.dp.toPx(), 11.sp, white)
                val bx = sx + 15.dp.toPx(); val bw = sw - 30.dp.toPx(); val byy = sy + 44.dp.toPx()
                drawRoundRect(Color(0xFF333333), Offset(bx, byy), Size(bw, 6.dp.toPx()), CornerRadius(3.dp.toPx()))
                drawRoundRect(green, Offset(bx, byy), Size(bw * screen.fraction.coerceIn(0f, 1f), 6.dp.toPx()), CornerRadius(3.dp.toPx()))
            }
        }
    }
}

/** A problem's mark in place of the device: a warning, or a cross when the address differed. */
@Composable
private fun ProblemMark(stop: Boolean) {
    Box(Modifier.size(72.dp).clip(CircleShape).background(if (stop) Fob.red else Fob.warning), contentAlignment = Alignment.Center) {
        StrokeIcon(if (stop) Icons.close else Icons.warning, tint = if (stop) Fob.onRed else Fob.orangeText, size = 34.dp, width = 2.2f)
    }
}

/** The address as Fob holds it, in groups, the first, middle and last marked, for comparing with the device. */
@Composable
private fun AddressBlock(address: String) {
    val groups = uniffi.fob.addressGroups(address)
    Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(Fob.highlight).padding(horizontal = 14.dp, vertical = 12.dp)) {
        Text(
            androidx.compose.ui.text.buildAnnotatedString {
                groups.forEachIndexed { i, g ->
                    if (i > 0) append(" ")
                    if (g.highlighted) {
                        withStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.Bold, background = Fob.addressMark, color = Fob.onAddressMark)) { append(g.text) }
                    } else {
                        append(g.text)
                    }
                }
            },
            fontFamily = FontFamily.Monospace, fontSize = 14.sp, lineHeight = 22.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun PassphraseField(onUse: (String) -> Unit) {
    val words = remember { trezorWords() }
    var text by remember { mutableStateOf("") }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlinedTextField(
            text, { text = it }, Modifier.fillMaxWidth(), singleLine = true,
            label = { Text(words.passphraseField) },
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        )
        PrimaryButton(words.openWallet, { val typed = text; text = ""; onUse(typed) }, Modifier.fillMaxWidth())
    }
}

/** The device's keypad: 7 8 9 above, 1 2 3 below, digits hidden, for a device that wants the PIN typed here. */
@Composable
private fun PinPad(onDone: (String?) -> Unit) {
    val words = remember { trezorWords() }
    var typed by remember { mutableStateOf("") }
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(if (typed.isEmpty()) " " else "•".repeat(typed.length), fontSize = 24.sp, modifier = Modifier.semantics { contentDescription = "Digits typed" })
        for (row in listOf(listOf(7, 8, 9), listOf(4, 5, 6), listOf(1, 2, 3))) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (position in row) {
                    Box(
                        Modifier.size(72.dp, 56.dp).clip(RoundedCornerShape(12.dp)).background(Fob.sunken)
                            .clickable(enabled = typed.length < 50, role = Role.Button) { typed += position }
                            .semantics { contentDescription = "Keypad position $position" },
                        contentAlignment = Alignment.Center,
                    ) { Text("•", fontSize = 20.sp) }
                }
            }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlineButton(words.pinDelete, { typed = typed.dropLast(1) }, Modifier.weight(1f))
            PrimaryButton(words.pinEnter, { val pin = typed; typed = ""; onDone(pin) }, Modifier.weight(1f), enabled = typed.isNotEmpty())
        }
    }
}
