package com.monokulo.fob

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import com.monokulo.fob.ui.FobTheme
import com.monokulo.fob.ui.screens.BackupScreen
import com.monokulo.fob.ui.screens.ConfirmScreen
import com.monokulo.fob.ui.screens.HomeScreen
import com.monokulo.fob.ui.screens.NodeScreen
import com.monokulo.fob.ui.screens.PaidScreen
import com.monokulo.fob.ui.screens.ReceiptScreen
import com.monokulo.fob.ui.screens.ReceiveScreen
import com.monokulo.fob.ui.screens.RestoreChooseScreen
import com.monokulo.fob.ui.screens.RestoreFoundScreen
import com.monokulo.fob.ui.screens.RestoreScanScreen
import com.monokulo.fob.ui.screens.RestoreSeedScreen
import com.monokulo.fob.ui.screens.RestoreViewOnlyScreen
import com.monokulo.fob.ui.screens.ScanPayScreen
import com.monokulo.fob.ui.screens.SendScreen
import com.monokulo.fob.ui.screens.SettingsScreen
import com.monokulo.fob.ui.screens.WalletCodeScreen
import com.monokulo.fob.ui.screens.WelcomeScreen
import kotlinx.coroutines.suspendCancellableCoroutine
import uniffi.fob.Activity
import uniffi.fob.Prepared
import kotlin.coroutines.resume

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
    data object Receive : Screen
    data object ScanPay : Screen
    /** The send form, filled from a scanned or linked payment code when there was one. */
    data class Send(val code: String? = null) : Screen
    data class Confirm(val prepared: Prepared) : Screen
    data class Paid(val prepared: Prepared, val tx: String) : Screen
    data class Receipt(val activity: Activity) : Screen
    data object Settings : Screen
    data object ShowSeed : Screen
    data object ShowWalletCode : Screen
    data object Node : Screen
}

class Nav(start: Screen) {
    val stack = mutableStateListOf(start)
    val current get() = stack.last()
    fun go(screen: Screen) { stack.add(screen) }
    fun back(): Boolean = if (stack.size > 1) { stack.removeAt(stack.lastIndex); true } else false
    /** Starts over at `screen`, forgetting the way here. */
    fun reset(screen: Screen) { stack.clear(); stack.add(screen) }
}

class MainActivity : FragmentActivity() {
    private val model: WalletModel get() = (application as FobApp).model
    private val link = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        link.value = intent?.dataString
        // For the catching-up notification; Fob works the same without it.
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 0)
        }
        val nav = Nav(if (model.hasWallet()) Screen.Home else Screen.Welcome)
        setContent {
            FobTheme {
                val pending by link
                LaunchedEffect(pending) {
                    val code = pending ?: return@LaunchedEffect
                    if (model.hasWallet()) nav.go(Screen.Send(code))
                    link.value = null
                }
                BackHandler(enabled = nav.stack.size > 1) { nav.back() }
                App(nav)
            }
        }
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
        super.onStop()
    }

    /** Fingerprint, face or screen lock before money leaves; passes on a phone with none set up. */
    suspend fun confirmIdentity(title: String): Boolean {
        val allowed = BIOMETRIC_STRONG or DEVICE_CREDENTIAL
        if (BiometricManager.from(this).canAuthenticate(allowed) != BiometricManager.BIOMETRIC_SUCCESS) return true
        return suspendCancellableCoroutine { done ->
            val prompt = BiometricPrompt(
                this, ContextCompat.getMainExecutor(this),
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                        if (done.isActive) done.resume(true)
                    }

                    override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                        if (done.isActive) done.resume(false)
                    }
                },
            )
            prompt.authenticate(BiometricPrompt.PromptInfo.Builder().setTitle(title).setAllowedAuthenticators(allowed).build())
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
            Screen.Home -> if (open || model.hasWallet()) HomeScreen(model, nav) else WelcomeScreen(model, nav)
            Screen.Receive -> ReceiveScreen(model, nav)
            Screen.ScanPay -> ScanPayScreen(model, nav)
            is Screen.Send -> SendScreen(model, nav, screen.code)
            is Screen.Confirm -> ConfirmScreen(model, nav, screen.prepared, ::confirmIdentity)
            is Screen.Paid -> PaidScreen(nav, screen.prepared)
            is Screen.Receipt -> ReceiptScreen(model, nav, screen.activity)
            Screen.Settings -> SettingsScreen(model, nav)
            Screen.ShowSeed -> BackupScreen(model, nav, afterCreate = false)
            Screen.ShowWalletCode -> WalletCodeScreen(model, nav)
            Screen.Node -> NodeScreen(model, nav)
        }
    }
}
