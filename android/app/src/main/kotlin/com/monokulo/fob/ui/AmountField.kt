package com.monokulo.fob.ui

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.fob.AmountEntry

/** Two arrows, one up and one down: the amount field's switch. */
private const val SWAP = "M7 4v14M3 14l4 4 4-4 M17 20V6M13 10l4-4 4 4"

/**
 * An amount (toggle B): the sign in front, what's typed, and an orange
 * chip naming the unit that switches it, its arrows turning half a turn;
 * the conversion under it. Rust wrote every word ([entry]); [onSwitch]
 * converts what's typed (Rust too). Without a price there's no chip.
 */
@Composable
fun AmountField(value: String, onChange: (String) -> Unit, entry: AmountEntry?, onSwitch: () -> Unit, modifier: Modifier = Modifier, placeholder: String = "", showRate: Boolean = true, optional: Boolean = false) {
    // Each tap turns the arrows another half turn, the same way round.
    var turns by rememberSaveable { mutableIntStateOf(0) }
    val spin by animateFloatAsState(turns * 180f, tween(260, easing = CubicBezierEasing(0.2f, 0f, 0f, 1f)), label = "swap")
    val style = TextStyle(fontFamily = Manrope, fontWeight = FontWeight.Bold, fontSize = 22.sp, color = Fob.ink)
    // Typing, the whole field and its line show above the keyboard and what's pinned over it, not only the cursor.
    val whole = remember { BringIntoViewRequester() }
    val scope = rememberCoroutineScope()
    Column(modifier.fillMaxWidth().bringIntoViewRequester(whole), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            Modifier.fillMaxWidth().height(60.dp).clip(RoundedCornerShape(16.dp)).background(Fob.card)
                .then(if (optional) Modifier.optionalOutline(value.isNotBlank()) else Modifier.border(1.dp, Fob.outline, RoundedCornerShape(16.dp)))
                .padding(start = 16.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            entry?.sign?.let { sign ->
                // Manrope has no ɱ (U+0271): XMR's sign is drawn in the system font, as bold.
                Text(
                    sign, style = style.copy(color = Fob.muted, fontFamily = if (entry.inFiat) Manrope else FontFamily.Default),
                    modifier = Modifier.widthIn(min = 18.dp).clearAndSetSemantics {},
                )
            }
            BasicTextField(
                value = value, onValueChange = onChange, singleLine = true, textStyle = style, cursorBrush = SolidColor(Fob.ink),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                modifier = Modifier.weight(1f).semantics { entry?.let { contentDescription = it.label } }
                    .onFocusChanged { if (it.isFocused) scope.launch { delay(350); whole.bringIntoView() } },
                decorationBox = { field ->
                    Box(contentAlignment = Alignment.CenterStart) {
                        if (value.isEmpty() && placeholder.isNotEmpty()) Text(placeholder, style = style.copy(color = Fob.muted, fontWeight = FontWeight.Medium, fontSize = 17.sp))
                        field()
                    }
                },
            )
            if (entry != null && entry.canSwitch) {
                Row(
                    Modifier.minimumInteractiveComponentSize().height(44.dp).clip(RoundedCornerShape(10.dp)).background(Fob.orange)
                        .clickable(role = Role.Button) { turns++; onSwitch() }
                        .semantics { entry.switchLabel?.let { contentDescription = it } }
                        .padding(start = 16.dp, end = 14.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(entry.unit, color = Fob.onOrange, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.widthIn(min = 34.dp))
                    Box(Modifier.rotate(spin)) { StrokeIcon(SWAP, tint = Fob.onOrange, size = 18.dp) }
                }
            }
        }
        // Its line always laid out, empty when Rust has nothing to say, so what's under the field never moves as one
        // comes or goes ("That isn't an amount", a price coming).
        // Without [showRate], the "1 XMR = $166.10" part is left off: "≈ 0.1204 XMR · 1 XMR = $166.10" is "≈ 0.1204 XMR".
        val line = if (showRate) entry?.line else entry?.conversion
        if (showRate || line != null) Text(line ?: "", color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(start = 4.dp).then(if (line == null) Modifier.clearAndSetSemantics {} else Modifier))
    }
}

/**
 * The amount, big and centred (Receive): the sign and what's typed at 44 sp, and under it Rust's conversion line
 * with the orange unit chip that switches it. Without a price there's no chip.
 */
@Composable
fun BigAmount(value: String, onChange: (String) -> Unit, entry: AmountEntry?, onSwitch: () -> Unit, modifier: Modifier = Modifier) {
    var turns by rememberSaveable { mutableIntStateOf(0) }
    val spin by animateFloatAsState(turns * 180f, tween(260, easing = CubicBezierEasing(0.2f, 0f, 0f, 1f)), label = "swap")
    val style = TextStyle(fontFamily = Manrope, fontWeight = FontWeight.Bold, fontSize = 44.sp, color = Fob.ink, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
    val whole = remember { BringIntoViewRequester() }
    val scope = rememberCoroutineScope()
    Column(modifier.fillMaxWidth().bringIntoViewRequester(whole), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
            entry?.sign?.let { sign ->
                Text(
                    sign, style = style.copy(color = Fob.muted, fontFamily = if (entry.inFiat) Manrope else FontFamily.Default, fontSize = 30.sp),
                    modifier = Modifier.padding(end = 4.dp).clearAndSetSemantics {},
                )
            }
            BasicTextField(
                value = value, onValueChange = onChange, singleLine = true, textStyle = style, cursorBrush = SolidColor(Fob.orange),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                modifier = Modifier.width(androidx.compose.foundation.layout.IntrinsicSize.Min).widthIn(min = 56.dp).semantics { entry?.let { contentDescription = it.label } }
                    .onFocusChanged { if (it.isFocused) scope.launch { delay(350); whole.bringIntoView() } },
                decorationBox = { field ->
                    Box(contentAlignment = Alignment.Center) {
                        if (value.isEmpty()) Text("0", style = style.copy(color = Fob.muted.copy(alpha = 0.6f)))
                        field()
                    }
                },
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.heightIn(min = 44.dp)) {
            val line = entry?.conversion
            if (line != null) Text(line, color = Fob.muted, fontSize = 14.sp)
            if (entry != null && entry.canSwitch) {
                Row(
                    Modifier.minimumInteractiveComponentSize().height(36.dp).clip(RoundedCornerShape(18.dp)).background(Fob.orange)
                        .clickable(role = Role.Button) { turns++; onSwitch() }
                        .semantics { entry.switchLabel?.let { contentDescription = it } }
                        .padding(start = 14.dp, end = 12.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(entry.unit, color = Fob.onOrange, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                    Box(Modifier.rotate(spin)) { StrokeIcon(SWAP, tint = Fob.onOrange, size = 15.dp) }
                }
            }
        }
    }
}
