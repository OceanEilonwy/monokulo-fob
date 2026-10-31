package com.monokulo.fob.ui.screens

import android.content.ClipData
import android.content.ClipDescription
import android.os.Build
import android.os.PersistableBundle
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.NoScreenshots
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.SectionLabel
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TopBar
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.fob.BackupHub
import uniffi.fob.HubItem
import uniffi.fob.HubRow
import uniffi.fob.KeyBadge
import uniffi.fob.KeyGroup
import uniffi.fob.KeyRow

/**
 * Backup and keys (board KeysHub), reached from Settings once the phone's
 * lock said it's you: the warning, Seed phrase and Wallet code under Back
 * up, Keys under For other wallets, every word from Rust. Its screens open
 * without asking again: the lock was asked on the way in.
 */
@Composable
fun BackupKeysScreen(model: WalletModel, nav: Nav) {
    NoScreenshots()
    val wallet by model.wallet.collectAsState()
    var hub by remember { mutableStateOf<BackupHub?>(null) }
    LaunchedEffect(wallet) { hub = wallet?.backupHub() }
    Page {
        TopBar("Backup and keys", onBack = { nav.back() })
        val h = hub ?: return@Page
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(Fob.highlight).padding(16.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                StrokeIcon(Icons.warning, tint = Fob.orangeText, size = 22.dp, width = 1.8f)
                Text(h.warning, fontSize = 14.sp, lineHeight = 20.sp)
            }
            HubCard("Back up", h.backup, nav)
            HubCard("For other wallets", h.other, nav)
            Text(h.footer, color = Fob.muted, fontSize = 13.sp, lineHeight = 19.sp, modifier = Modifier.padding(horizontal = 8.dp))
        }
    }
}

@Composable
private fun HubCard(title: String, rows: List<HubRow>, nav: Nav) {
    Column {
        SectionLabel(title)
        Card {
            rows.forEach { row ->
                val (icon, screen) = when (row.item) {
                    HubItem.SEED_PHRASE -> Icons.seedWords to Screen.ShowSeed
                    HubItem.WALLET_CODE -> Icons.request to Screen.ShowWalletCode
                    HubItem.KEYS -> Icons.key to Screen.Keys
                }
                LeadRow(icon, row.title, row.line, minHeight = 72.dp, gap = 14.dp) { nav.go(screen) }
            }
        }
    }
}

/**
 * Keys (board KeysDetail): every key in full at once, the secret ones
 * first, each with its badge, what it's for and a Copy button. The value
 * wraps anywhere, a 64-digit key over two or three lines.
 */
@Composable
fun KeysScreen(model: WalletModel, nav: Nav) {
    NoScreenshots()
    val wallet by model.wallet.collectAsState()
    var groups by remember { mutableStateOf<List<KeyGroup>>(emptyList()) }
    LaunchedEffect(wallet) { groups = wallet?.keysPage() ?: emptyList() }
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    // The key just copied, by title: its "Copied" shows for 2 s.
    var copied by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(copied) { if (copied != null) { delay(2_000); copied = null } }
    Page {
        TopBar("Keys", onBack = { nav.back() })
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            groups.forEach { group ->
                Column {
                    SectionLabel(group.title)
                    Card {
                        group.keys.forEach { key ->
                            KeyLine(key, copied = copied == key.title) {
                                scope.launch { clipboard.setClipEntry(sensitiveClip(key.title, key.clip).toClipEntry()) }
                                copied = key.title
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * A clip the keyboard's and the system's clipboard previews don't show
 * (Android 13 and later); earlier versions ignore the mark.
 */
private fun sensitiveClip(label: String, text: String): ClipData =
    ClipData.newPlainText(label, text).apply {
        val sensitive = if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE"
        description.extras = PersistableBundle().apply { putBoolean(sensitive, true) }
    }

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun KeyLine(key: KeyRow, copied: Boolean, onCopy: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(start = 20.dp, end = 4.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            // The badge goes under the title when both don't fit (a small phone, a big font), never squeezing it.
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp),
                itemVerticalAlignment = Alignment.CenterVertically,
            ) {
                Text(key.title, fontSize = 15.sp)
                Badge(key)
            }
            key.line?.let { Text(it, color = Fob.muted, fontSize = 12.5.sp) }
            // Hex and an address have no spaces: they break at any letter.
            Text(key.value, fontFamily = FontFamily.Monospace, fontSize = 12.5.sp, lineHeight = 18.sp)
            if (copied) {
                Text("Copied", color = Fob.green, fontSize = 12.5.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            }
        }
        Box(
            Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button, onClick = onCopy).semantics { contentDescription = key.copy },
            contentAlignment = Alignment.Center,
        ) { StrokeIcon(if (copied) Icons.check else Icons.copy, size = 18.dp, width = 1.8f) }
    }
}

/** What a key lets someone do, in a small pill: orange for spending, plain for seeing, muted for public. */
@Composable
private fun Badge(key: KeyRow) {
    val (fill: Color, ink: Color) = when (key.badge) {
        KeyBadge.SPENDS_MONEY -> Fob.addressMark to Fob.onAddressMark
        KeyBadge.SEES_PAYMENTS -> Fob.sunken to Fob.ink
        KeyBadge.PUBLIC -> Fob.sunken to Fob.muted
    }
    Text(
        key.badgeText, color = ink, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, maxLines = 1,
        modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(fill).padding(horizontal = 8.dp, vertical = 2.dp),
    )
}
