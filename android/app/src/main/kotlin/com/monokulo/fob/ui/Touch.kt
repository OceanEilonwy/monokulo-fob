package com.monokulo.fob.ui

import android.content.Intent
import android.view.HapticFeedbackConstants
import android.os.Build
import androidx.activity.compose.LocalActivity
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.monokulo.fob.Nfc
import com.monokulo.fob.PayRunner
import com.monokulo.fob.TouchPhones
import com.monokulo.fob.WalletModel
import com.monokulo.fob.nfcState
import com.monokulo.fob.preferFobService
import uniffi.fob.PayCard

/**
 * NFC on this phone as of now (looked at again each time the screen is
 * resumed, as it may have been turned on in Android's settings), and
 * whether Fob uses it for [meet] and for [pay] (Settings' two NFC switches).
 */
class TouchState(val nfc: Nfc, val meet: Boolean, val pay: Boolean) {
    /** Meet is listening: NFC on in the phone and Meet on in Settings. */
    val meetActive get() = nfc == Nfc.ON && meet
    /** Payments are listening (Send, Receive, Home): NFC on in the phone and Payments on in Settings. */
    val payActive get() = nfc == Nfc.ON && pay
}

@Composable
fun rememberTouchState(model: WalletModel): TouchState {
    val context = LocalContext.current
    val meet by model.meetNfc.collectAsState()
    val pay by model.payNfc.collectAsState()
    var nfc by remember { mutableStateOf(nfcState(context)) }
    LifecycleResumeEffect(Unit) {
        nfc = nfcState(context)
        onPauseOrDispose {}
    }
    return TouchState(nfc, meet, pay)
}

/** Android's NFC settings, where Turn on NFC and "NFC is off" lead. */
fun openNfcSettings(context: android.content.Context) {
    runCatching { context.startActivity(Intent(android.provider.Settings.ACTION_NFC_SETTINGS)) }
}

/** A short tick as a touch lands. */
@Composable
fun rememberTick(): () -> Unit {
    val view = LocalView.current
    return { view.performHapticFeedback(if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.LONG_PRESS) }
}

/**
 * Touch to pay, the reader (Send, Home): while the screen is resumed and
 * [enabled], a phone on Receive held to this one gives its payment code to
 * [onCode], with a tick; a touch that took nothing says why to [onProblem].
 */
@Composable
fun rememberPayReading(model: WalletModel, touch: TouchState, enabled: Boolean = true, onProblem: (String) -> Unit = {}, onCode: (String) -> Unit) {
    val activity = LocalActivity.current
    val network by model.network.collectAsState()
    val code by rememberUpdatedState(onCode)
    val problem by rememberUpdatedState(onProblem)
    val tick = rememberTick()
    val ticking by rememberUpdatedState(tick)
    if (activity != null && touch.payActive && enabled) {
        LifecycleResumeEffect(network) {
            val runner = PayRunner(activity, network, { text -> ticking(); code(text) }, { why -> problem(why) })
            runner.start()
            onPauseOrDispose { runner.stop() }
        }
    }
}

/**
 * Touch to pay, the card (Receive): while resumed, [code] (the payment code
 * Receive shows) is what a reader gets. Each time a reader takes it all,
 * [onRead] runs.
 */
@Composable
fun rememberPayCard(touch: TouchState, code: String?, onRead: () -> Unit = {}) {
    val activity = LocalActivity.current
    val card = remember(code) { code?.let { runCatching { PayCard(it) }.getOrNull() } }
    if (activity != null && touch.payActive && card != null) {
        LifecycleResumeEffect(card) {
            TouchPhones.pay = card
            preferFobService(activity, true)
            onPauseOrDispose {
                if (TouchPhones.pay === card) TouchPhones.pay = null
                preferFobService(activity, false)
            }
        }
    }
    val read by TouchPhones.payRead.collectAsState()
    val reading by rememberUpdatedState(onRead)
    LaunchedEffect(read) { if (read > 0) reading() }
}

