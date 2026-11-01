package com.monokulo.fob.ui

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** The most pixels a side of a photo Rust keeps; anything claiming more isn't decoded. */
private const val PHOTO_SIDE_MAX = 128

/** Photos decoded for their size on screen, so a list draws each contact's once. */
private val decoded = LruCache<String, ImageBitmap>(64)

private fun key(photo: ByteArray, px: Int) = "${photo.contentHashCode()}:${photo.size}:$px"

/**
 * [photo] decoded for [px] pixels a side: its header read first, nothing
 * decoded that claims more than Rust keeps, scaled down as it's decoded;
 * null when it can't be (the initial shows).
 */
private fun decodePhoto(photo: ByteArray, px: Int): ImageBitmap? = runCatching {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(photo, 0, photo.size, bounds)
    val side = maxOf(bounds.outWidth, bounds.outHeight)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0 || side > PHOTO_SIDE_MAX) return@runCatching null
    var sample = 1
    while (side / (sample * 2) >= px) sample *= 2
    BitmapFactory.decodeByteArray(photo, 0, photo.size, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
}.getOrNull()?.also { decoded.put(key(photo, px), it) }

/**
 * A contact's avatar (Shared › "Contact avatar"): their photo in a circle,
 * or the initial Rust gives on [fill] (sunken, on a card). With [describe], TalkBack
 * reads "Sam's photo" or "Sam"; inside a row that already says the name,
 * it reads nothing.
 */
@Composable
fun Avatar(name: String, initial: String, photo: ByteArray?, size: Dp, modifier: Modifier = Modifier, describe: Boolean = false, fill: Color = Fob.sunken) {
    val px = with(LocalDensity.current) { size.roundToPx() }.coerceAtLeast(1)
    // Decoded off the main thread; one decoded before shows at once.
    val image by produceState(photo?.let { decoded.get(key(it, px)) }, photo, px) {
        value = photo?.let { p -> decoded.get(key(p, px)) ?: withContext(Dispatchers.Default) { decodePhoto(p, px) } }
    }
    val shown = image
    val label = if (shown != null) "$name's photo" else name
    Box(
        modifier.size(size).clip(CircleShape).background(fill)
            .clearAndSetSemantics { if (describe) contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        if (shown != null) {
            Image(shown, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else {
            Text(initial, fontFamily = Manrope, fontWeight = FontWeight.ExtraBold, fontSize = (size.value * 0.44f).sp, color = Fob.ink, maxLines = 1)
        }
    }
}
