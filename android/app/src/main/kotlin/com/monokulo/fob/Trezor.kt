package com.monokulo.fob

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbConstants
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbDeviceConnection
import android.hardware.usb.UsbEndpoint
import android.hardware.usb.UsbInterface
import android.hardware.usb.UsbManager
import android.os.Build
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.suspendCancellableCoroutine
import uniffi.fob.FobException
import uniffi.fob.TrezorLink
import uniffi.fob.TrezorPassphrase
import uniffi.fob.TrezorPrompts
import kotlin.coroutines.resume

/**
 * A Trezor on the phone's USB port: found, asked for, and talked to one
 * 64-byte report at a time, which is all Rust needs of it. Only Trezors
 * running trezor-core (Model T, Safe 3, Safe 5) hold Monero; they all
 * show up as one USB product.
 */
object TrezorUsb {
    private const val VENDOR = 0x1209
    private const val PRODUCT = 0x53c1
    private const val PERMISSION = "com.monokulo.fob.TREZOR_PERMISSION"

    private fun manager(context: Context) = context.getSystemService(Context.USB_SERVICE) as UsbManager

    /** The Trezor plugged in, if one is. */
    fun find(context: Context): UsbDevice? =
        manager(context).deviceList.values.firstOrNull { it.vendorId == VENDOR && it.productId == PRODUCT }

    /** Asks Android (which asks the person) to let Fob use [device]; waits for the answer. */
    private suspend fun permitted(context: Context, device: UsbDevice): Boolean {
        val manager = manager(context)
        if (manager.hasPermission(device)) return true
        return suspendCancellableCoroutine { waiting ->
            val receiver = object : BroadcastReceiver() {
                override fun onReceive(c: Context, intent: Intent) {
                    runCatching { context.unregisterReceiver(this) }
                    if (waiting.isActive) waiting.resume(intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false))
                }
            }
            ContextCompat.registerReceiver(context, receiver, IntentFilter(PERMISSION), ContextCompat.RECEIVER_NOT_EXPORTED)
            // The system fills the answer into this intent, so it has to be mutable.
            val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0
            val intent = PendingIntent.getBroadcast(context, 0, Intent(PERMISSION).setPackage(context.packageName), flags)
            manager.requestPermission(device, intent)
            waiting.invokeOnCancellation { runCatching { context.unregisterReceiver(receiver) } }
        }
    }

    /** Opens the Trezor plugged in, or throws why not in words for the screen. */
    suspend fun open(context: Context): Link {
        val device = find(context) ?: throw FobException.Trezor("No Trezor is plugged in. Connect it with a USB cable (an adapter from the phone's port, if it needs one) and unlock it.")
        return open(context, device)
    }

    /** Opens [device], asking Android (which asks the person) first. */
    suspend fun open(context: Context, device: UsbDevice): Link {
        if (!permitted(context, device)) throw FobException.Trezor("Android didn't let Fob use the Trezor. When it asks whether Fob may, allow it; then unplug the device, plug it in again and try again.")
        return Link(context, manager(context), device)
    }

    /**
     * The reports to and from an opened Trezor. Closed when done, so another
     * app can have it; closed by itself when the cable comes out, so nothing
     * waits on a device that's gone.
     */
    class Link(private val context: Context, manager: UsbManager, private val device: UsbDevice) : TrezorLink, AutoCloseable {
        private val connection: UsbDeviceConnection = manager.openDevice(device) ?: throw FobException.Trezor("The Trezor couldn't be opened. Unplug it and plug it in again.")
        private val detached = object : BroadcastReceiver() {
            override fun onReceive(c: Context, intent: Intent) {
                val gone: UsbDevice? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) intent.getParcelableExtra(UsbManager.EXTRA_DEVICE, UsbDevice::class.java) else @Suppress("DEPRECATION") intent.getParcelableExtra(UsbManager.EXTRA_DEVICE)
                if (gone == null || gone.deviceId == device.deviceId) close()
            }
        }
        private val usbInterface: UsbInterface
        private val input: UsbEndpoint
        private val output: UsbEndpoint
        @Volatile private var closed = false

        init {
            // trezor-core's first interface is the one to talk to; the second is for debugging.
            val candidates = (0 until device.interfaceCount).map(device::getInterface)
            fun endpoints(i: UsbInterface) = (0 until i.endpointCount).map(i::getEndpoint).filter { it.type == UsbConstants.USB_ENDPOINT_XFER_INT || it.type == UsbConstants.USB_ENDPOINT_XFER_BULK }
            val chosen = candidates.firstOrNull { i -> endpoints(i).let { e -> e.any { it.direction == UsbConstants.USB_DIR_IN } && e.any { it.direction == UsbConstants.USB_DIR_OUT } } }
                ?: throw FobException.Trezor("This Trezor doesn't show a way to talk to it. Trezor Model One can't hold Monero.")
            usbInterface = chosen
            input = endpoints(chosen).first { it.direction == UsbConstants.USB_DIR_IN }
            output = endpoints(chosen).first { it.direction == UsbConstants.USB_DIR_OUT }
            if (!connection.claimInterface(usbInterface, true)) {
                connection.close()
                throw FobException.Trezor("Another app is using the Trezor. Close Trezor Suite or the other wallet and try again.")
            }
            ContextCompat.registerReceiver(context, detached, IntentFilter(UsbManager.ACTION_USB_DEVICE_DETACHED), ContextCompat.RECEIVER_EXPORTED)
        }

        override fun write(report: ByteArray) {
            val sent = connection.bulkTransfer(output, report, report.size, WRITE_TIMEOUT)
            if (sent != report.size) throw FobException.Trezor("The Trezor didn't take what Fob sent. Check the cable.")
        }

        /** Waits for the next report: for as long as the person takes on the device, within reason. */
        override fun read(): ByteArray {
            val report = ByteArray(REPORT)
            val deadline = System.currentTimeMillis() + READ_PATIENCE
            while (!closed && System.currentTimeMillis() < deadline) {
                val got = connection.bulkTransfer(input, report, REPORT, READ_SLICE)
                if (got > 0) return if (got == REPORT) report else report.copyOf(got)
            }
            throw FobException.Trezor(if (closed) "The Trezor was disconnected." else "The Trezor didn't answer in time.")
        }

        override fun close() {
            if (closed) return
            closed = true
            runCatching { context.unregisterReceiver(detached) }
            runCatching { connection.releaseInterface(usbInterface) }
            connection.close()
        }

        private companion object {
            const val REPORT = 64
            const val WRITE_TIMEOUT = 3_000
            /** Each wait on the device, so a closed link (unplugged, or cancelled) is noticed soon. */
            const val READ_SLICE = 300
            /** Reading gives up after this, in milliseconds: five minutes to find the device and press. */
            const val READ_PATIENCE = 5 * 60_000L
        }
    }
}

