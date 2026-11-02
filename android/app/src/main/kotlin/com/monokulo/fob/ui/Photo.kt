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
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.text.font.FontWeight
import kotlin.math.roundToInt
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
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
import uniffi.fob.contactPhotoKept
import java.io.ByteArrayOutputStream

/**
 * A photo's (side, WebP quality) tried in turn until Rust keeps it (at most 8 KB, since photos only travel by
 * touching phones and never in a QR code): quality 90 down to 40 at 128, 112 and 96 pixels, and only when none of
 * those fits, lower qualities and smaller sides, so a busy photo still fits without every other one going blocky.
 */
private val PHOTO_STEPS: List<Pair<Int, Int>> =
    listOf(128, 112, 96).flatMap { side -> listOf(90, 80, 70, 60, 50, 40).map { q -> side to q } } +
        listOf(96 to 30, 96 to 20, 72 to 30, 72 to 10, 64 to 10, 48 to 0)

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
    // A picked or taken photo waits here to be moved and scaled under the circle.
    var cropping by remember { mutableStateOf<Bitmap?>(null) }
    fun use(bitmap: Bitmap?) {
        bitmap ?: return
        cropping = bitmap
    }
    cropping?.let { source ->
        PhotoCropDialog(
            source, onCancel = { cropping = null },
            onUse = { square -> cropping = null; scope.launch { onPhoto(withContext(Dispatchers.Default) { smallPhoto(square) }) } },
        )
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
 * a photo, Choose from your photos, and Remove photo while there's one.
 */
@Composable
fun PhotoSheet(title: String, hasPhoto: Boolean, chooser: PhotoChooser, onRemove: () -> Unit, onDismiss: () -> Unit) {
    BottomSheet(title, onDismiss = onDismiss) {
        SheetHandle()
        Text("Photo", style = display(20), modifier = Modifier.padding(start = 4.dp, top = 4.dp, bottom = 2.dp).semantics { heading() })
        Card {
            SheetRow(Icons.camera, "Take a photo") { onDismiss(); chooser.take() }
            SheetRow(Icons.picture, "Choose from your photos") { onDismiss(); chooser.choose() }
            if (hasPhoto) SheetRow(Icons.trash, "Remove photo", tint = Fob.red) { onDismiss(); onRemove() }
        }
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
 * A photo from [source]: a square (cropped by the user, or centred if not), scaled and
 * encoded by Android's WebP encoder along [PHOTO_STEPS] until Rust keeps
 * it; null if it never does.
 */
private fun smallPhoto(source: Bitmap): ByteArray? {
    val side = minOf(source.width, source.height)
    if (side <= 0) return null
    val square = if (source.width == source.height) source else Bitmap.createBitmap(source, (source.width - side) / 2, (source.height - side) / 2, side, side)
    val format = if (Build.VERSION.SDK_INT >= 30) Bitmap.CompressFormat.WEBP_LOSSY else @Suppress("DEPRECATION") Bitmap.CompressFormat.WEBP
    val scaled = mutableMapOf<Int, Bitmap>()
    for ((side, quality) in PHOTO_STEPS) {
        val bitmap = scaled.getOrPut(side) { Bitmap.createScaledBitmap(square, side, side, true) }
        val bytes = ByteArrayOutputStream().also { bitmap.compress(format, quality, it) }.toByteArray()
        contactPhotoKept(bytes)?.let { return it }
    }
    return null
}


/** Zoom runs from the circle just filled by the short side (1) to this many times closer. */
private const val MAX_ZOOM = 4f

/**
 * Move and scale: the photo under a circle (what everyone will see), moved
 * by dragging and zoomed by pinching or by the slider. Use photo gives the
 * square that circle covers; the avatar shows it in a circle.
 */
@Composable
private fun PhotoCropDialog(source: Bitmap, onCancel: () -> Unit, onUse: (Bitmap) -> Unit) {
    Dialog(onDismissRequest = onCancel, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        val image = remember(source) { source.asImageBitmap() }
        var zoom by remember { mutableFloatStateOf(1f) }
        // The photo's own point under the circle's centre, in the photo's pixels.
        var centre by remember { mutableStateOf(Offset(source.width / 2f, source.height / 2f)) }
        // The circle's diameter on screen and the scale at which the photo's short side fills it, set as it's laid out.
        var diameter by remember { mutableFloatStateOf(1f) }
        val base = diameter / minOf(source.width, source.height)
        fun held(c: Offset, z: Float): Offset {
            val half = diameter / 2f / (base * z)
            return Offset(c.x.coerceIn(half, source.width - half), c.y.coerceIn(half, source.height - half))
        }
        Column(Modifier.fillMaxSize().background(Color.Black).safeDrawingPadding()) {
            Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = EdgeButton.inset), verticalAlignment = Alignment.CenterVertically) {
                EdgeIconButton(Icons.close, "Cancel", onCancel, tint = Color.White)
                Text("Move and scale", color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f).semantics { heading() })
            }
            BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
                val density = LocalDensity.current
                val w = with(density) { maxWidth.toPx() }
                val h = with(density) { maxHeight.toPx() }
                diameter = minOf(w - with(density) { 48.dp.toPx() }, h - with(density) { 24.dp.toPx() }).coerceAtLeast(1f)
                Canvas(
                    Modifier.fillMaxSize()
                        .pointerInput(source) {
                            detectTransformGestures { _, pan, gestureZoom, _ ->
                                val z = (zoom * gestureZoom).coerceIn(1f, MAX_ZOOM)
                                centre = held(Offset(centre.x - pan.x / (base * z), centre.y - pan.y / (base * z)), z)
                                zoom = z
                            }
                        }
                        .semantics { contentDescription = "Your photo. Drag to move, pinch to zoom" },
                ) {
                    val s = base * zoom
                    val middle = Offset(size.width / 2f, size.height / 2f)
                    drawImage(
                        image, dstOffset = IntOffset((middle.x - centre.x * s).roundToInt(), (middle.y - centre.y * s).roundToInt()),
                        dstSize = IntSize((source.width * s).roundToInt(), (source.height * s).roundToInt()),
                    )
                    // Dim everything but the circle: the photo itself stays clear inside it (a Clear blend would cut the photo out too).
                    val dim = androidx.compose.ui.graphics.Path().apply {
                        fillType = androidx.compose.ui.graphics.PathFillType.EvenOdd
                        addRect(androidx.compose.ui.geometry.Rect(0f, 0f, size.width, size.height))
                        addOval(androidx.compose.ui.geometry.Rect(middle, diameter / 2f))
                    }
                    drawPath(dim, Color(0xCC080807))
                    drawCircle(Color.White, radius = diameter / 2f, center = middle, style = Stroke(2.dp.toPx()))
                }
                Text(
                    "Drag to move · pinch to zoom", color = Color.White.copy(alpha = 0.85f), fontSize = 14.sp, fontWeight = FontWeight.Medium,
                    modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 4.dp),
                )
            }
            Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("−", color = Color.White, fontSize = 22.sp)
                    Slider(
                        value = zoom, onValueChange = { z -> zoom = z; centre = held(centre, z) },
                        valueRange = 1f..MAX_ZOOM, modifier = Modifier.weight(1f).semantics { contentDescription = "Zoom" },
                        colors = SliderDefaults.colors(thumbColor = Color.White, activeTrackColor = Color.White, inactiveTrackColor = Color.White.copy(alpha = 0.3f)),
                    )
                    Text("+", color = Color.White, fontSize = 22.sp)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    OutlineButton("Cancel", onCancel, Modifier.weight(1f), tint = Color.White, edge = Color.White.copy(alpha = 0.6f))
                    PrimaryButton("Use photo", {
                        val half = diameter / 2f / (base * zoom)
                        val side = (half * 2).roundToInt().coerceIn(1, minOf(source.width, source.height))
                        val left = (centre.x - half).roundToInt().coerceIn(0, source.width - side)
                        val top = (centre.y - half).roundToInt().coerceIn(0, source.height - side)
                        onUse(Bitmap.createBitmap(source, left, top, side, side))
                    }, Modifier.weight(1f))
                }
            }
        }
    }
}
