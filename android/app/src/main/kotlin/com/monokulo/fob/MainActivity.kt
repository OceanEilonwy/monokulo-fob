package com.monokulo.fob

import android.Manifest
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.View
import android.view.ViewTreeObserver
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.toArgb
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import com.monokulo.fob.ui.FobTheme
import com.monokulo.fob.ui.LocalBottomSpace
import com.monokulo.fob.ui.LocalFobColors
import com.monokulo.fob.ui.PriceToastHost
import com.monokulo.fob.ui.screens.BackupKeysScreen
import com.monokulo.fob.ui.screens.BackupScreen
import com.monokulo.fob.ui.screens.CurrencyListScreen
import com.monokulo.fob.ui.screens.CoinsScreen
import com.monokulo.fob.ui.screens.ConfirmScreen
import com.monokulo.fob.ui.screens.ContactEditScreen
import com.monokulo.fob.ui.screens.ContactScanScreen
import com.monokulo.fob.ui.screens.ContactScreen
import com.monokulo.fob.ui.screens.ContactsScreen
import com.monokulo.fob.ui.screens.HomeScreen
import com.monokulo.fob.ui.screens.KeysScreen
import com.monokulo.fob.ui.screens.NodeScreen
import com.monokulo.fob.ui.screens.PaidScreen
import com.monokulo.fob.ui.screens.PaymentScreen
import com.monokulo.fob.ui.screens.PriceFromScreen
import com.monokulo.fob.ui.screens.ProfileEditScreen
import com.monokulo.fob.ui.screens.ProfileScanScreen
import com.monokulo.fob.ui.screens.ProfileScreen
import com.monokulo.fob.ui.screens.ProfileShareScreen
import com.monokulo.fob.ui.screens.ReceiptScreen
import com.monokulo.fob.ui.screens.ReceiveDraft
import com.monokulo.fob.ui.screens.ReceiveScreen
import com.monokulo.fob.ui.screens.RequestScreen
import com.monokulo.fob.ui.screens.RestoreChooseScreen
import com.monokulo.fob.ui.screens.RestoreFoundScreen
import com.monokulo.fob.ui.screens.RestoreScanScreen
import com.monokulo.fob.ui.screens.RestoreSeedScreen
import com.monokulo.fob.ui.screens.RestoreViewOnlyScreen
import com.monokulo.fob.ui.screens.SendScreen
import com.monokulo.fob.ui.screens.SettingsScreen
import com.monokulo.fob.ui.screens.SplitScreen
import com.monokulo.fob.ui.screens.WalletCodeScreen
import com.monokulo.fob.ui.screens.WelcomeScreen
import com.monokulo.fob.ui.screens.ZmqScreen
import kotlinx.coroutines.CompletableDeferred
import com.monokulo.fob.ui.screens.PendingSend
import com.monokulo.fob.ui.screens.SendDraft
import uniffi.fob.Activity
import uniffi.fob.ContactDraft
import uniffi.fob.Prepared