/** What the device's hello said: its name, and what it will ask for. */
data class TrezorHello(val name: String, val pinProtection: Boolean, val passphraseProtection: Boolean)

/** What the Trezor is asking of the person right now, for the screen to show. */
sealed interface TrezorAsk {
    /** No device has answered yet: Fob looks for one on USB until one does. */
    data object Looking : TrezorAsk
    /** The device answered the hello. */
    data class Hello(val hello: TrezorHello) : TrezorAsk
    /** The exchange failed; [problem] says how, and whether Try again is offered. */
    data class Problem(val problem: uniffi.fob.TrezorProblem) : TrezorAsk
    /** Look at the device: it shows [what] to confirm. */
    data class Button(val what: String) : TrezorAsk
    /** Its PIN, by the positions on the keypad the device shows. */
    data object Pin : TrezorAsk
    /** Its passphrase; [onDevice] when the device can take it itself. */
    data class Passphrase(val onDevice: Boolean) : TrezorAsk
    /** A long exchange, [done] of [total]. */
    data class Progress(val done: UInt, val total: UInt) : TrezorAsk
}

/**
 * Rust's questions from the device, turned into [ask] for the screens, and
 * the answers back. Rust calls these on its own thread and waits, so a
 * question blocks there until a screen answers it.
 */
class TrezorAsks : TrezorPrompts {
    private val _ask = MutableStateFlow<TrezorAsk?>(null)
    val ask: StateFlow<TrezorAsk?> = _ask

