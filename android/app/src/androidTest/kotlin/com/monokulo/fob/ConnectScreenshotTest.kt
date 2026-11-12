package com.monokulo.fob

import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.fob.ui.FobTheme
import com.monokulo.fob.ui.ThemeChoice
import com.monokulo.fob.ui.screens.ConnectAnswerScreen
import com.monokulo.fob.ui.screens.ConnectConsentScreen
import com.monokulo.fob.ui.screens.ConnectNewScreen
import com.monokulo.fob.ui.screens.ConnectPickScreen
import com.monokulo.fob.ui.screens.ConnectScanScreen
import com.monokulo.fob.ui.screens.ConnectedAppsContent
import com.monokulo.fob.ui.screens.PickRow
import org.junit.Rule
import org.junit.Test
import uniffi.fob.AccountRow
import uniffi.fob.ConnectShares
import uniffi.fob.ConnectedApp
import uniffi.fob.connectRequest
import uniffi.fob.connectWords
import uniffi.fob.connectedAppsWords
import uniffi.fob.qrCode
import java.io.File

/**
 * Connect to Fob's screens drawn from fixed data, one PNG each, to hold against the boards (VkFobPick, VkFobNew,
 * VkFobConsent, VkFobScan, VkFobAnswer) and Settings › Connected apps. scripts/screenshots.sh runs it on an
 * emulator and pulls the PNGs.
 */
class ConnectScreenshotTest {
    @get:Rule
    val compose = createComposeRule()

    /** Till on this phone asking, as Android's explicit intent brings it (with a callback). */
    private val sameDevice = connectRequest(
        "fob://connect?v=1&app=Till&name=Corner%20Caf%C3%A9&device=Pixel%208&net=mainnet&pk=B6N8vBQgk8i3VdwbEOhstCY3StFqqFPtC9_AsrhtHHw" +
            "&state=oKGio6SlpqeoqaqrrK2urw&want=view,node,prices&cb=till%3A%2F%2Ffob",
    )

    /** The iPad's request, scanned: no callback. */
    private val iPad = connectRequest(
        "fob://connect?v=1&app=Till&name=Corner%20Caf%C3%A9&device=Corner%20Caf%C3%A9%20iPad&net=mainnet&pk=B6N8vBQgk8i3VdwbEOhstCY3StFqqFPtC9_AsrhtHHw" +
            "&state=oKGio6SlpqeoqaqrrK2urw&want=view,node,prices",
    )

    private val wallets = listOf(
        PickRow(
            "cafe", "Corner Café",
            listOf("Front counter", "Back bar", "Kiosk", "Patio", "Bakery", "Window", "Market Sat", "Market Sun", "Events 1", "Events 2", "Van", "Spare iPad").map { "Till ($it)" },
            13u,
        ),
        PickRow("stall", "Market stall", listOf("Till (Stall)"), 2u),
        PickRow("popup", "Pop-up", listOf("Till (Pop-up front)", "Till (Pop-up back)", "Till (Pop-up phone)"), 4u),
        PickRow("mine", "My wallet", emptyList(), 1u),
    )

    private fun shot(name: String, content: @Composable () -> Unit) {
        compose.setContent { FobTheme(ThemeChoice.LIGHT) { content() } }
        compose.waitForIdle()
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(context.getExternalFilesDir(null), "screenshots").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private val words get() = connectWords(sameDevice.app, sameDevice.device)

    @Test
    fun vkFobPick() = shot("VkFobPick") {
        ConnectPickScreen(words, "Till", sameDevice.label, null, verified = true, wallets, picked = null, busy = false, error = null, {}, {}, {}, {})
    }

    @Test
    fun vkFobPickChosen() = shot("VkFobPick-chosen") {
        ConnectPickScreen(words, "Till", sameDevice.label, null, verified = true, wallets, picked = "cafe", busy = false, error = null, {}, {}, {}, {})
    }

    @Test
    fun vkFobNew() = shot("VkFobNew") {
        ConnectNewScreen(words, "Corner Café", {}, seed = null, written = false, {}, busy = false, error = null, {}, {}, {})
    }

    @Test
    fun vkFobConsent() = shot("VkFobConsent") {
        ConnectConsentScreen(
            words, "Corner Café", sameDevice.label, {}, personal = false, before = null, useAgain = false, {},
            ConnectShares("node.example.com:18089", "Kraken · AUD"), wantsNode = true, wantsPrices = true,
            shareNode = true, {}, sharePrices = true, {}, busy = false, error = null, {}, {}, {},
        )
    }

    @Test
    fun vkFobConsentPersonal() = shot("VkFobConsent-personal") {
        ConnectConsentScreen(
            words, "My wallet", sameDevice.label, {}, personal = true, before = null, useAgain = false, {},
            ConnectShares("node.example.com:18089", "Kraken · AUD"), wantsNode = true, wantsPrices = true,
            shareNode = true, {}, sharePrices = true, {}, busy = false, error = null, {}, {}, {},
        )
    }

    @Test
    fun vkFobScan() = shot("VkFobScan") {
        ConnectScanScreen(connectWords(iPad.app, iPad.device), iPad.app, null, iPad.identicon, {}, {}, {})
    }

    @Test
    fun vkFobAnswer() = shot("VkFobAnswer") {
        val answer = "fob://connected#v=1&state=oKGio6SlpqeoqaqrrK2urw&epk=OlU9dHktcn76m5pM3j2hrZPxotDAnLY5saPA_aFMviQ&box=" + "Dn4rD5fJKLkZhBTVvPAhTzYjBgbkKWzYlUizhJ8QLhP".repeat(8)
        ConnectAnswerScreen(connectWords(iPad.app, iPad.device), qrCode(answer), iPad.identicon, {}, null)
    }

    @Test
    fun vkFobApps() = shot("VkFobApps") {
        val now = 1_794_441_600uL // 12 Nov 2026
        val front = ConnectedApp(1u, 1u, "Till", "Corner Café", "Pixel 8", "Till (Corner Café)", now)
        val pad = ConnectedApp(2u, 2u, "Till", "Front counter", "Corner Café iPad", "Till (Front counter)", now - 86_400uL * 3u)
        ConnectedAppsContent(
            connectedAppsWords(),
            listOf(
                AccountRow(1u, "Till (Corner Café)", 310_000_000_000uL, 310_000_000_000uL, "0.31 XMR", "0.31 XMR", listOf(front)),
                AccountRow(2u, "Till (Front counter)", 0uL, 0uL, "0 XMR", "0 XMR", listOf(pad)),
            ),
            busy = false, error = null, {}, {}, {},
        )
    }
}