/** The NFC mark on a [size] disc: orange waves on the page's sunken colour (or [fill]); [waves] breathe out round it. */
@Composable
fun NfcMark(size: Dp = 32.dp, fill: Color = Fob.sunken, tint: Color = Fob.orangeText, modifier: Modifier = Modifier, waves: Boolean = false, reach: Dp = 7.dp, onClick: (() -> Unit)? = null) {
    val phase = if (waves) rememberWavePhase() else 0f
    Box(
        modifier.size(size)
            .then(
                if (waves) Modifier.drawBehind {
                    for (i in 0 until 2) {
                        val t = (phase + i / 2f) % 1f
                        drawCircle(tint.copy(alpha = 0.5f * (1f - t)), radius = this.size.width / 2f + reach.toPx() * t, style = Stroke(1.6.dp.toPx()))
                    }
                } else Modifier,
            )
            .clip(CircleShape).background(fill)
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .semantics { contentDescription = "NFC is on" },
        contentAlignment = Alignment.Center,
    ) { StrokeIcon(Icons.nfc, tint = tint, size = size * 0.6f, width = 2f) }
}

/**
 * NFC on Receive: a rounded square [size] across beside the address badge on the code's top edge, white and ringed in the
 * page's colour like it. On, the NFC mark in grey on the white, as Send's over the camera, its three waves lighting up one after another; off (in
 * Android, or Touch phones in Settings), grey and struck through, and tapping it goes to turn it on. None where the
 * phone has no NFC.
 */
@Composable
fun NfcBadge(touch: TouchState, size: Dp, onTurnOn: () -> Unit, modifier: Modifier = Modifier) {
    if (touch.nfc == Nfc.NONE) return
    val on = touch.payActive
    val phase = if (on) rememberWavePhase(1800) else 0f
    Box(
        modifier.size(size).clip(RoundedCornerShape(16.dp)).background(Fob.paper).padding(3.dp).clip(RoundedCornerShape(13.dp)).background(Fob.qrLight)
            .then(if (on) Modifier else Modifier.clickable(role = Role.Button, onClick = onTurnOn))
            .semantics { contentDescription = if (on) "NFC is on" else "NFC is off. Turn it on" },
        contentAlignment = Alignment.Center,
    ) {
        val tint = if (on) Fob.qrMuted else Fob.muted
        Box(
            Modifier.size(40.dp).then(
                if (on) Modifier else Modifier.drawBehind {
                    drawLine(tint, Offset(this.size.width * 0.24f, this.size.height * 0.76f), Offset(this.size.width * 0.76f, this.size.height * 0.24f), strokeWidth = 3.2.dp.toPx(), cap = androidx.compose.ui.graphics.StrokeCap.Round)
                },
            ),
            Alignment.Center,
        ) {
            if (on) {
                // The three waves, each lit as the phase passes it, then dimmed again: the signal going out.
                Icons.nfcWaves.forEachIndexed { i, wave ->
                    val a = ((phase - i * 0.16f) % 1f + 1f) % 1f
                    val lit = if (a < 0.4f) kotlin.math.sin(Math.PI.toFloat() * a / 0.4f) else 0f
                    StrokeIcon(wave, tint = tint.copy(alpha = 0.3f + 0.7f * lit), size = 32.dp, width = 2.7f)
                }
            } else StrokeIcon(Icons.nfc, tint = tint, size = 32.dp, width = 2.7f)
        }
    }
}

/** A phase that runs 0 to 1 and round again, slowly: the waves' breathing. */
@Composable
fun rememberWavePhase(millis: Int = 2800): Float {
    val transition = rememberInfiniteTransition(label = "waves")
    val phase by transition.animateFloat(0f, 1f, infiniteRepeatable(tween(millis, easing = LinearEasing), RepeatMode.Restart), label = "wave")
    return phase
}