    /** Whether a device exchange is under way at all, for the screen to show. */
    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy

    /** What the device's hello said, once it has: the steps are counted off it. */
    private val _hello = MutableStateFlow<TrezorHello?>(null)
    val hello: StateFlow<TrezorHello?> = _hello

    /** The address the device is about to show, as Fob received it, for the screen to show in full. */
    private val _address = MutableStateFlow<String?>(null)
    val address: StateFlow<String?> = _address

    private var pinAnswer: CompletableDeferred<String?>? = null
    private var passphraseAnswer: CompletableDeferred<TrezorPassphrase>? = null
    private var retryAnswer: CompletableDeferred<Boolean>? = null

    /** The person gave up while Fob was still looking for a device. */
    @Volatile var cancelled = false
        private set

    /** Ends the exchange from the phone's side: closes the link, so Rust's wait ends. Set while one is on. */
    @Volatile var onCancel: (() -> Unit)? = null

    /** An exchange begins: the screen shows, looking for a device. */
    fun begin() {
        onCancel = null
        cancelled = false
        _hello.value = null
        _address.value = null
        _busy.value = true
        _ask.value = TrezorAsk.Looking
    }

    /** A device is open: the cancel lets go of it. */
    fun hold(onCancel: () -> Unit) {
        this.onCancel = onCancel
    }

    fun end() {
        _busy.value = false
        _ask.value = null
        _hello.value = null
        _address.value = null
        onCancel = null
        pinAnswer?.cancel()
        passphraseAnswer?.cancel()
        retryAnswer?.cancel()
    }

    /** The person gives up: any question is answered no, and the device is let go. */
    fun cancel() {
        cancelled = true
        pinAnswer?.complete(null)
        passphraseAnswer?.complete(TrezorPassphrase.Cancelled)
        retryAnswer?.complete(false)
        onCancel?.invoke()
    }

    /** Fob is looking for a device again (after Try again). */
    fun looking() {
        cancelled = false
        _hello.value = null
        _address.value = null
        _ask.value = TrezorAsk.Looking
    }

    /**
     * The exchange failed: the card says how, with Try again where it
     * makes sense. Waits for the person: true to start over, false to stop.
     */
    suspend fun problem(reason: String, cancelledOnDevice: Boolean): Boolean {
        val shown = uniffi.fob.trezorProblem(reason, cancelledOnDevice)
        val answer = CompletableDeferred<Boolean>()
        retryAnswer = answer
        _ask.value = TrezorAsk.Problem(shown)
        return try {
            answer.await()
        } catch (cancelled: Exception) {
            false
        } finally {
            retryAnswer = null
        }
    }

    fun retry() {
        retryAnswer?.complete(true)
    }

    override fun onHello(name: String, pinProtection: Boolean, passphraseProtection: Boolean) {
        _hello.value = TrezorHello(name, pinProtection, passphraseProtection)
    }

    override fun onAddress(address: String) {
        _address.value = address
    }

    override fun onButton(what: String) {
        _ask.value = TrezorAsk.Button(what)
    }

    override fun pin(): String? {
        val answer = CompletableDeferred<String?>()
        pinAnswer = answer
        _ask.value = TrezorAsk.Pin
        return try {
            runBlocking { answer.await() }
        } catch (cancelled: Exception) {
            null
        } finally {
            _ask.value = null
        }
    }

    override fun passphrase(onDevice: Boolean): TrezorPassphrase {
        val answer = CompletableDeferred<TrezorPassphrase>()
        passphraseAnswer = answer
        _ask.value = TrezorAsk.Passphrase(onDevice)
        return try {
            runBlocking { answer.await() }
        } catch (cancelled: Exception) {
            TrezorPassphrase.Cancelled
        } finally {
            _ask.value = null
        }
    }

    override fun progress(done: UInt, total: UInt) {
        _ask.value = TrezorAsk.Progress(done, total)
    }

    fun answerPin(pin: String?) {
        pinAnswer?.complete(pin)
    }

    fun answerPassphrase(answer: TrezorPassphrase) {
        passphraseAnswer?.complete(answer)
    }
}
