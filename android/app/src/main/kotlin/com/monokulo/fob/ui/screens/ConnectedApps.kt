package com.monokulo.fob.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.fob.Nav
import com.monokulo.fob.Screen
import com.monokulo.fob.WalletModel
import com.monokulo.fob.ui.Card
import com.monokulo.fob.ui.Divider
import com.monokulo.fob.ui.ErrorText
import com.monokulo.fob.ui.Fob
import com.monokulo.fob.ui.OutlineButton
import com.monokulo.fob.ui.Page
import com.monokulo.fob.ui.TextButton
import com.monokulo.fob.ui.TopBar
import com.monokulo.fob.ui.reason
import kotlinx.coroutines.launch
import uniffi.fob.AccountRow
import uniffi.fob.ConnectedApp
import uniffi.fob.ConnectedAppsWords
import uniffi.fob.connectedAccountLine
import uniffi.fob.connectedAppLine
import uniffi.fob.connectedAppsWords
import uniffi.fob.connectedForget
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Settings › Connected apps (decision 424): every app this wallet gave view access to, by its account, with what
 * the account holds, Sweep to main, and Forget, which says plainly that a view key can't be taken back.
 */
@Composable
fun ConnectedAppsScreen(model: WalletModel, nav: Nav) {
    val wallet by model.wallet.collectAsState()
    val scope = rememberCoroutineScope()
    val words = remember { connectedAppsWords() }
    var accounts by remember { mutableStateOf<List<AccountRow>?>(null) }
    var forgetting by remember { mutableStateOf<ConnectedApp?>(null) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    suspend fun read() { accounts = wallet?.accounts()?.filter { it.apps.isNotEmpty() } }
    LaunchedEffect(wallet) { read() }
    ConnectedAppsContent(
        words, accounts, busy, error,
        onSweep = { row ->
            busy = true
            error = null
            scope.launch {
                runCatching { model.prepareSweep(row.index) }
                    .onSuccess { nav.go(Screen.Confirm(prepared = it, note = row.label)) }
                    .onFailure { error = it.reason() }
                busy = false
            }
        },
        onForget = { forgetting = it },
        onBack = { nav.back() },
    )
    forgetting?.let { app ->
        val (title, text) = connectedForget(app.label ?: app.app, app.app)
        AlertDialog(
            onDismissRequest = { forgetting = null },
            title = { Text(title) },
            text = { Text(text) },
            confirmButton = {
                TextButton(words.forget, {
                    forgetting = null
                    scope.launch {
                        runCatching { model.forgetApp(app.id) }.onFailure { error = it.reason() }
                        read()
                    }
                }, color = Fob.red)
            },
            dismissButton = { TextButton(words.cancel, { forgetting = null }, color = Fob.ink) },
        )
    }
}

/** The list, from what it's given: for the screen and its screenshot. [accounts] null is still being read. */
@Composable
fun ConnectedAppsContent(
    words: ConnectedAppsWords, accounts: List<AccountRow>?, busy: Boolean, error: String?,
    onSweep: (AccountRow) -> Unit, onForget: (ConnectedApp) -> Unit, onBack: () -> Unit,
) {
    Page {
        TopBar(words.title, onBack = onBack)
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(words.intro, color = Fob.muted, fontSize = 15.sp, lineHeight = 21.sp, modifier = Modifier.padding(horizontal = 8.dp))
            ErrorText(error)
            val rows = accounts ?: return@Column
            if (rows.isEmpty()) Text(words.none, fontSize = 16.sp, modifier = Modifier.padding(horizontal = 8.dp))
            rows.forEach { row -> AccountCard(words, row, busy, onSweep, onForget) }
        }
    }
}

@Composable
private fun AccountCard(words: ConnectedAppsWords, row: AccountRow, busy: Boolean, onSweep: (AccountRow) -> Unit, onForget: (ConnectedApp) -> Unit) {
    val dates = remember { SimpleDateFormat("d MMM", Locale.getDefault()) }
    Card {
        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 14.dp, bottom = 10.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(row.label ?: row.apps.first().app, fontSize = 16.sp, fontWeight = FontWeight.Medium)
            Text(connectedAccountLine(row.index, row.totalText), color = Fob.muted, fontSize = 14.sp)
        }
        row.apps.forEach { app ->
            Divider()
            Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(start = 20.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    connectedAppLine(app.device, dates.format(Date(app.connected.toLong() * 1000))),
                    fontSize = 15.sp, modifier = Modifier.weight(1f),
                )
                TextButton(words.forget, { onForget(app) })
            }
        }
        // Account 0 is the main one; a sweep has nothing to move until something's ready to spend.
        if (row.index > 0u && row.unlocked > 0uL) {
            Divider()
            Box(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
                OutlineButton(words.sweep, { if (!busy) onSweep(row) }, Modifier.fillMaxWidth(), enabled = !busy, height = 48.dp)
            }
        }
    }
}
