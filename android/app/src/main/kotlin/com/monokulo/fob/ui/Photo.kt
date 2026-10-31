package com.monokulo.fob.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.fob.contactPhotoFits
import java.io.ByteArrayOutputStream

/**
 * A photo's (side, WebP quality) tried in turn until Rust keeps it (decision 4 of the plan): quality 80 down to 20
 * at 96, 72 and 64 pixels, as decision 109 had it; only when none of those fits, quality 10 and 0 at 64, then 48
 * (decision 149), so a busy photo still fits without every other one going blocky.
 */
private val PHOTO_STEPS: List<Pair<Int, Int>> =
    listOf(96, 72, 64).flatMap { side -> listOf(80, 70, 60, 50, 40, 30, 20).map { side to it } } +
        listOf(64 to 10, 64 to 0, 48 to 10, 48 to 0)

/** Take a photo or choose one; what comes back is already small enough for Rust. */
class PhotoChooser(val take: () -> Unit, val choose: () -> Unit)

/**
 * Take a photo uses the system camera's preview picture (no file, only the
 * camera permission Fob already asks for); choose uses Android's Photo
 * Picker (no storage permission). [onPhoto] gets the WebP Rust keeps, or
 * null when it couldn't be made small enough (decision 109). Used for a
 * contact's photo and for your profile's.
 */
@Composable
fun rememberPhotoChooser(onPhoto: (ByteArray?) -> Unit): PhotoChooser {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    fun use(bitmap: Bitmap?) {
        bitmap ?: return
        scope.launch { onPhoto(withContext(Dispatchers.Default) { smallPhoto(bitmap) }) }
    }
    val takePhoto = rememberLauncherForActivityResult(ActivityResultContracts.TakePicturePreview()) { use(it) }
    // Fob holds the camera permission, so the system camera needs it granted too.
    val askCamera = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { if (it) takePhoto.launch(null) }
    val pickPhoto = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        scope.launch { use(withContext(Dispatchers.IO) { decode(context, uri) }) }
    }
    return remember(takePhoto, askCamera, pickPhoto) {
        PhotoChooser(
            take = {
                if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) takePhoto.launch(null)
                else askCamera.launch(Manifest.permission.CAMERA)
            },
            choose = { pickPhoto.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
        )
    }
}

/**
 * The photo sheet (Round 5 › "Contact photo: take, choose, remove"): Take
 * a photo, Choose from your photos, Remove photo while there's one, and
 * [footnote] under them.
 */
@Composable
fun PhotoSheet(title: String, hasPhoto: Boolean, chooser: PhotoChooser, footnote: String, onRemove: () -> Unit, onDismiss: () -> Unit) {
    BottomSheet(title, onDismiss = onDismiss) {
        SheetHandle()
        Text("Photo", style = display(20), modifier = Modifier.padding(start = 4.dp, top = 4.dp, bottom = 2.dp).semantics { heading() })
        Card {
            SheetRow(Icons.camera, "Take a photo") { onDismiss(); chooser.take() }
            SheetRow(Icons.picture, "Choose from your photos") { onDismiss(); chooser.choose() }
            if (hasPhoto) SheetRow(Icons.trash, "Remove photo", tint = Fob.red) { onDismiss(); onRemove() }
        }
        Text(footnote, color = Fob.muted, fontSize = 12.5.sp, lineHeight = 18.sp, modifier = Modifier.padding(horizontal = 6.dp))
    }
}

/** An avatar with the camera badge on its corner, opening the photo sheet. */
@Composable
fun PhotoButton(name: String, initial: String, photo: ByteArray?, size: Dp, onClick: () -> Unit) {
    val badge = if (size >= 120.dp) 40.dp else 32.dp
    Box(
        Modifier.size(size).clickable(role = Role.Button, indication = null, interactionSource = null, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = if (photo != null) "Change photo" else "Add a photo" },
    ) {
        Avatar(name, initial, photo, size)
        Box(
            Modifier.align(Alignment.BottomEnd).offset(2.dp, 2.dp).size(badge).clip(CircleShape).background(Fob.ink)
                .border(3.dp, Fob.paper, CircleShape),
            contentAlignment = Alignment.Center,
        ) { StrokeIcon(Icons.camera, tint = Fob.paper, size = badge * 0.45f, width = 2.2f) }
    }
}

/** A row of the photo sheet: an icon and what it does. */
@Composable
private fun SheetRow(icon: String, text: String, tint: Color = Fob.ink, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 20.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        StrokeIcon(icon, tint = tint, size = 22.dp)
        Text(text, color = tint, fontSize = 16.sp)
    }
}

/** A picked photo, decoded small enough to crop (and turned as the photo says, from Android 9). */
private fun decode(context: Context, uri: Uri): Bitmap? = runCatching {
    if (Build.VERSION.SDK_INT >= 28) {
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, uri)) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.setTargetSampleSize((minOf(info.size.width, info.size.height) / 384).coerceAtLeast(1))
        }
    } else {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        val sample = Integer.highestOneBit((minOf(bounds.outWidth, bounds.outHeight) / 384).coerceAtLeast(1))
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample }) }
    }
}.getOrNull()

/**
 * A photo from [source]: cropped to a centred square, then scaled and
 * encoded by Android's WebP encoder along [PHOTO_STEPS] until Rust keeps
 * it; null if it never does.
 */
private fun smallPhoto(source: Bitmap): ByteArray? {
    val side = minOf(source.width, source.height)
    if (side <= 0) return null
    val square = Bitmap.createBitmap(source, (source.width - side) / 2, (source.height - side) / 2, side, side)
    val format = if (Build.VERSION.SDK_INT >= 30) Bitmap.CompressFormat.WEBP_LOSSY else @Suppress("DEPRECATION") Bitmap.CompressFormat.WEBP
    val scaled = mutableMapOf<Int, Bitmap>()
    for ((side, quality) in PHOTO_STEPS) {
        val bitmap = scaled.getOrPut(side) { Bitmap.createScaledBitmap(square, side, side, true) }
        val bytes = ByteArrayOutputStream().also { bitmap.compress(format, quality, it) }.toByteArray()
        if (contactPhotoFits(bytes)) return bytes
    }
    return null
}
