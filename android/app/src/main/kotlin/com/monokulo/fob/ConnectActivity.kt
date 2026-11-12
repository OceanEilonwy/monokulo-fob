package com.monokulo.fob

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.graphics.drawable.toBitmap
import androidx.fragment.app.FragmentActivity
import com.monokulo.fob.ui.FobTheme
import com.monokulo.fob.ui.NoScreenshots
import com.monokulo.fob.ui.reason
import com.monokulo.fob.ui.screens.ConnectAnswerScreen
import com.monokulo.fob.ui.screens.ConnectConsentScreen
import com.monokulo.fob.ui.screens.ConnectNewScreen
import com.monokulo.fob.ui.screens.ConnectPickScreen
import com.monokulo.fob.ui.screens.ConnectScanScreen
import com.monokulo.fob.ui.screens.NEW_WALLET
import com.monokulo.fob.ui.screens.PickRow
import kotlinx.coroutines.launch
import uniffi.fob.ConnectAnswer
import uniffi.fob.ConnectRequest
import uniffi.fob.ConnectShares
import uniffi.fob.ConnectedApp
import uniffi.fob.connectDeclined
import uniffi.fob.connectRequest
import uniffi.fob.connectWords
import uniffi.fob.isConnectRequest

/**
 * Connect to Fob (decisions 422, 423): another app asks for view access to one of Fob's wallets, three ways.
 *
 * - **The same phone, for a result.** The app starts this with an explicit intent ([ACTION_CONNECT], Fob's package,
 *   the request as its data) and `startActivityForResult`. Android says which app it is (`callingActivity`), so
 *   Choose a wallet can mark a known one verified ([KnownApps]). The answer comes back as the result's data: the
 *   sealed answer with `RESULT_OK`, or the declined one with `RESULT_CANCELED`. Nothing goes through a link.
 * - **A `fob://connect` link.** No caller is known; the answer opens the request's callback, or, without one, shows
 *   as a code (C3).
 * - **Scanned** with Fob's own camera (Send): the request's pattern is checked against the other device's first
 *   (C2), and the answer is a code (C3).
 *
 * Its own activity, not MainActivity's back stack: MainActivity is `singleTask`, which Android won't start for a
 * result in another app's task.
 */
class ConnectActivity : FragmentActivity() {
    private val model: WalletModel get() = (application as FobApp).model

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val text = intent?.dataString ?: intent?.getStringExtra(EXTRA_REQUEST)
        val request = text?.takeIf(::isConnectRequest)?.let { runCatching { connectRequest(it) }.getOrElse { e -> refuse(e.reason()); return } }
        if (text == null || request == null) {
            refuse("That isn't a Connect to Fob code.")
            return
        }
        val caller = callingActivity?.packageName
        val forResult = caller != null
        val verified = KnownApps.verified(this, caller, request.app)
        val icon = caller?.let { appIcon(it) }
        val scanned = !forResult && intent.getBooleanExtra(EXTRA_SCANNED, false)
        setContent {
            val theme by model.theme.collectAsState()
            FobTheme(theme) {
                ConnectFlow(
                    model, text, request, icon, verified, scanned,
                    returns = forResult || request.hasCallback,
                    onAnswer = { link, yes -> answer(link, yes, forResult) },
                    onClose = { finish() },
                )
            }
        }
    }

    private fun refuse(reason: String) {
        Toast.makeText(this, reason, Toast.LENGTH_LONG).show()
        setResult(RESULT_CANCELED)
        finish()
    }

    /** The answer [link] back to the app: as the result, or by opening it. */
    private fun answer(link: String, yes: Boolean, forResult: Boolean) {
        if (forResult) {
            setResult(if (yes) RESULT_OK else RESULT_CANCELED, Intent().setData(Uri.parse(link)))
        } else {
            runCatching { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(link)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                .onFailure { Toast.makeText(this, "The app that asked isn't there to take the answer.", Toast.LENGTH_LONG).show() }
        }
        finish()
    }

    private fun appIcon(pkg: String): ImageBitmap? =
        runCatching { packageManager.getApplicationIcon(pkg).toBitmap(96, 96).asImageBitmap() }.getOrNull()

    companion object {
        /** The explicit intent's action. */
        const val ACTION_CONNECT = "com.monokulo.fob.action.CONNECT"
        /** The request, for Fob's own scanner. */
        private const val EXTRA_REQUEST = "com.monokulo.fob.extra.REQUEST"
        private const val EXTRA_SCANNED = "com.monokulo.fob.extra.SCANNED"

        /** Opens a request Fob's scanner read: the pattern check first, the answer as a code. */
        fun openScanned(context: Context, text: String) {
            context.startActivity(Intent(context, ConnectActivity::class.java).putExtra(EXTRA_REQUEST, text).putExtra(EXTRA_SCANNED, true))
        }
    }
}

private enum class Step { CHECK, PICK, NEW, CONSENT, ANSWER }

/**
 * The flow over the screens: Check (scanned only), Choose a wallet, New wallet (for a new one), consent, and the
 * answer's code when it doesn't go back by itself ([returns] false). Choosing a wallet opens it, as Wallets does.
 */
