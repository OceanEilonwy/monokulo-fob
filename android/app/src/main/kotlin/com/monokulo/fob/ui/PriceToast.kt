package com.monokulo.fob.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import uniffi.fob.PriceToast
import kotlin.math.abs
import kotlin.math.roundToInt

/** What the price toast shows: a failure (Retry and the cog), Retry asking, or the price it got (for 2 s). */
sealed interface PriceToastState {
    data class Failed(val toast: PriceToast) : PriceToastState
    data class Retrying(val toast: PriceToast) : PriceToastState
    data class Updated(val title: String, val line: String) : PriceToastState
}

/**
 * How tall the current screen's own bottom bar is (Review, Share), so the
 * price toast rises above it; none on a screen without one.
 */
val LocalBottomSpace = staticCompositionLocalOf<MutableState<Dp>> { mutableStateOf(0.dp) }

/**
 * Says this, a screen's bottom bar, takes the bottom of the screen, with
 * [extra] more below it; none again once it goes.
 */
@Composable
fun Modifier.bottomSpace(extra: Dp = 0.dp): Modifier {
    val space = LocalBottomSpace.current
    val density = LocalDensity.current
    DisposableEffect(space) { onDispose { space.value = 0.dp } }
    return onSizeChanged { space.value = with(density) { it.height.toDp() } + extra }
}

/**
 * The price toast, the same over every screen: it rises from the bottom,
 * above the gesture bar and the screen's bottom bar ([LocalBottomSpace]).
 * Swiped down or sideways it goes ([onDismiss]); [onSettings] is the cog.
 * A [notice] (Rust's words when a price choice moved something) sits
 * above it for a few seconds; tapped, it goes. While the keyboard is up
 * the toast steps aside, back when it goes: above the keyboard it would
 * cover the field being typed in, whose own line says how old the price is.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun PriceToastHost(
    state: PriceToastState?, onRetry: () -> Unit, onSettings: () -> Unit, onDismiss: () -> Unit,
    notice: String? = null, onNoticeDismiss: () -> Unit = {},
) {
    val space by LocalBottomSpace.current
    // The last state drawn, so the toast keeps its words as it goes.
    var shown by remember { mutableStateOf(state) }
    if (state != null) shown = state
    var said by remember { mutableStateOf(notice) }
    if (notice != null) said = notice
    val lift by animateDpAsState(if (space > 0.dp) space + 8.dp else 16.dp, label = "toast lift")
    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(bottom = lift), contentAlignment = Alignment.BottomCenter) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            AnimatedVisibility(notice != null, enter = slideInVertically { it } + fadeIn(), exit = fadeOut()) {
                said?.let { Notice(it, onNoticeDismiss) }
            }
            AnimatedVisibility(state != null && !WindowInsets.isImeVisible, enter = slideInVertically { it } + fadeIn(), exit = slideOutVertically { it } + fadeOut()) {
                shown?.let { ToastCard(it, onRetry, onSettings, onDismiss) }
            }
        }
    }
}

/** A short note in the toast's colours: words only, read by TalkBack as they come. */
@Composable
private fun Notice(text: String, onDismiss: () -> Unit) {
    val shape = RoundedCornerShape(16.dp)
    Box(
        Modifier.padding(horizontal = 16.dp).widthIn(max = 560.dp).fillMaxWidth()
            .shadow(8.dp, shape).clip(shape).background(Fob.inverse)
            .clickable(onClickLabel = "Dismiss", onClick = onDismiss)
            .semantics { liveRegion = LiveRegionMode.Polite }
            .heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 12.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(text, color = Fob.onInverse, fontSize = 14.sp)
    }
}

@Composable
private fun ToastCard(state: PriceToastState, onRetry: () -> Unit, onSettings: () -> Unit, onDismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    // Where a swipe has it; let go short of [far], it springs back.
    val dx = remember { Animatable(0f) }
    val dy = remember { Animatable(0f) }
    val far = with(LocalDensity.current) { 96.dp.toPx() }
    val settle = {
        if (abs(dx.value) > far || dy.value > far / 2) onDismiss()
        else scope.launch { launch { dx.animateTo(0f) }; dy.animateTo(0f) }
    }
    val (title, line) = when (state) {
        is PriceToastState.Failed -> state.toast.title to state.toast.line
        is PriceToastState.Retrying -> state.toast.retryingTitle to state.toast.retryingLine
        is PriceToastState.Updated -> state.title to state.line
    }
    val shape = RoundedCornerShape(16.dp)
    Row(
        Modifier.padding(horizontal = 16.dp).widthIn(max = 560.dp).fillMaxWidth()
            .offset { IntOffset(dx.value.roundToInt(), dy.value.roundToInt()) }
            .shadow(8.dp, shape).clip(shape).background(Fob.inverse)
            .pointerInput(Unit) {
                detectDragGestures(onDragEnd = { settle() }, onDragCancel = { settle() }) { change, drag ->
                    change.consume()
                    scope.launch { dx.snapTo(dx.value + drag.x) }
                    scope.launch { dy.snapTo((dy.value + drag.y).coerceAtLeast(0f)) }
                }
            }
            // A swipe isn't there for TalkBack: Dismiss is an action.
            .semantics { customActions = listOf(CustomAccessibilityAction("Dismiss") { onDismiss(); true }) }
            .heightIn(min = 64.dp).padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when (state) {
            is PriceToastState.Failed -> StrokeIcon(Icons.warning, tint = Fob.inverseAccent, size = 22.dp, width = 1.8f)
            is PriceToastState.Retrying -> CircularProgressIndicator(Modifier.size(22.dp), color = Fob.onInverse, strokeWidth = 2.2.dp)
            is PriceToastState.Updated -> StrokeIcon(Icons.check, tint = Fob.inverseGreen, size = 22.dp, width = 2.4f)
        }
        // Read as a status when it comes and when its words change.
        Column(
            Modifier.weight(1f).semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(title, color = Fob.onInverse, fontSize = 14.5.sp, fontWeight = FontWeight.SemiBold)
            if (line.isNotEmpty()) Text(line, color = Fob.inverseMuted, fontSize = 12.5.sp)
        }
        if (state is PriceToastState.Failed) {
            Box(
                Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onRetry).padding(horizontal = 10.dp),
                contentAlignment = Alignment.Center,
            ) { Text("Retry", color = Fob.inverseAccent, fontSize = 14.sp, fontWeight = FontWeight.Bold) }
            Box(
                Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button, onClick = onSettings).semantics { contentDescription = "Price settings" },
                contentAlignment = Alignment.Center,
            ) { StrokeIcon(Icons.gear, tint = Fob.onInverse, size = 20.dp, width = 1.6f) }
        }
    }
}
