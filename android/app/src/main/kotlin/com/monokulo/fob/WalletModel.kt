package com.monokulo.fob

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import android.text.format.DateFormat
import com.monokulo.fob.ui.ThemeChoice
import uniffi.fob.ActivityDay
import uniffi.fob.Balance
import uniffi.fob.FobWallet
import uniffi.fob.Network
import uniffi.fob.NotReady
import uniffi.fob.Prepared
import uniffi.fob.SyncObserver
import uniffi.fob.SyncStatus
import java.io.File
import java.util.TimeZone

/**
 * What the screens show, kept up to date from the Rust wallet. Everything
 * about the wallet happens in Rust; this only holds the latest answers and
 * passes calls through.
 *
 * One per process, owned by [FobApp]: the screens and [SyncService] share
 * it, so the wallet file is only ever open once.
 */
class WalletModel(private val app: Application) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val prefs = app.getSharedPreferences("fob", 0)
    private val vault = Vault(app)

    private val _network = MutableStateFlow(if (prefs.getString("network", null) == "mainnet") Network.MAINNET else Network.STAGENET)
    val network: StateFlow<Network> = _network

    private val _wallet = MutableStateFlow<FobWallet?>(null)
    val wallet: StateFlow<FobWallet?> = _wallet

    private val _balance = MutableStateFlow<Balance?>(null)
    val balance: StateFlow<Balance?> = _balance

    /** Activity grouped by the phone's days, ready to draw. */
    private val _activity = MutableStateFlow<List<ActivityDay>>(emptyList())
    val activity: StateFlow<List<ActivityDay>> = _activity

    /** The money not ready to spend yet, piece by piece; null when all of it is. */
    private val _notReady = MutableStateFlow<NotReady?>(null)
    val notReady: StateFlow<NotReady?> = _notReady

    /**
     * Light, dark or as the phone is. Only how Fob looks, the same for every
     * wallet, so it's an app preference rather than in the wallet file.
     */
    private val _theme = MutableStateFlow(ThemeChoice.entries.firstOrNull { it.name == prefs.getString("theme", null) } ?: ThemeChoice.PHONE)
    val theme: StateFlow<ThemeChoice> = _theme

    fun setTheme(choice: ThemeChoice) {
        prefs.edit().putString("theme", choice.name).apply()
        _theme.value = choice
    }

    private val _status = MutableStateFlow<SyncStatus?>(null)
    val status: StateFlow<SyncStatus?> = _status

    /** Set when the saved wallet couldn't be opened. */
    private val _openError = MutableStateFlow<String?>(null)
    val openError: StateFlow<String?> = _openError

    private var syncing = false

    /** Whether Fob is on screen. Off screen, syncing stops once caught up. */
    var visible = false
        private set

    private val observer = object : SyncObserver {
        override fun onStatus(status: SyncStatus) {
            scope.launch {
                _status.value = status
                refresh()
                if (status.synced && !visible) stopSync()
                // Far behind while on screen: hold the process open to finish.
                if (!status.synced && visible && status.tip > status.scanned + FAR_BEHIND) onFarBehind?.invoke()
            }
        }
    }

    init {
        open()
    }

    private fun path(network: Network) = File(app.filesDir, "wallet-${network.name.lowercase()}.fob").absolutePath

    fun hasWallet(network: Network = _network.value) = File(path(network)).exists()

    private fun open() {
        val network = _network.value
        if (!hasWallet(network)) return
        scope.launch {
            runCatching { withContext(Dispatchers.IO) { FobWallet.open(path(network), vault.fileKey()) } }
                .onSuccess { use(it) }
                .onFailure { _openError.value = it.message }
        }
    }

    private suspend fun use(wallet: FobWallet) {
        _wallet.value?.let { it.stopSync(); it.close() }
        _wallet.value = wallet
        _status.value = wallet.status()
        refresh()
        if (syncing) wallet.startSync(observer)
    }

    suspend fun refresh() {
        val wallet = _wallet.value ?: return
        _balance.value = wallet.balance()
        val now = System.currentTimeMillis()
        val (seconds, offset, hours24) = Triple((now / 1000).toULong(), TimeZone.getDefault().getOffset(now) / 1000, DateFormat.is24HourFormat(app))
        _activity.value = wallet.activityDays(seconds, offset, hours24)
        _notReady.value = wallet.notReady(seconds, offset, hours24)
    }

    /** Pull to refresh: asks the node now, and returns once that round is over. */
    suspend fun syncNow() {
        val wallet = _wallet.value ?: return
        wallet.syncNow()
        refresh()
    }

    /** Talks to another node; on another host, instant updates go off. Throws what Rust refused. */
    suspend fun setNode(url: String) {
        val wallet = _wallet.value ?: return
        wallet.setNode(url)
        _status.value = wallet.status()
    }

    /** Instant updates (ZMQ) on or off; [endpoint], if given, replaces the one set. Throws what Rust refused. */
    suspend fun setZmq(on: Boolean, endpoint: String? = null) {
        val wallet = _wallet.value ?: return
        wallet.setZmq(on, endpoint)
        _status.value = wallet.status()
    }

    /**
     * Fob came on screen or went off it. On screen it syncs; off screen it
     * keeps going only while catching up, which [SyncService] holds open.
     */
    fun setVisible(visible: Boolean) {
        this.visible = visible
        if (visible) startSync() else if (_status.value?.synced != false) stopSync()
    }

    /** Told when Fob falls more than a day behind while on screen. */
    var onFarBehind: (() -> Unit)? = null

    /** Behind the chain: worth keeping the process alive to catch up. */
    val catchingUp get() = _wallet.value != null && _status.value?.synced != true

    fun startSync() {
        if (syncing) return
        syncing = true
        _wallet.value?.startSync(observer)
    }

    fun stopSync() {
        syncing = false
        _wallet.value?.stopSync()
    }

    private suspend fun adopt(make: (String, ByteArray, Network) -> FobWallet) {
        val network = _network.value
        val wallet = withContext(Dispatchers.IO) { make(path(network), vault.fileKey(), network) }
        use(wallet)
    }

    suspend fun create() = adopt { path, key, network -> FobWallet.create(path, key, network) }

    suspend fun restoreWords(words: String, restorePoint: String?) =
        adopt { path, key, network -> FobWallet.restoreWords(path, key, network, words, restorePoint) }

    suspend fun restoreCode(code: String, restorePoint: String?) =
        adopt { path, key, network -> FobWallet.restoreCode(path, key, network, code, restorePoint) }

    suspend fun restoreViewOnly(address: String, viewKey: String, restorePoint: String?) =
        adopt { path, key, network -> FobWallet.restoreViewOnly(path, key, network, address, viewKey, restorePoint) }

    /** Moves to the other network's wallet, if there is one. */
    fun switchNetwork(network: Network) {
        if (network == _network.value) return
        _wallet.value?.let { it.stopSync(); it.close() }
        _wallet.value = null
        _balance.value = null
        _activity.value = emptyList()
        _notReady.value = null
        _status.value = null
        prefs.edit().putString("network", network.name.lowercase()).apply()
        _network.value = network
        open()
    }

    /** Removes this network's wallet from the phone. Its seed restores it. */
    fun forget() {
        val network = _network.value
        _wallet.value?.let { it.stopSync(); it.close() }
        _wallet.value = null
        _balance.value = null
        _activity.value = emptyList()
        _notReady.value = null
        _status.value = null
        File(path(network)).delete()
    }

    suspend fun prepare(to: String, amount: String, receiptJson: String?): Prepared =
        requireNotNull(_wallet.value).prepare(to, amount, receiptJson)

    suspend fun send(prepared: Prepared): String {
        val hash = requireNotNull(_wallet.value).send(prepared.id)
        refresh()
        return hash
    }


    private companion object {
        /** About a day of blocks. */
        const val FAR_BEHIND = 720u
    }
}