/** Where the app is. Each screen is a value; the back stack is a list of them. */
sealed interface Screen {
    data object Welcome : Screen
    data object Backup : Screen
    data object RestoreChoose : Screen
    data object RestoreSeed : Screen
    data object RestoreViewOnly : Screen
    data object RestoreScan : Screen
    data class RestoreFound(val code: String) : Screen
    data object Home : Screen
    /**
     * Receive, for [contact] (chosen already, from their card's Request) or
     * no one yet; [draft] fills the form as it was (back from New contact).
     */
    data class Receive(val contact: ULong? = null, val draft: ReceiveDraft? = null) : Screen
    /** A request: what it asked, what came, Show the code again, Delete. */
    data class Request(val id: ULong) : Screen
    /**
     * Send: the camera under a sheet with the form; [code], from a link, is
     * read as if scanned. With [draft], the form opens filled, as it was
     * (back from Confirm).
     */
    data class Send(val code: String? = null, val draft: SendDraft? = null) : Screen
    /**
     * [note]: typed on Send, or the payment code's description, shown again before sending. Opened from Send with
     * [pending] and no [prepared]: what Rust checked shows at once, and the screen becomes the prepared one when the
     * fee is in.
     */
    data class Confirm(val prepared: Prepared? = null, val note: String? = null, val pending: PendingSend? = null) : Screen
    data class Paid(val prepared: Prepared, val tx: String) : Screen
    data class Receipt(val activity: Activity) : Screen
    /** A payment without a receipt. */
    data class Payment(val activity: Activity) : Screen
    /** Every coin, ready or not, with Split up at the bottom. */
    data object Coins : Screen
    /** Split up: what's ready to spend paid back to this wallet as pieces. */
    data object Split : Screen
    data object Settings : Screen
    /** Price from: each provider, checked as it opens; from Settings or the price toast's cog. */
    data object PriceFrom : Screen
    /** "Default currency": every currency, with a search. */
    data object CurrencyList : Screen
    /**
     * Backup and keys: Seed phrase, Wallet code and Keys, opened from
     * Settings once the phone's lock says it's you; its screens don't ask
     * again while Fob stays on screen.
     */
    data object BackupKeys : Screen
    /** Every key in full, each with Copy; from Backup and keys. */
    data object Keys : Screen
    /** Everyone this wallet pays, and who was paid but isn't saved. */
    data object Contacts : Screen
    /** A contact's card. */
    data class Contact(val id: ULong) : Screen
    /**
     * The edit card: contact [id], or a new one; [prefill] fills a new one.
     * [forRequest]: New contact from Receive's sheet, which goes back to
     * Receive with it chosen.
     */
    data class ContactEdit(val id: ULong? = null, val prefill: ContactDraft? = null, val forRequest: Boolean = false) : Screen
    /** Scanning a contact's send address, from its card's Change. */
    data class ContactScan(val id: ULong) : Screen
    /** Scanning another Fob's profile, from Contacts. */
    data object ProfileScan : Screen
    /** Your profile: the name and photo, Share my profile, the switch and every share. */
    data object Profile : Screen
    /**
     * The profile's editor: [step] ("Step 3 of 3 · optional") as the last
     * step of a new or restored wallet, with Skip; Edit without one.
     */
    data class ProfileEdit(val step: String? = null) : Screen
    /** A share's code, full screen: just made by Share my profile, or again. */
    data class ProfileShare(val id: ULong, val fresh: Boolean = false) : Screen
    /**
     * The seed phrase, from Backup and keys or Home's backup reminder: with
     * the warning to write them down while they aren't yet.
     */
    data object ShowSeed : Screen
    data object ShowWalletCode : Screen
    data object Node : Screen
    data object Zmq : Screen
}

class Nav(start: Screen) {
    val stack = mutableStateListOf(start)
    val current get() = stack.last()
    fun go(screen: Screen) { stack.add(screen) }
    fun back(): Boolean = if (stack.size > 1) { stack.removeAt(stack.lastIndex); true } else false
    /** Puts `screen` in place of the current one. */
    fun replace(screen: Screen) { stack[stack.lastIndex] = screen }
    /** Starts over at `screen`, forgetting the way here. */
    fun reset(screen: Screen) { stack.clear(); stack.add(screen) }
    /**
     * Set by a screen with a form (Send, Receive): puts it on the stack as
     * filled, before something over every screen (the price toast's cog)
     * opens another on top, so Back finds it as it was.
     */
    var keepForm: (() -> Unit)? = null
}

/**
 * Keeps the [Nav] while the activity is made again for a change the
 * manifest's `configChanges` doesn't cover (font size, language), so Fob
 * comes back where it was, not at Home.
 */
class NavHolder : ViewModel() {
    var nav: Nav? = null
    /** The answer to a phone-lock prompt still up, kept likewise: it reaches the Confirm that comes back. */
    var identity: CompletableDeferred<Boolean>? = null
    /** Where a phone-lock prompt for Backup and keys leads once it says it's you. */
    var lockedTarget: Screen? = null
}

/**
 * A screen lock (PIN, pattern, password) is set on this phone: Backup and
 * keys and sending ask for it (or a fingerprint or face) only then.
 */
fun phoneLock(context: Context): Boolean =
    context.getSystemService(KeyguardManager::class.java)?.isDeviceSecure ?: true

/**
 * Backup and keys and what it leads to (shown only after the phone's lock),
 * and a new wallet's words: left when Fob leaves the screen. A new wallet's
 * come back from Home's reminder, behind the lock.
 */
private fun locked(screen: Screen) =
    screen == Screen.ShowSeed || screen == Screen.BackupKeys || screen == Screen.Keys || screen == Screen.ShowWalletCode || screen == Screen.Backup

/** The longest the launch screen waits for the saved wallet to open. */
private const val HOLD_MS = 2_500L

