package com.monokulo.fob.ui

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp

/**
 * A contact's avatar (Shared › "Contact avatar"): their photo in a circle,
 * or the initial Rust gives on the sunken colour. With [describe], TalkBack
 * reads "Sam's photo" or "Sam"; inside a row that already says the name,
 * it reads nothing.
 */
@Composable
fun Avatar(name: String, initial: String, photo: ByteArray?, size: Dp, modifier: Modifier = Modifier, describe: Boolean = false) {
    val image = remember(photo) { photo?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() } }
    val label = if (image != null) "$name's photo" else name
    Box(
        modifier.size(size).clip(CircleShape).background(Fob.sunken)
            .clearAndSetSemantics { if (describe) contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        if (image != null) {
            Image(image, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else {
            Text(initial, fontFamily = Manrope, fontWeight = FontWeight.ExtraBold, fontSize = (size.value * 0.44f).sp, color = Fob.ink, maxLines = 1)
        }
    }
}
