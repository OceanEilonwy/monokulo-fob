package com.monokulo.fob.ui

import android.graphics.Bitmap
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
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.fob.decodePhoto

/** Photos decoded once, so a list draws each contact's without decoding it again. */
private val decoded = LruCache<String, ImageBitmap>(64)

private fun key(photo: ByteArray) = "${photo.contentHashCode()}:${photo.size}"

/**
 * [photo] as a bitmap, null when it can't be (the initial shows).
 *
 * Decoded by Rust ([decodePhoto], the pure-Rust image-webp), never by
 * BitmapFactory: a contact's photo comes from someone else, a phone touched
 * while Meet is open saves it without asking, and Rust checks only its
 * headers when it keeps it. Android's own WebP decoder is C (Skia's libwebp),
 * where a crafted picture has reached native code before (CVE-2023-4863), so
 * a stranger's bytes never reach it; only the pixels Rust made do. Your own
 * photo is drawn the same way, as it's the same kind of kept WebP; only the
 * picture you pick to make it from goes through Android's decoder (Photo.kt),
 * since you chose that picture yourself.
 */
private fun photoBitmap(photo: ByteArray, key: String = key(photo)): ImageBitmap? = runCatching {
    val pixels = decodePhoto(photo) ?: return@runCatching null
    val width = pixels.width.toInt()
    val height = pixels.height.toInt()
    val rgba = pixels.rgba
    if (width <= 0 || height <= 0 || rgba.size != width * height * 4) return@runCatching null
    // Colours as ARGB ints, which createBitmap premultiplies itself: copying
    // RGBA bytes straight in would skip that, and a photo with see-through
    // edges would draw too bright.
    val colors = IntArray(width * height) { i ->
        val at = i * 4
        (rgba[at + 3].toInt() and 0xff shl 24) or (rgba[at].toInt() and 0xff shl 16) or (rgba[at + 1].toInt() and 0xff shl 8) or (rgba[at + 2].toInt() and 0xff)
    }
    Bitmap.createBitmap(colors, width, height, Bitmap.Config.ARGB_8888).asImageBitmap()
}.getOrNull()?.also { decoded.put(key, it) }

/**
 * A contact's avatar in a list row ([Avatar] otherwise), the photo named by
 * the row's [photoKey] and fetched by [id] through [photo] only when it isn't
 * decoded already (decision 414). Inside a row that says the name, it reads
 * nothing to TalkBack.
 */
@Composable
fun ContactAvatar(name: String, initial: String, id: ULong, photoKey: String?, size: Dp, photo: suspend (ULong) -> ByteArray?, modifier: Modifier = Modifier) {
    // A row names its contact's photo by a key, and the bytes cross from
    // Rust once per key, not with every row of every refresh: fetched and
    // decoded off the main thread, then drawn from the cache.
    val image by produceState(photoKey?.let { decoded.get(it) }, photoKey) {
        value = photoKey?.let { k -> decoded.get(k) ?: withContext(Dispatchers.Default) { photo(id)?.let { photoBitmap(it, k) } } }
    }
    AvatarOf(name, initial, image, size, modifier, describe = false, fill = Fob.sunken)
}

/**
 * A contact's avatar (Shared › "Contact avatar"): their photo in a circle,
 * or the initial Rust gives on [fill] (sunken, on a card). With [describe], TalkBack
 * reads "Sam's photo" or "Sam"; inside a row that already says the name,
 * it reads nothing.
 */
@Composable
fun Avatar(name: String, initial: String, photo: ByteArray?, size: Dp, modifier: Modifier = Modifier, describe: Boolean = false, fill: Color = Fob.sunken) {
    // Decoded off the main thread; one decoded before shows at once. At most
    // 128 pixels a side, it's scaled to the avatar as it's drawn.
    val image by produceState(photo?.let { decoded.get(key(it)) }, photo) {
        value = photo?.let { p -> decoded.get(key(p)) ?: withContext(Dispatchers.Default) { photoBitmap(p) } }
    }
    AvatarOf(name, initial, image, size, modifier, describe, fill)
}

@Composable
private fun AvatarOf(name: String, initial: String, shown: ImageBitmap?, size: Dp, modifier: Modifier, describe: Boolean, fill: Color) {
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