class MainActivity : FragmentActivity() {
    private val model: WalletModel get() = (application as FobApp).model
    private val link = mutableStateOf<String?>(null)
    private lateinit var holder: NavHolder
    /** Made here, as androidx.biometric asks, so a prompt up while the activity is made again still answers. */
    private lateinit var prompt: BiometricPrompt

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        holder = ViewModelProvider(this)[NavHolder::class.java]
        prompt = BiometricPrompt(
            this, ContextCompat.getMainExecutor(this),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    holder.identity?.complete(true)
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    holder.identity?.complete(false)
                }
            },
        )
        // A link is read once, not again when the activity is made again.
        if (savedInstanceState == null) link.value = intent?.dataString
        // For the catching-up notification; Fob works the same without it.
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 0)
        }
        val nav = holder.nav ?: Nav(if (model.hasWallet()) Screen.Home else Screen.Welcome).also { holder.nav = it }
        // Made again while the phone's lock was asked for Backup and keys: its answer still leads there.
        holder.lockedTarget?.let { target ->
            holder.lockedTarget = null
            if (identityPending()) openLocked(target)
        }
        holdWhileOpening()
        setContent {
            val theme by model.theme.collectAsState()
            FobTheme(theme) {
                SystemBars()
                val pending by link
                LaunchedEffect(pending) {
                    val code = pending ?: return@LaunchedEffect
                    if (model.hasWallet()) nav.go(Screen.Send(code))
                    link.value = null
                }
                BackHandler(enabled = nav.stack.size > 1) { nav.back() }
                // The price toast over whatever screen is open, above its bottom bar.
                val bottomSpace = remember { mutableStateOf(0.dp) }
                CompositionLocalProvider(LocalBottomSpace provides bottomSpace) {
                    Box(Modifier.fillMaxSize()) {
                        App(nav)
                        val toast by model.priceToast.collectAsState()
                        val notice by model.notice.collectAsState()
                        PriceToastHost(
                            toast, onRetry = model::retryPrices, onDismiss = model::dismissPriceToast,
                            onSettings = {
                                if (nav.current != Screen.PriceFrom) {
                                    nav.keepForm?.invoke()
                                    nav.go(Screen.PriceFrom)
                                }
                            },
                            notice = notice, onNoticeDismiss = model::dismissNotice,
                            // Confirm cancels its payment as it goes, sending or not: no Price from over it.
                            settingsEnabled = nav.current !is Screen.Confirm,
                        )
                    }
                }
            }
        }
    }

    /**
     * Keeps the launch screen up while the saved wallet opens, so Home's
     * first frame has its balance, photo and reminder rather than a dash
     * that fills in and pushes what's under it a moment later. Android's
     * own way (a first draw held back), so no splash library; never longer
     * than [HOLD_MS], a slow phone then seeing Home fill in.
     */
    private fun holdWhileOpening() {
        val content = findViewById<View>(android.R.id.content)
        val started = SystemClock.uptimeMillis()
        content.viewTreeObserver.addOnPreDrawListener(object : ViewTreeObserver.OnPreDrawListener {
            override fun onPreDraw(): Boolean {
                if (model.opening && SystemClock.uptimeMillis() - started < HOLD_MS) return false
                content.viewTreeObserver.removeOnPreDrawListener(this)
                return true
            }
        })
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        link.value = intent.dataString
    }

    override fun onStart() {
        super.onStart()
        // Started while Fob is on screen (Android doesn't allow it from the
        // background) when it's far behind; it stops itself once caught up.
        model.onFarBehind = { SyncService.start(this) }
        model.setVisible(true)
    }

    override fun onStop() {
        model.setVisible(false)
        model.onFarBehind = null
        // Leaving Fob (home, another app, the screen off) closes Backup and
        // keys, so coming back asks for the phone's lock again; turning the
        // phone or a font change doesn't.
        // Everything above the first of them goes too (the price toast's cog
        // can open Price from over Keys), so Back can't lead to one.
        if (!isChangingConfigurations) {
            holder.nav?.stack?.let { stack ->
                val first = stack.indexOfFirst(::locked)
                if (first >= 1) while (stack.size > first) stack.removeAt(stack.lastIndex)
            }
            // A secret key copied from Keys comes off the clipboard with it.
            SecretClip.clear()
        }
        super.onStop()
    }

    /**
     * Opens [screen] (Backup and keys, or the seed phrase from Home's
     * reminder) once the phone's lock says it's you; cancelling stays put,
     * and a second tap while asking does nothing.
     */
    fun openLocked(screen: Screen) {
        if (holder.lockedTarget != null) return
        holder.lockedTarget = screen
        lifecycleScope.launch {
            val ok = confirmIdentity("Backup and keys")
            holder.lockedTarget = null
            if (ok) holder.nav?.go(screen)
        }
    }

    /**
     * Fingerprint, face or screen lock before money leaves or Backup and
     * keys opens; passes only on a phone with no screen lock set. A prompt
     * already up (the activity made again under it) is waited for, not
     * asked twice. Anything else that stops the prompt (the sensor busy, a
     * security update needed) refuses, with a word why.
     */
    suspend fun confirmIdentity(title: String): Boolean {
        // androidx.biometric 1.1.0 takes a strong biometric with the screen
        // lock everywhere but Android 9 and 10, which take only a weak one
        // with it (their prompt has no strength classes).
        val allowed = (if (Build.VERSION.SDK_INT in 28..29) BIOMETRIC_WEAK else BIOMETRIC_STRONG) or DEVICE_CREDENTIAL
        holder.identity?.takeIf { it.isActive }?.let { return it.await() }
        if (!phoneLock(this)) return true
        if (BiometricManager.from(this).canAuthenticate(allowed) != BiometricManager.BIOMETRIC_SUCCESS) {
            Toast.makeText(this, "Fob couldn't ask for your phone's lock just now, so it stopped. Try again in a moment.", Toast.LENGTH_LONG).show()
            return false
        }
        val asked = CompletableDeferred<Boolean>().also { holder.identity = it }
        prompt.authenticate(BiometricPrompt.PromptInfo.Builder().setTitle(title).setAllowedAuthenticators(allowed).build())
        return asked.await()
    }

    /** A phone-lock prompt is up, asked before the activity was made again. */
    fun identityPending(): Boolean = holder.identity?.isActive == true

    /** Status and navigation bar icons dark on light paper, light on dark. */
    @Composable
    private fun SystemBars() {
        val colors = LocalFobColors.current
        val paper = colors.paper.toArgb()
        DisposableEffect(colors.dark) {
            val clear = android.graphics.Color.TRANSPARENT
            val style = if (colors.dark) SystemBarStyle.dark(clear) else SystemBarStyle.light(clear, clear)
            enableEdgeToEdge(statusBarStyle = style, navigationBarStyle = style)
            window.setBackgroundDrawable(ColorDrawable(paper))
            onDispose {}
        }
    }

    @Composable
    private fun App(nav: Nav) {
        val wallet by model.wallet.collectAsState()
        // Each entry on the stack keeps its own state: two Send screens in a
        // row (one empty, one from a link) mustn't share text fields.
        val screen = nav.current
        key(nav.stack.size, screen) { Route(nav, screen, wallet != null) }
    }

    @Composable
    private fun Route(nav: Nav, screen: Screen, open: Boolean) {
        when (screen) {
            Screen.Welcome -> WelcomeScreen(model, nav)
            Screen.Backup -> BackupScreen(model, nav, afterCreate = true)
            Screen.RestoreChoose -> RestoreChooseScreen(nav)
            Screen.RestoreSeed -> RestoreSeedScreen(model, nav)
            Screen.RestoreViewOnly -> RestoreViewOnlyScreen(model, nav)
            Screen.RestoreScan -> RestoreScanScreen(model, nav)
            is Screen.RestoreFound -> RestoreFoundScreen(model, nav, screen.code)
            Screen.Home -> if (open || model.hasWallet()) HomeScreen(model, nav, ::openLocked) else WelcomeScreen(model, nav)
            is Screen.Receive -> ReceiveScreen(model, nav, screen)
            is Screen.Request -> RequestScreen(model, nav, screen.id)
            is Screen.Send -> SendScreen(model, nav, screen.code, screen.draft)
            is Screen.Confirm -> ConfirmScreen(model, nav, screen, ::confirmIdentity, ::identityPending)
            is Screen.Paid -> PaidScreen(model, nav, screen.prepared, screen.tx)
            is Screen.Receipt -> ReceiptScreen(model, nav, screen.activity)
            is Screen.Payment -> PaymentScreen(model, nav, screen.activity)
            Screen.Coins -> CoinsScreen(model, nav)
            Screen.Split -> SplitScreen(model, nav)
            Screen.Settings -> SettingsScreen(model, nav, ::openLocked)
            Screen.PriceFrom -> PriceFromScreen(model, nav)
            Screen.CurrencyList -> CurrencyListScreen(model, nav)
            Screen.BackupKeys -> BackupKeysScreen(model, nav)
            Screen.Keys -> KeysScreen(model, nav)
            Screen.Contacts -> ContactsScreen(model, nav)
            is Screen.Contact -> ContactScreen(model, nav, screen.id)
            is Screen.ContactEdit -> ContactEditScreen(model, nav, screen.id, screen.prefill, screen.forRequest)
            is Screen.ContactScan -> ContactScanScreen(model, nav, screen.id)
            Screen.ProfileScan -> ProfileScanScreen(model, nav)
            Screen.Profile -> ProfileScreen(model, nav)
            is Screen.ProfileEdit -> ProfileEditScreen(model, nav, screen.step)
            is Screen.ProfileShare -> ProfileShareScreen(model, nav, screen.id, screen.fresh)
            Screen.ShowSeed -> BackupScreen(model, nav, afterCreate = false)
            Screen.ShowWalletCode -> WalletCodeScreen(model, nav)
            Screen.Node -> NodeScreen(model, nav)
            Screen.Zmq -> ZmqScreen(model, nav)
        }
    }
}
