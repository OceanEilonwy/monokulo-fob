package com.monokulo.fob.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import androidx.compose.foundation.background
import androidx.compose.ui.semantics.heading
import com.monokulo.fob.ui.BottomSheet
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.SheetHandle
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.Icons
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.StrokeIcon
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.launch
import uniffi.fob.Network
import uniffi.fob.WalletEntry

/**
 * The wallets on this phone. Tapping one opens it in place of this one and
 * goes to its Home; only the open one syncs, so another catches up as it opens.
 */
@Composable
fun WalletsScreen(model: WalletModel, nav: Nav) {
    val wallets by model.wallets.collectAsState()
    val current by model.current.collectAsState()
    var renaming by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { model.readWallets() }
    Page {
        TopBar("Wallets", onBack = { nav.back() })
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Card {
                wallets.forEachIndexed { i, wallet ->
                    if (i > 0) Divider()
                    WalletRow(wallet, open = wallet.file == current?.file, onRename = { renaming = wallet.file }) {
                        if (wallet.file != current?.file) model.switchWallet(wallet.file)
                        nav.reset(Screen.Home)
                    }
                }
                if (wallets.isNotEmpty()) Divider()
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button) { nav.go(Screen.AddWallet) }.padding(horizontal = 20.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    StrokeIcon(Icons.plus, tint = Fob.orangeText, size = 20.dp)
                    Text("Add a wallet", color = Fob.orangeText, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                }
            }
        }
    }
    wallets.firstOrNull { it.file == renaming }?.let { wallet -> RenameDialog(model, wallet) { renaming = null } }
}

/**
 * Home's wallet switcher, a sheet over it: tapping a wallet opens it and
 * stays on Home; Manage is the Wallets page, where they're renamed.
 */
@Composable
fun WalletSheet(model: WalletModel, nav: Nav, onDismiss: () -> Unit) {
    val wallets by model.wallets.collectAsState()
    val current by model.current.collectAsState()
    LaunchedEffect(Unit) { model.readWallets() }
    BottomSheet("Wallets", onDismiss = onDismiss) {
        SheetHandle()
        Text("Wallets", fontSize = 20.sp, fontWeight = FontWeight.ExtraBold, modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp).semantics { heading() })
        Card {
            wallets.forEachIndexed { i, wallet ->
                if (i > 0) Divider()
                val open = wallet.file == current?.file
                WalletRow(wallet, open = open, onRename = null, highlight = open) {
                    if (!open) model.switchWallet(wallet.file)
                    onDismiss()
                }
            }
            if (wallets.isNotEmpty()) Divider()
            Row(
                Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button) { onDismiss(); nav.go(Screen.AddWallet) }.padding(horizontal = 20.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                StrokeIcon(Icons.plus, tint = Fob.orangeText, size = 20.dp)
                Text("Add a wallet", color = Fob.orangeText, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            TextButton("Manage", { onDismiss(); nav.go(Screen.Wallets) })
        }
    }
}

/** A wallet: a tick when open, its name and network, and Rename (not where [onRename] is null). */
@Composable
private fun WalletRow(wallet: WalletEntry, open: Boolean, onRename: (() -> Unit)?, highlight: Boolean = false, onOpen: () -> Unit) {
    val network = if (wallet.network == Network.MAINNET) "Mainnet" else "Stagenet, test coins"
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).then(if (highlight) Modifier.background(Fob.highlight) else Modifier).padding(end = if (onRename != null) 8.dp else 0.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.weight(1f).heightIn(min = 64.dp).clickable(role = Role.Button, onClickLabel = if (open) "Go to its home screen" else "Open", onClick = onOpen)
                .semantics(mergeDescendants = true) { selected = open }.padding(start = 20.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Box(Modifier.size(20.dp), contentAlignment = Alignment.Center) {
                if (open) StrokeIcon(Icons.check, tint = Fob.green, size = 20.dp, width = 2.6f)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(wallet.name, fontSize = 16.sp, fontWeight = if (open) FontWeight.SemiBold else FontWeight.Normal)
                Text(if (open) "$network · open" else network, fontSize = 13.sp, color = Fob.muted)
            }
        }
        if (onRename != null) Box(Modifier.semantics(mergeDescendants = true) { contentDescription = "Rename ${wallet.name}" }) { TextButton("Rename", onRename) }
    }
}

@Composable
private fun RenameDialog(model: WalletModel, wallet: WalletEntry, onDone: () -> Unit) {
    val scope = rememberCoroutineScope()
    var name by rememberSaveable { mutableStateOf(wallet.name) }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = onDone,
        title = { Text("Rename wallet") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Field(name, { name = it; error = null }, "Name")
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton("Save", {
                scope.launch {
                    runCatching { model.renameWallet(wallet.file, name) }
                        .onSuccess { onDone() }
                        .onFailure { error = it.reason() }
                }
            })
        },
        dismissButton = { TextButton("Cancel", onDone, color = Fob.ink) },
    )
}
