package com.monokulo.fob

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import android.text.format.DateFormat
import com.monokulo.fob.ui.PriceToastState
import com.monokulo.fob.ui.ThemeChoice
import uniffi.fob.ActivityDay
import uniffi.fob.AmountEntry
import uniffi.fob.Balance
import uniffi.fob.Coins
import uniffi.fob.ContactCard
import uniffi.fob.ContactRow
import uniffi.fob.FobWallet
import uniffi.fob.Network
import uniffi.fob.MadeRequest
import uniffi.fob.NotReady
import uniffi.fob.PaidBefore
import uniffi.fob.RequestDetail
import uniffi.fob.PaymentDetail
import uniffi.fob.CurrencyList
import uniffi.fob.PriceCheck
import uniffi.fob.PriceProvider
import uniffi.fob.PriceRetry
import uniffi.fob.PriceSettings
import uniffi.fob.PriceTransport
import uniffi.fob.SaveOffer
import uniffi.fob.Prepared
import uniffi.fob.Profile
import uniffi.fob.ProfileRow
import uniffi.fob.ProfileShare
import uniffi.fob.ShareLine
import uniffi.fob.Swapped
import uniffi.fob.TouchSwap
import uniffi.fob.SyncObserver
import uniffi.fob.SyncStatus
import uniffi.fob.SyncDetails
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

    /** How prices are shown, kept in the wallet file by Rust. */
    private val _prices = MutableStateFlow<PriceSettings?>(null)
    val prices: StateFlow<PriceSettings?> = _prices

    /**
     * The price toast, over every screen while price lookups fail. Rust
     * says when (a new toast id) and in what words; this holds what's
     * drawn and runs Retry.
     */
    private val _priceToast = MutableStateFlow<PriceToastState?>(null)
    val priceToast: StateFlow<PriceToastState?> = _priceToast

    /** The last toast id swiped away or retried: not shown again; Rust gives a new id when it should be. */
    private var toastDone = 0uL

    /**
     * A short note at the bottom of the screen, Rust's words when a price
     * choice moved something else ("Kraken doesn't price New Zealand
     * dollars, so Fob now asks CoinGecko"); gone after a few seconds.
     */
    private val _notice = MutableStateFlow<String?>(null)
    val notice: StateFlow<String?> = _notice

    private fun say(text: String?) {
        text ?: return
        _notice.value = text
        scope.launch {
            delay(4_000)
            if (_notice.value === text) _notice.value = null
        }
    }

    fun dismissNotice() {
        _notice.value = null
    }

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

    /**
     * Your profile and Settings' words for it, for Home's top bar: read
     * when a wallet opens and again whenever it's saved, so Home has it at
     * once on coming back from editing it.
     */
    private val _you = MutableStateFlow<You?>(null)
    val you: StateFlow<You?> = _you

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
        _wallet.value?.let { close(it) }
        _wallet.value = wallet
        _status.value = wallet.status()
        _you.value = You(wallet.profile(), wallet.profileRow())
        // Toast ids start again with each wallet opened.
        toastDone = 0uL
        _priceToast.value = null
        refresh()
        if (syncing) wallet.startSync(observer)
        if (visible) wallet.startPrices(observer)
    }

    private fun close(wallet: FobWallet) {
        wallet.stopSync()
        wallet.stopPrices()
        wallet.close()
    }

    suspend fun refresh() {
        val wallet = _wallet.value ?: return
        val (seconds, offset, hours24) = clock()
        _prices.value = wallet.priceSettings(seconds, offset, hours24)
        _balance.value = wallet.balance(seconds)
        _activity.value = wallet.activityDays(seconds, offset, hours24)
        _notReady.value = wallet.notReady(seconds, offset, hours24)
        refreshToast(wallet)
    }

    /**
     * The toast as Rust has it now: a new id shows (or comes back after a
     * swipe), the same id only takes newer words, none takes it down (a
     * lookup worked). Retry's own words aren't cut short.
     */
    private suspend fun refreshToast(wallet: FobWallet) {
        val (seconds, offset, hours24) = clock()
        val toast = wallet.priceToast(seconds, offset, hours24)?.takeIf { it.id > toastDone }
        when (_priceToast.value) {
            is PriceToastState.Retrying -> return
            is PriceToastState.Updated -> if (toast == null) return
            else -> {}
        }
        _priceToast.value = toast?.let { PriceToastState.Failed(it) }
    }

    /** The toast swiped away: gone until Rust shows another. */
    fun dismissPriceToast() {
        (_priceToast.value as? PriceToastState.Failed)?.let { toastDone = it.toast.id }
        (_priceToast.value as? PriceToastState.Retrying)?.let { toastDone = it.toast.id }
        _priceToast.value = null
    }

    /**
     * The toast's Retry, or Settings' Try again (with the toast swiped away
     * or not): "Trying again…" in the toast, then the price it got for 2 s,
     * or the failure again.
     */
    fun retryPrices() {
        val wallet = _wallet.value ?: return
        if (_priceToast.value is PriceToastState.Retrying) return
        scope.launch {
            val (seconds, offset, hours24) = clock()
            val failed = (_priceToast.value as? PriceToastState.Failed)?.toast
                ?: wallet.priceToast(seconds, offset, hours24) ?: return@launch
            toastDone = failed.id
            val asking = PriceToastState.Retrying(failed)
            _priceToast.value = asking
            val started = System.currentTimeMillis()
            val result = wallet.retryPrices(seconds, offset, hours24)
            // Offline, the answer is back at once: "Trying again…" stays long
            // enough to be seen, so Retry visibly did something.
            delay(RETRY_SHOWN_MS - (System.currentTimeMillis() - started))
            // Swiped away meanwhile: nothing more to say.
            if (_priceToast.value !== asking) return@launch
            when (result) {
                is PriceRetry.Updated -> {
                    val updated = PriceToastState.Updated(result.title, result.line)
                    _priceToast.value = updated
                    refresh()
                    delay(2_000)
                    if (_priceToast.value === updated) _priceToast.value = null
                }
                is PriceRetry.Failed -> {
                    _priceToast.value = PriceToastState.Failed(result.toast)
                    refresh()
                }
            }
        }
    }

    /** The phone's clock as Rust takes it: now, its UTC offset, and whether it shows 24 hours. */
    private fun clock(): Triple<ULong, Int, Boolean> {
        val now = System.currentTimeMillis()
        return Triple((now / 1000).toULong(), TimeZone.getDefault().getOffset(now) / 1000, DateFormat.is24HourFormat(app))
    }

    /** The card Home's sync chip opens, every word from Rust; null without a wallet. */
    fun syncDetails(): SyncDetails? {
        val wallet = _wallet.value ?: return null
        val (seconds, offset, hours24) = clock()
        return wallet.syncDetails(seconds, offset, hours24)
    }

    /** The Coins screen, every word from Rust; null without a wallet. */
    suspend fun coins(): Coins? {
        val wallet = _wallet.value ?: return null
        val (seconds, offset, hours24) = clock()
        return wallet.coins(seconds, offset, hours24)
    }

    /** A payment's own screen, every word from Rust; null if there's no such payment. */
    suspend fun payment(tx: String): PaymentDetail? {
        val wallet = _wallet.value ?: return null
        val (seconds, offset, hours24) = clock()
        return wallet.payment(tx, seconds, offset, hours24)
    }

    /**
     * An amount field's words for [text], in money when [inFiat], every word
     * from Rust; null without a wallet. Money typed against an old price has
     * Rust ask again, a failure coming as the toast.
     */
    suspend fun amountEntry(text: String, inFiat: Boolean): AmountEntry? {
        val wallet = _wallet.value ?: return null
        val (seconds, offset, hours24) = clock()
        return wallet.amountEntry(text, inFiat, seconds, offset, hours24)
    }

    /** Keeps the note typed on Receive with the address it was for; outlives the screen. */
    fun rememberReceiveNote(index: UInt, note: String) {
        val wallet = _wallet.value ?: return
        scope.launch { runCatching { wallet.rememberReceiveNote(index, note) } }
    }

    /** "Also show amounts in" a currency's [code], or XMR only (null); Rust's words show as a notice when the provider had to change for it. */
    suspend fun setCurrency(code: String?) = say(changePrices { it.setCurrency(code) })

    /** Prices from [provider]; Rust's words show as a notice when the transport had to change for it. */
    suspend fun setPriceProvider(provider: PriceProvider) = say(changePrices { it.setPriceProvider(provider) })

    /** Throws what Rust refused (Tor, for now). */
    suspend fun setPriceTransport(transport: PriceTransport) = changePrices { it.setPriceTransport(transport) }

    /** "Show first": the currency first, or XMR. */
    suspend fun setFiatFirst(fiatFirst: Boolean) = changePrices { it.setFiatFirst(fiatFirst) }

    private suspend fun <T> changePrices(change: suspend (FobWallet) -> T): T? {
        val wallet = _wallet.value ?: return null
        val result = change(wallet)
        refresh()
        return result
    }

    /** Every provider of the chosen currency asked for its price now; null with XMR only. */
    suspend fun checkPrices(): List<PriceCheck>? = _wallet.value?.checkPrices()

    /** The "Also show amounts in" list, with this phone's currency (its locale's) for Rust to place. */
    suspend fun currencyList(query: String): CurrencyList? {
        val phone = runCatching { java.util.Currency.getInstance(java.util.Locale.getDefault()).currencyCode }.getOrNull()
        return _wallet.value?.currencyList(phone, query)
    }

    /** Pull to refresh: asks the node now (and the price), and returns once that round is over. */
    suspend fun syncNow() {
        val wallet = _wallet.value ?: return
        wallet.syncNow()
        refresh()
    }

    /**
     * Talks to another node, accepting its self-signed certificate with [selfSigned];
     * on another host, instant updates go off. Throws what Rust refused.
     */
    suspend fun setNode(url: String, selfSigned: Boolean) {
        val wallet = _wallet.value ?: return
        wallet.setNode(url, selfSigned)
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
        // Prices are asked only while Fob is on screen.
        _wallet.value?.let { if (visible) it.startPrices(observer) else it.stopPrices() }
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
        _wallet.value?.let { close(it) }
        _wallet.value = null
        _balance.value = null
        _activity.value = emptyList()
        _notReady.value = null
        _prices.value = null
        _priceToast.value = null
        _status.value = null
        _you.value = null
        prefs.edit().putString("network", network.name.lowercase()).apply()
        _network.value = network
        open()
    }

    /** Removes this network's wallet from the phone. Its seed restores it. */
    fun forget() {
        val network = _network.value
        _wallet.value?.let { close(it) }
        _wallet.value = null
        _balance.value = null
        _activity.value = emptyList()
        _notReady.value = null
        _prices.value = null
        _priceToast.value = null
        _status.value = null
        _you.value = null
        File(path(network)).delete()
    }

    /**
     * [amount] as the amount field took it, in money when [inFiat] (Rust
     * converts at the current price); [note] (typed, or the payment
     * code's description) is kept with the payment, and Rust keeps what
     * still applies of [code], the payment code the form came from.
     */
    suspend fun prepare(to: String, amount: String, inFiat: Boolean, note: String?, code: String?): Prepared =
        requireNotNull(_wallet.value).prepare(to, amount, inFiat, note, code)

    /** What the Paid screen offers for payment [tx]; null for a split or an address that's already a contact's. */
    suspend fun saveOffer(tx: String): SaveOffer? = _wallet.value?.saveOffer(tx)

    /** Who this wallet paid before, for Send's sheet; empty without a wallet. */
    suspend fun paidBefore(): List<PaidBefore> {
        val wallet = _wallet.value ?: return emptyList()
        val (seconds, offset, hours24) = clock()
        return wallet.paidBefore(seconds, offset, hours24)
    }

    /** Contacts by name, those whose name or note has [search] in it; empty without a wallet. */
    suspend fun contacts(search: String = ""): List<ContactRow> {
        val wallet = _wallet.value ?: return emptyList()
        val (seconds, offset, hours24) = clock()
        return wallet.contacts(search, seconds, offset, hours24)
    }

    /** The contacts Send's sheet offers: with a send address, the most recently paid first. */
    suspend fun contactsOnSend(): List<ContactRow> {
        val wallet = _wallet.value ?: return emptyList()
        val (seconds, offset, hours24) = clock()
        return wallet.contactsOnSend(seconds, offset, hours24)
    }

    /** Addresses paid that are no contact's, for "Paid, not saved". */
    suspend fun paidNotSaved(): List<PaidBefore> {
        val wallet = _wallet.value ?: return emptyList()
        val (seconds, offset, hours24) = clock()
        return wallet.paidNotSaved(seconds, offset, hours24)
    }

    /** A contact's card, every word from Rust; null once it's deleted. */
    suspend fun contactCard(id: ULong): ContactCard? {
        val wallet = _wallet.value ?: return null
        val (seconds, offset, hours24) = clock()
        return wallet.contactCard(id, seconds, offset, hours24)
    }

    /**
     * Changes the contacts (throwing what Rust refused), then refreshes:
     * Activity names payments to them.
     */
    suspend fun <T> changeContacts(change: suspend (FobWallet) -> T): T {
        val result = change(requireNotNull(_wallet.value))
        refresh()
        return result
    }

    /**
     * Makes a request (Share or Copy on Receive, never just opening it): a
     * new address for [contact] or no one, asking [amount] (in money when
     * [inFiat]) with [note]. Throws what Rust refused. Activity shows it.
     */
    suspend fun makeRequest(contact: ULong?, amount: String, inFiat: Boolean, note: String): MadeRequest {
        val made = requireNotNull(_wallet.value).makeRequest(contact, amount.ifBlank { null }, inFiat, note.ifBlank { null })
        refresh()
        return made
    }

    /** A request's own screen, every word from Rust; null once it's deleted. */
    suspend fun request(id: ULong): RequestDetail? {
        val wallet = _wallet.value ?: return null
        val (seconds, offset, hours24) = clock()
        return wallet.request(id, seconds, offset, hours24)
    }

    /** Deletes a request; its address still receives. Throws what Rust refused. */
    suspend fun deleteRequest(id: ULong) {
        requireNotNull(_wallet.value).deleteRequest(id)
        refresh()
    }

    /** Share's sheet: every contact, each with its requests in a line. */
    suspend fun requestContacts(): List<ContactRow> = _wallet.value?.requestContacts() ?: emptyList()

    /** Saves your name and photo; throws what Rust refused. */
    suspend fun setProfile(name: String, photo: ByteArray?): Profile {
        val wallet = requireNotNull(_wallet.value)
        return wallet.setProfile(name, photo).also { _you.value = You(it, wallet.profileRow()) }
    }

    /** "Share my profile": a share on a new address, kept at once; Activity shows it. */
    suspend fun shareProfile(): ProfileShare {
        val share = requireNotNull(_wallet.value).shareProfile()
        refresh()
        return share
    }

    /** Your shares, newest first, each with what came to it. */
    suspend fun profileShares(): List<ShareLine> {
        val wallet = _wallet.value ?: return emptyList()
        val (seconds, offset, hours24) = clock()
        return wallet.profileShares(seconds, offset, hours24)
    }

    /** Touch phones, as share [id]'s screen opens with NFC on: a swap on that share's address. */
    suspend fun startSwap(id: ULong): TouchSwap = requireNotNull(_wallet.value).startSwap(id)

    /** Ends a swap, keeping it as a share when anything went out; their profile when it came. */
    suspend fun endSwap(swap: TouchSwap): Swapped? {
        val swapped = _wallet.value?.endSwap(swap)
        refresh()
        return swapped
    }

    /** Ends a swap as its screen goes, whatever came, on the model's scope so leaving doesn't cut it short. */
    fun endSwapLater(swap: TouchSwap) {
        scope.launch { runCatching { endSwap(swap) } }
    }

    /** A split for the confirm screen: [counts] pieces of each of Rust's notes. */
    suspend fun prepareSplit(counts: List<UInt>): Prepared = requireNotNull(_wallet.value).prepareSplit(counts)

    /**
     * Sends [prepared] on the model's scope: once begun it runs to the end
     * even if Confirm goes (the screen made again), and Rust records it
     * whatever happens to this call.
     */
    suspend fun send(prepared: Prepared): String {
        val wallet = requireNotNull(_wallet.value)
        return scope.async { wallet.send(prepared.id).also { refresh() } }.await()
    }

    /** Forgets a prepared payment Confirm left without sending; on the model's scope, as Confirm is going. */
    fun cancelLater(prepared: Prepared) {
        val wallet = _wallet.value ?: return
        scope.launch { wallet.cancel(prepared.id) }
    }


    private companion object {
        /** About a day of blocks. */
        const val FAR_BEHIND = 720u
        /** The least time the price toast says "Trying again…". */
        const val RETRY_SHOWN_MS = 800L
    }
}

/** Your profile (null before it's set up) and its row's words: "Your profile", or "Add a shareable Fob contact". */
data class You(val profile: Profile?, val row: ProfileRow)
