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
fun AmountField(value: String, onChange: (String) -> Unit, entry: AmountEntry?, onSwitch: () -> Unit, modifier: Modifier = Modifier, placeholder: String = "") {
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
                .border(1.dp, Fob.outline, RoundedCornerShape(16.dp)).padding(start = 16.dp, end = 8.dp),
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
                    Modifier.minimumInteractiveComponentSize().height(44.dp).clip(RoundedCornerShape(22.dp)).background(Fob.orange)
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
        entry?.line?.let { Text(it, color = Fob.muted, fontSize = 13.sp, modifier = Modifier.padding(start = 4.dp)) }
    }
}
