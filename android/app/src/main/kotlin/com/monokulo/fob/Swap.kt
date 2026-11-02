package com.monokulo.fob

import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.nfc.NfcAdapter
import android.nfc.Tag
import android.nfc.cardemulation.CardEmulation
import android.nfc.cardemulation.HostApduService
import android.nfc.tech.IsoDep
import android.os.Bundle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.fob.Network
import uniffi.fob.PayCard
import uniffi.fob.PayReader
import uniffi.fob.PayStep
import uniffi.fob.ReaderStep
import uniffi.fob.TouchSwap
import uniffi.fob.payIsSelect
import uniffi.fob.payNotAvailable
import uniffi.fob.swapNotAvailable
import kotlin.random.Random

/**
 * Touch phones (Round 6 › "Touch phones"): while Share my profile is in
 * front, this phone is a card ([SwapService]) and, at random moments, a
 * reader ([SwapRunner]), so two phones on that screen settle into one of
 * each. Every byte goes to Rust's [TouchSwap] and back; nothing here knows
 * the protocol.
 */
object TouchPhones {
    /** The swap the card service answers for: set only while Share my profile is resumed. */
    @Volatile var session: TouchSwap? = null
    /** The swap that has their profile, from either side. */
    val swapped = MutableStateFlow<TouchSwap?>(null)
    /** Why the last touch took nothing, in Rust's words. */
    val problem = MutableStateFlow<String?>(null)
    /** The payment code the card service answers for: set only while Receive is resumed. */
    @Volatile var pay: PayCard? = null
    /** Counts the times a reader took Receive's whole code. */
    val payRead = MutableStateFlow(0)
}

/**
 * Fob's card side (`res/xml/apduservice.xml`: Fob's own application, an
 * unlocked phone). With no Share my profile in front it answers "not
 * available" to everything, so nothing is read from a phone in a pocket.
 */
class SwapService : HostApduService() {
    /** The reader chose the payment application (Receive's code) rather than the swap's, for this link. */
    private var viaPay = false

    override fun processCommandApdu(commandApdu: ByteArray, extras: Bundle?): ByteArray {
        // A select says which of Fob's two applications this link is for; what follows goes to the same one.
        val select = commandApdu.size >= 2 && commandApdu[1] == 0xA4.toByte()
        if (select) viaPay = payIsSelect(commandApdu)
        if (viaPay) {
            val card = TouchPhones.pay ?: return payNotAvailable()
            val answer = card.cardAnswer(commandApdu)
            if (card.read()) TouchPhones.payRead.value += 1
            return answer
        }
        val swap = TouchPhones.session ?: return swapNotAvailable()
        val answer = swap.cardAnswer(commandApdu)
        if (swap.swapped()) TouchPhones.swapped.value = swap
        return answer
    }

    override fun onDeactivated(reason: Int) {
        TouchPhones.session?.cardLost()
        TouchPhones.pay?.cardLost()
        viaPay = false
    }
}

/** NFC on this phone: none (or no card emulation), off, or on. */
enum class Nfc { NONE, OFF, ON }

fun nfcState(context: Context): Nfc {
    val adapter = NfcAdapter.getDefaultAdapter(context) ?: return Nfc.NONE
    if (!context.packageManager.hasSystemFeature(PackageManager.FEATURE_NFC_HOST_CARD_EMULATION)) return Nfc.NONE
    return if (adapter.isEnabled) Nfc.ON else Nfc.OFF
}

/**
 * Runs [swap] while started: the card service answers for it (Fob's
 * service preferred while in front), and reader mode goes on and off at
 * random 150–450 ms, never mid-exchange, since a phone reading isn't a
 * card.
 */
class SwapRunner(private val activity: Activity, private val swap: TouchSwap, private val scope: CoroutineScope) {
    private val adapter = NfcAdapter.getDefaultAdapter(activity)
    private var job: Job? = null

