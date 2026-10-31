package com.monokulo.fob.ui

import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import java.util.concurrent.Executors

/**
 * The camera, reading QR codes; [onText] gets each one read (on the main
 * thread). The light is on while [torch]; [onHasTorch] says, once the
 * camera is open, whether it has one.
 */
@Composable
fun QrScanner(onText: (String) -> Unit, modifier: Modifier = Modifier, torch: Boolean = false, onHasTorch: (Boolean) -> Unit = {}) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val executor = remember { Executors.newSingleThreadExecutor() }
    // The camera outlives a recomposition: it calls whatever was passed last.
    val read by rememberUpdatedState(onText)
    val hasTorch by rememberUpdatedState(onHasTorch)
    var camera by remember { mutableStateOf<Camera?>(null) }
    DisposableEffect(Unit) { onDispose { executor.shutdown() } }
    LaunchedEffect(camera, torch) {
        val c = camera ?: return@LaunchedEffect
        if (c.cameraInfo.hasFlashUnit()) c.cameraControl.enableTorch(torch)
    }
    AndroidView(
        modifier = modifier,
        factory = { ctx ->
            val view = PreviewView(ctx)
            val future = ProcessCameraProvider.getInstance(ctx)
            future.addListener({
                val provider = future.get()
                val preview = Preview.Builder().build().also { it.surfaceProvider = view.surfaceProvider }
                val reader = MultiFormatReader().apply {
                    setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
                }
                val analysis = ImageAnalysis.Builder()
                    .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                    .build()
                analysis.setAnalyzer(executor) { image ->
                    image.use {
                        val plane = it.planes[0]
                        val bytes = ByteArray(plane.buffer.remaining()).also(plane.buffer::get)
                        val source = PlanarYUVLuminanceSource(
                            bytes, plane.rowStride, it.height, 0, 0, it.width, it.height, false,
                        )
                        runCatching { reader.decodeWithState(BinaryBitmap(HybridBinarizer(source))) }
                            .onSuccess { result -> ContextCompat.getMainExecutor(context).execute { read(result.text) } }
                        reader.reset()
                    }
                }
                provider.unbindAll()
                val bound = provider.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
                camera = bound
                hasTorch(bound.cameraInfo.hasFlashUnit())
            }, ContextCompat.getMainExecutor(ctx))
            view
        },
    )
}