@Composable
private fun ConnectFlow(
    model: WalletModel, text: String, request: ConnectRequest, icon: ImageBitmap?, verified: Boolean, scanned: Boolean,
    returns: Boolean, onAnswer: (String, Boolean) -> Unit, onClose: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val words = remember(request) { connectWords(request.app, request.device) }
    val steps = remember { mutableStateListOf(if (scanned) Step.CHECK else Step.PICK) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var wallets by remember { mutableStateOf<List<PickRow>>(emptyList()) }
    var picked by remember { mutableStateOf<String?>(null) }
    var walletName by remember { mutableStateOf("") }
    // A new wallet.
    var name by remember { mutableStateOf(request.name.ifBlank { request.app }) }
    var madeAs by remember { mutableStateOf<String?>(null) }
    var seed by remember { mutableStateOf<List<String>?>(null) }
    var written by remember { mutableStateOf(false) }
    // Consent.
    var label by remember { mutableStateOf(request.label) }
    var personal by remember { mutableStateOf(false) }
    var before by remember { mutableStateOf<ConnectedApp?>(null) }
    var useAgain by remember { mutableStateOf(false) }
    var shares by remember { mutableStateOf(ConnectShares(null, null)) }
    var shareNode by remember { mutableStateOf(true) }
    var sharePrices by remember { mutableStateOf(true) }
    var answer by remember { mutableStateOf<ConnectAnswer?>(null) }

    suspend fun readWallets() {
        wallets = runCatching { model.connectWallets(request.network).map(PickRow::of) }.getOrDefault(emptyList())
    }
    LaunchedEffect(Unit) { readWallets() }

    /** What consent shows of the wallet now open. */
    suspend fun readConsent() {
        val wallet = requireNotNull(model.wallet.value)
        walletName = model.current.value?.name ?: name
        shares = wallet.connectShares()
        before = wallet.connectedBefore(text)
        useAgain = before != null
        // Personal: money of its own in account 0, and no app connected yet.
        val apps = wallet.connectedApps()
        personal = apps.isEmpty() && wallet.accountActivity(0u).isNotEmpty()
    }

    fun go(step: Step) {
        error = null
        steps.add(step)
    }

    /** Not now: the app hears no, when the answer goes back to it; a scanned request just closes. */
    fun declineNow() {
        if (returns) runCatching { connectDeclined(text) }.onSuccess { onAnswer(it, false) }.onFailure { onClose() } else onClose()
    }

    fun back() {
        error = null
        if (steps.size > 1) steps.removeAt(steps.lastIndex) else declineNow()
    }

    BackHandler { if (steps.last() == Step.CHECK || steps.last() == Step.ANSWER) onClose() else back() }

    when (steps.last()) {
        Step.CHECK -> ConnectScanScreen(words, request.app, icon, request.identicon, onMatch = { go(Step.PICK) }, onNoMatch = onClose, onClose = onClose)
        Step.PICK -> ConnectPickScreen(
            words, request.app, label, icon, verified, wallets, picked, busy, error,
            onPick = { picked = it; error = null },
            onContinue = {
                val file = picked ?: return@ConnectPickScreen
                if (file == NEW_WALLET) { go(Step.NEW); return@ConnectPickScreen }
                busy = true
                scope.launch {
                    runCatching { model.openWallet(file); readConsent() }.onSuccess { go(Step.CONSENT) }.onFailure { error = it.reason() }
                    busy = false
                }
            },
            onNotNow = { declineNow() },
            onBack = { back() },
        )
        Step.NEW -> {
            if (seed != null) NoScreenshots()
            ConnectNewScreen(
                words, name, { name = it; error = null }, seed, written, { written = it }, busy, error,
                onShow = {
                    busy = true
                    error = null
                    scope.launch {
                        runCatching {
                            model.createOn(request.network, name.trim())
                            madeAs = name.trim()
                            // As bytes, zeroed as soon as they're words; the words live only as long as this screen.
                            seed = model.wallet.value?.seedPhrase()?.let { s -> String(s.phrase, Charsets.UTF_8).split(' ').also { s.phrase.fill(0) } }
                        }.onFailure { error = it.reason() }
                        busy = false
                    }
                },
                onMake = {
                    busy = true
                    scope.launch {
                        runCatching {
                            val file = model.current.value?.file
                            if (file != null && madeAs != null && name.trim() != madeAs) model.renameWallet(file, name.trim())
                            model.backupDone()
                            readConsent()
                        }.onSuccess {
                            seed = null
                            go(Step.CONSENT)
                        }.onFailure { error = it.reason() }
                        busy = false
                    }
                },
                onBack = {
                    // A wallet made here stays, in Choose a wallet's list; its words were shown.
                    seed = null
                    written = false
                    madeAs = null
                    scope.launch { readWallets() }
                    back()
                },
            )
        }
        Step.CONSENT -> ConnectConsentScreen(
            words, walletName, label, { label = it; error = null }, personal, before, useAgain, { useAgain = it },
            shares, request.wantsNode, request.wantsPrices, shareNode, { shareNode = it }, sharePrices, { sharePrices = it },
            busy, error,
            onGive = {
                busy = true
                scope.launch {
                    runCatching {
                        model.connectApp(text, label.trim(), if (useAgain) before?.account else null, shareNode && request.wantsNode, sharePrices && request.wantsPrices)
                    }.onSuccess { given ->
                        if (returns) onAnswer(given.answer, true) else { answer = given; go(Step.ANSWER) }
                    }.onFailure { error = it.reason() }
                    busy = false
                }
            },
            onNotNow = { declineNow() },
            onBack = { back() },
        )
        Step.ANSWER -> answer?.let { ConnectAnswerScreen(words, it.qr, request.identicon, onDone = onClose, onBack = null) }
    }
}