    fun start() {
        val nfc = adapter ?: return
        TouchPhones.problem.value = null
        TouchPhones.session = swap
        runCatching { CardEmulation.getInstance(nfc).setPreferredService(activity, ComponentName(activity, SwapService::class.java)) }
        job = scope.launch {
            while (isActive) {
                runCatching { nfc.enableReaderMode(activity, ::read, READER_FLAGS, null) }
                pause()
                runCatching { nfc.disableReaderMode(activity) }
                pause()
            }
        }
    }

    fun stop() {
        job?.cancel()
        job = null
        if (TouchPhones.session === swap) TouchPhones.session = null
        adapter?.let {
            runCatching { it.disableReaderMode(activity) }
            runCatching { CardEmulation.getInstance(it).unsetPreferredService(activity) }
        }
    }

    private suspend fun pause() {
        delay(Random.nextLong(150, 451))
        while (swap.busy()) delay(50)
    }

    /** A card in the field, on NFC's thread: the whole exchange, then close. */
    private fun read(tag: Tag) {
        val iso = IsoDep.get(tag) ?: return
        var apdu = swap.readerStart() ?: return
        try {
            iso.connect()
            iso.timeout = 1000
            while (true) {
                when (val step = swap.readerNext(iso.transceive(apdu))) {
                    is ReaderStep.Send -> apdu = step.apdu
                    is ReaderStep.Done -> { TouchPhones.swapped.value = swap; return }
                    is ReaderStep.Failed -> { TouchPhones.problem.value = step.reason; return }
                }
            }
        } catch (_: Exception) {
            // The phones moved apart: the next touch starts over.
            swap.readerLost()
        } finally {
            runCatching { iso.close() }
        }
    }

    private companion object {
        const val READER_FLAGS = NfcAdapter.FLAG_READER_NFC_A or NfcAdapter.FLAG_READER_NFC_B or
            NfcAdapter.FLAG_READER_SKIP_NDEF_CHECK or NfcAdapter.FLAG_READER_NO_PLATFORM_SOUNDS
    }
}

/**
 * Touch to pay, the reader: while started (Send or Home in front) reader
 * mode stays on, and a phone on Receive gives its payment code, handed to
 * [onCode] on the main thread; a touch that took nothing says why to
 * [onProblem]. The protocol is Rust's [PayReader].
 */
class PayRunner(private val activity: Activity, network: Network, private val onCode: (String) -> Unit, private val onProblem: (String) -> Unit) {
    private val adapter = NfcAdapter.getDefaultAdapter(activity)
    private val reader = PayReader(network)

    fun start() {
        val nfc = adapter ?: return
        runCatching { nfc.enableReaderMode(activity, ::read, FLAGS, null) }
    }

    fun stop() {
        adapter?.let { runCatching { it.disableReaderMode(activity) } }
    }

    /** A card in the field, on NFC's thread: the whole read, then close. */
    private fun read(tag: Tag) {
        val iso = IsoDep.get(tag) ?: return
        var apdu = reader.start()
        try {
            iso.connect()
            iso.timeout = 1000
            while (true) {
                when (val step = reader.next(iso.transceive(apdu))) {
                    is PayStep.Send -> apdu = step.apdu
                    is PayStep.Done -> { activity.runOnUiThread { onCode(step.code) }; return }
                    is PayStep.Failed -> { activity.runOnUiThread { onProblem(step.reason) }; return }
                }
            }
        } catch (_: Exception) {
            // The phones moved apart: the next touch starts over.
            reader.lost()
        } finally {
            runCatching { iso.close() }
        }
    }

    private companion object {
        const val FLAGS = NfcAdapter.FLAG_READER_NFC_A or NfcAdapter.FLAG_READER_NFC_B or
            NfcAdapter.FLAG_READER_SKIP_NDEF_CHECK or NfcAdapter.FLAG_READER_NO_PLATFORM_SOUNDS
    }
}

/** Fob's service made the one to answer while a screen is in front, and let go after. */
fun preferFobService(activity: Activity, on: Boolean) {
    val nfc = NfcAdapter.getDefaultAdapter(activity) ?: return
    runCatching {
        val card = CardEmulation.getInstance(nfc)
        if (on) card.setPreferredService(activity, ComponentName(activity, SwapService::class.java)) else card.unsetPreferredService(activity)
    }
}
