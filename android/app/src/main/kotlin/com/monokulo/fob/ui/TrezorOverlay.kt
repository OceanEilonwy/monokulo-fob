package com.monokulo.fob.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.TrezorAsk
import com.monokulo.fob.WalletModel
import uniffi.fob.TrezorPassphrase

/**
 * Over every screen while Fob talks to a Trezor: what the device asks of
 * the person. A press to confirm on the device; its PIN, typed here by the
 * positions of the digits on the scrambled keypad the device shows; a
 * passphrase, typed here or on the device; or just how far along a long
 * exchange is. Nothing while no device exchange is on.
 */
@Composable
fun TrezorOverlay(model: WalletModel) {
    val asks = model.trezorAsks
    val busy by asks.busy.collectAsState()
    val ask by asks.ask.collectAsState()
    if (!busy) return
    BottomSheet("Your Trezor", onDismiss = {}) {
        SheetHandle()
        Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            when (val a = ask) {
                null -> Waiting("Talking to the Trezor…", onCancel = asks::cancel)
                is TrezorAsk.Button -> Waiting(buttonWords(a.what), onCancel = asks::cancel)
                is TrezorAsk.Progress -> {
                    Text("Working with the Trezor…", fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                    ProgressBar(if (a.total == 0u) 0f else a.done.toFloat() / a.total.toFloat())
                    OutlineButton("Cancel", asks::cancel, Modifier.fillMaxWidth())
                }
                TrezorAsk.Pin -> PinPad(onDone = asks::answerPin)
                is TrezorAsk.Passphrase -> PassphraseEntry(a.onDevice, onDone = asks::answerPassphrase)
            }
        }
    }
}

/** What to say for the device's screens, by the names its firmware gives them. */
private fun buttonWords(what: String): String = when {
    what.contains("pin", ignoreCase = true) -> "Unlock the Trezor: enter its PIN on the device."
    what.contains("address", ignoreCase = true) -> "The Trezor shows this wallet's address. Fob shows the same one on Receive: if they ever differ, don't use this wallet."
    what.contains("watch", ignoreCase = true) || what.contains("export", ignoreCase = true) -> "On the Trezor, allow Fob to watch this wallet: it asks to export the view key."
    what.contains("sync", ignoreCase = true) || what.contains("key_image", ignoreCase = true) -> "On the Trezor, allow the key image sync: it shows which of this wallet's coins are spent."
    what.contains("tx", ignoreCase = true) || what.contains("send", ignoreCase = true) || what.contains("output", ignoreCase = true) || what.contains("fee", ignoreCase = true) -> "Check the payment on the Trezor and confirm it there."
    else -> "Confirm on the Trezor."
}

@Composable
private fun Waiting(text: String, onCancel: () -> Unit) {
    Text(text, fontSize = 17.sp, lineHeight = 24.sp)
    WaitingBar(120.dp, "Waiting for the Trezor")
    OutlineButton("Cancel", onCancel, Modifier.fillMaxWidth())
}

/**
 * The device shows its digits scrambled on a 3 × 3 grid; the phone shows
 * the grid blank, laid out as the device's (7 8 9 above, 1 2 3 below), and
 * sends the positions pressed.
 */
@Composable
private fun PinPad(onDone: (String?) -> Unit) {
    var typed by remember { mutableStateOf("") }
    Text("Enter the PIN", fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
    Text("The Trezor shows where each digit is. Tap the same places here.", color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp)
    Text(if (typed.isEmpty()) " " else "•".repeat(typed.length), fontSize = 28.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().semantics { contentDescription = "${typed.length} digits typed" })
    for (row in listOf(listOf(7, 8, 9), listOf(4, 5, 6), listOf(1, 2, 3))) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally)) {
            for (position in row) {
                Box(
                    Modifier.size(72.dp).clip(RoundedCornerShape(18.dp)).background(Fob.sunken)
                        .clickable(enabled = typed.length < 50, role = Role.Button) { typed += position }
                        .semantics { contentDescription = "Keypad position $position" },
                    contentAlignment = Alignment.Center,
                ) { Box(Modifier.size(10.dp).clip(CircleShape).background(Fob.muted)) }
            }
        }
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlineButton("Cancel", { typed = ""; onDone(null) }, Modifier.weight(1f))
        OutlineButton("Delete", { typed = typed.dropLast(1) }, Modifier.weight(1f))
        PrimaryButton("Enter", { val pin = typed; typed = ""; onDone(pin) }, Modifier.weight(1f), enabled = typed.isNotEmpty())
    }
}

@Composable
private fun PassphraseEntry(onDevice: Boolean, onDone: (TrezorPassphrase) -> Unit) {
    var text by remember { mutableStateOf("") }
    Text("Passphrase", fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
    Text("This Trezor has hidden wallets. Each passphrase is a wallet of its own; leave it empty for the standard one.", color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp)
    OutlinedTextField(
        text, { text = it }, Modifier.fillMaxWidth(), label = { Text("Passphrase") }, singleLine = true,
        visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
    )
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlineButton("Cancel", { text = ""; onDone(TrezorPassphrase.Cancelled) }, Modifier.weight(1f))
        PrimaryButton("Use", { val typed = text; text = ""; onDone(TrezorPassphrase.Typed(typed)) }, Modifier.weight(1f))
    }
    if (onDevice) {
        Box(Modifier.height(4.dp))
        OutlineButton("Type it on the Trezor instead", { onDone(TrezorPassphrase.OnDevice) }, Modifier.fillMaxWidth())
    }
}
