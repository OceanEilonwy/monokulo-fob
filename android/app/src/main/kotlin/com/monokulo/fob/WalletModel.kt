package com.monokulo.fob

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.fob.Activity
import uniffi.fob.Balance
import uniffi.fob.FobWallet
import uniffi.fob.Network
import uniffi.fob.Prepared
import uniffi.fob.SyncObserver
import uniffi.fob.SyncStatus
import java.io.File

/**
 * What the screens show, kept up to date from the Rust wallet. Everything
 * about the wallet happens in Rust; this only holds the latest answers and
 * passes calls through.
 */
class WalletModel(app: Application) : AndroidViewModel(app) {
    private val prefs = app.getSharedPreferences("fob", 0)
    private val vault = Vault(app)

    private val _network = MutableStateFlow(if (prefs.getString("network", null) == "mainnet") Network.MAINNET else Network.STAGENET)
    val network: StateFlow<Network> = _network

    private val _wallet = MutableStateFlow<FobWallet?>(null)
    val wallet: StateFlow<FobWallet?> = _wallet

    private val _balance = MutableStateFlow<Balance?>(null)
    val balance: StateFlow<Balance?> = _balance

    private val _activity = MutableStateFlow<List<Activity>>(emptyList())
    val activity: StateFlow<List<Activity>> = _activity

    private val _status = MutableStateFlow<SyncStatus?>(null)
    val status: StateFlow<SyncStatus?> = _status

    /** Set when the saved wallet couldn't be opened. */
    private val _openError = MutableStateFlow<String?>(null)
    val openError: StateFlow<String?> = _openError

    private var syncing = false

    private val observer = object : SyncObserver {
        override fun onStatus(status: SyncStatus) {
            _status.value = status
            viewModelScope.launch { refresh() }
        }
    }

    init {
        open()
    }

    private fun path(network: Network) = File(getApplication<Application>().filesDir, "wallet-${network.name.lowercase()}.fob").absolutePath

    fun hasWallet(network: Network = _network.value) = File(path(network)).exists()

    private fun open() {
        val network = _network.value
        if (!hasWallet(network)) return
        viewModelScope.launch {
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
        _activity.value = wallet.activity()
    }

    /** Syncs while the app is on screen. */
    fun startSync() {
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

    override fun onCleared() {
        _wallet.value?.let { it.stopSync(); it.close() }
    }
}
