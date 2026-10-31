//! [`fob_core`] for the apps, through UniFFI. This crate only converts types
//! and turns fob-core's async calls into Kotlin `suspend` functions; every
//! decision about the wallet is made in fob-core, so the apps stay thin.

use std::{path::PathBuf, sync::Arc};

use fob_core::{
    activity, amount, not_ready, pattern,
    price::{self, Source as _},
    qr, receipt, uri,
};

uniffi::setup_scaffolding!();

/// What went wrong, for the app to show. (The field is `reason`, not
/// `message`, which Kotlin's `Throwable` already has.)
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FobError {
    /// Something the person typed or scanned isn't right.
    #[error("{reason}")]
    Invalid { reason: String },
    /// The node couldn't be reached or answered badly.
    #[error("{reason}")]
    Node { reason: String },
    #[error("{reason}")]
    NotEnoughFunds { reason: String },
    /// The wallet file and key don't match.
    #[error("{reason}")]
    WrongKey { reason: String },
    #[error("{reason}")]
    ViewOnly { reason: String },
    #[error("{reason}")]
    Other { reason: String },
}

impl From<fob_core::FobError> for FobError {
    fn from(e: fob_core::FobError) -> Self {
        use fob_core::FobError as E;
        let reason = e.to_string();
        match e {
            E::BadSeed | E::BadAddress(_) | E::BadKey | E::BadAmount | E::BadCode | E::BadEndpoint(_) | E::WrongNetwork { .. } => FobError::Invalid { reason },
            E::Node(_) => FobError::Node { reason },
            E::NotEnoughFunds { .. } => FobError::NotEnoughFunds { reason },
            E::WrongKey => FobError::WrongKey { reason },
            E::ViewOnly => FobError::ViewOnly { reason },
            E::Send(_) | E::Storage(_) | E::NoSuchPayment | E::Price(_) => FobError::Other { reason },
        }
    }
}

type Result<T> = std::result::Result<T, FobError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Network {
    Mainnet,
    Stagenet,
}

impl From<Network> for fob_core::Network {
    fn from(n: Network) -> Self {
        match n {
            Network::Mainnet => fob_core::Network::Mainnet,
            Network::Stagenet => fob_core::Network::Stagenet,
        }
    }
}

impl From<fob_core::Network> for Network {
    fn from(n: fob_core::Network) -> Self {
        match n {
            fob_core::Network::Mainnet => Network::Mainnet,
            fob_core::Network::Stagenet => Network::Stagenet,
        }
    }
}

fn file_key(key: Vec<u8>) -> Result<[u8; 32]> {
    key.try_into().map_err(|_| FobError::Other { reason: "the wallet key must be 32 bytes".into() })
}

/// Balances and lists: four decimals.
fn xmr(atomic: u64) -> String {
    amount::format_short(atomic)
}

/// What's about to be paid: every digit.
fn exact(atomic: u64) -> String {
    amount::format(atomic)
}

#[derive(uniffi::Record)]
pub struct ReceiptLine {
    pub label: String,
    pub amount: String,
}

#[derive(uniffi::Record)]
pub struct Receipt {
    pub merchant: String,
    pub issued: String,
    pub lines: Vec<ReceiptLine>,
    pub item_count: u32,
    pub tax: Option<String>,
    pub total: String,
    pub currency: String,
}

impl From<receipt::ReceiptView> for Receipt {
    fn from(r: receipt::ReceiptView) -> Self {
        Receipt {
            merchant: r.merchant,
            issued: r.issued,
            lines: r.lines.into_iter().map(|l| ReceiptLine { label: l.label, amount: l.amount }).collect(),
            item_count: r.item_count,
            tax: r.tax,
            total: r.total,
            currency: r.currency,
        }
    }
}

#[derive(uniffi::Record)]
pub struct Balance {
    pub total: u64,
    pub unlocked: u64,
    /// "2.4813"
    pub total_text: String,
    pub unlocked_text: String,
    /// "0.5 XMR ready to spend", only while some money can't be spent yet.
    pub ready_text: Option<String>,
    /// What the balance is worth now, "$412.06"; only with prices on and a
    /// current price.
    pub fiat_text: Option<String>,
    /// Show [`fiat_text`](Self::fiat_text) first, the XMR under it.
    pub fiat_first: bool,
}

/// A currency to choose.
#[derive(uniffi::Record)]
pub struct CurrencyChoice {
    /// "USD"
    pub code: String,
    /// "US dollar"
    pub name: String,
}

/// How price requests travel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PriceTransport {
    Web,
    Tor,
}

impl From<PriceTransport> for price::Transport {
    fn from(t: PriceTransport) -> Self {
        match t {
            PriceTransport::Web => price::Transport::Web,
            PriceTransport::Tor => price::Transport::Tor,
        }
    }
}

impl From<price::Transport> for PriceTransport {
    fn from(t: price::Transport) -> Self {
        match t {
            price::Transport::Web => PriceTransport::Web,
            price::Transport::Tor => PriceTransport::Tor,
        }
    }
}

/// A place prices can come from, to choose.
#[derive(uniffi::Record)]
pub struct PriceSourceChoice {
    pub transport: PriceTransport,
    /// "Kraken · over Tor"
    pub text: String,
    /// "Not available yet" for one Fob can't use.
    pub note: Option<String>,
    pub available: bool,
    pub chosen: bool,
}

/// Settings' "Display" and "Prices" cards.
#[derive(uniffi::Record)]
pub struct PriceSettings {
    /// "Show prices".
    pub on: bool,
    /// "USD"
    pub currency: String,
    pub currencies: Vec<CurrencyChoice>,
    /// "Show amounts in" the currency, not XMR.
    pub fiat_first: bool,
    /// "Kraken · over the internet"
    pub source_text: String,
    pub sources: Vec<PriceSourceChoice>,
    /// Why the last request failed, until one works.
    pub error: Option<String>,
}

#[derive(uniffi::Record)]
/// A payment in Activity: enough to open its screen, which asks `payment`
/// for the rest.
pub struct Activity {
    pub tx: String,
    pub outgoing: bool,
    /// "0.1421", without a sign.
    pub amount_text: String,
    pub receipt: Option<Receipt>,
}

fn activity(a: fob_core::Activity) -> Activity {
    Activity {
        outgoing: a.direction == fob_core::Direction::Out,
        amount_text: xmr(a.amount),
        tx: a.tx,
        receipt: a.receipt.map(Into::into),
    }
}

/// One row of Home's Activity, ready to draw.
#[derive(uniffi::Record)]
pub struct ActivityRow {
    /// The payment, for its own screen.
    pub activity: Activity,
    /// The shop, who was paid, or "Received".
    pub title: String,
    /// "Receipt, 3 items · 09:12", "Not ready · 3 of 10 · 08:40".
    pub line: String,
    /// With its sign: "+0.5", "−0.1421".
    pub amount_text: String,
    /// The receipt's total: "$13.50".
    pub fiat_text: Option<String>,
}

/// A day of Activity under its label: "TODAY", "YESTERDAY", "27 OCT".
#[derive(uniffi::Record)]
pub struct ActivityDay {
    pub label: String,
    pub rows: Vec<ActivityRow>,
}

/// One piece of money not ready yet: an output short of its confirmations.
#[derive(uniffi::Record)]
pub struct NotReadyPiece {
    /// "0.653"
    pub amount_text: String,
    /// Worth now, with prices on: "$108.44".
    pub fiat_text: Option<String>,
    pub confirmations: u64,
    /// 10, or 60 for a mined reward.
    pub needed: u64,
    /// `confirmations / needed`, for its ring.
    pub progress: f32,
    /// "Received · 08:40", "Change · Corner Café".
    pub source: String,
    /// "09:02"
    pub ready: String,
    /// "in 14 min"
    pub eta: String,
}

/// The money not ready yet, for Home's card.
#[derive(uniffi::Record)]
pub struct NotReady {
    /// "0.653 XMR not ready yet"
    pub title: String,
    /// "3 pieces · all ready about 09:02"
    pub line: String,
    /// Weighted by amount, 0 to 1.
    pub progress: f32,
    /// "42%"
    pub percent: String,
    /// Soonest ready first.
    pub pieces: Vec<NotReadyPiece>,
}

impl From<not_ready::NotReady> for NotReady {
    fn from(n: not_ready::NotReady) -> Self {
        NotReady {
            title: n.title,
            line: n.line,
            progress: n.progress,
            percent: n.percent,
            pieces: n
                .pieces
                .into_iter()
                .map(|p| NotReadyPiece {
                    amount_text: p.amount,
                    fiat_text: p.fiat,
                    confirmations: p.confirmations,
                    needed: p.needed,
                    progress: p.progress,
                    source: p.source,
                    ready: p.ready,
                    eta: p.eta,
                })
                .collect(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct SyncStatus {
    pub scanned: u64,
    pub tip: u64,
    pub synced: bool,
    pub progress: f32,
    pub seconds_left: Option<u64>,
    /// When the last scanned block was mined, seconds since the epoch.
    pub up_to: u64,
    pub error: Option<String>,
    pub zmq: ZmqStatus,
    /// The subtitle for "Instant updates (ZMQ)": "host:18083 · listening", or "Off".
    pub zmq_text: String,
}

/// Instant updates from the node's ZMQ publisher.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ZmqStatus {
    Off,
    Connecting,
    Listening,
    CantConnect { reason: String },
}

impl From<fob_core::ZmqStatus> for ZmqStatus {
    fn from(z: fob_core::ZmqStatus) -> Self {
        match z {
            fob_core::ZmqStatus::Off => ZmqStatus::Off,
            fob_core::ZmqStatus::Connecting => ZmqStatus::Connecting,
            fob_core::ZmqStatus::Listening => ZmqStatus::Listening,
            fob_core::ZmqStatus::CantConnect(reason) => ZmqStatus::CantConnect { reason },
        }
    }
}

impl From<fob_core::SyncStatus> for SyncStatus {
    fn from(s: fob_core::SyncStatus) -> Self {
        SyncStatus {
            scanned: s.scanned,
            tip: s.tip,
            synced: s.synced,
            progress: s.progress,
            seconds_left: s.seconds_left,
            up_to: s.up_to,
            error: s.error,
            zmq: s.zmq.into(),
            zmq_text: s.zmq_text,
        }
    }
}

/// Hears about syncing; implemented by the app. Called from Fob's own
/// threads: hand the status to the UI, don't call back into the wallet here.
#[uniffi::export(with_foreign)]
pub trait SyncObserver: Send + Sync {
    fn on_status(&self, status: SyncStatus);
}

struct Observer(Arc<dyn SyncObserver>);

impl fob_core::SyncListener for Observer {
    fn on_status(&self, status: fob_core::SyncStatus) {
        self.0.on_status(status.into());
    }
}

#[derive(uniffi::Record)]
pub struct Prepared {
    pub id: u64,
    pub to: String,
    pub amount_text: String,
    pub fee_text: String,
    pub total_text: String,
    /// What they're worth now, with prices on: "$20.00".
    pub amount_fiat: Option<String>,
    pub fee_fiat: Option<String>,
    pub total_fiat: Option<String>,
    pub receipt: Option<Receipt>,
}

#[derive(uniffi::Record)]
pub struct PaymentRequest {
    pub address: String,
    pub amount: Option<u64>,
    pub amount_text: Option<String>,
    pub recipient_name: Option<String>,
    pub description: Option<String>,
    pub receipt: Option<Receipt>,
    /// The receipt as ORF JSON, to hand back to `prepare` so it's kept.
    pub receipt_json: Option<String>,
}

#[derive(uniffi::Record)]
pub struct SeedPhrase {
    /// "polyseed" (16 words) or "classic" (25).
    pub kind: String,
    pub words: Vec<String>,
}

#[derive(uniffi::Record)]
pub struct WalletCodePreview {
    pub address: String,
    pub can_spend: bool,
    pub restore_height: Option<u64>,
    /// Roughly when that block was mined, seconds since the epoch.
    pub restore_time: Option<u64>,
}

#[derive(uniffi::Record)]
pub struct QrCode {
    pub size: u32,
    /// Row by row, `true` for dark.
    pub dark: Vec<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CodeKind {
    Payment,
    Wallet,
    Unknown,
}

/// A Fob wallet, saved in one file.
#[derive(uniffi::Object)]
pub struct FobWallet {
    inner: fob_core::Wallet,
}

fn wallet(inner: fob_core::Result<fob_core::Wallet>) -> Result<Arc<FobWallet>> {
    Ok(Arc::new(FobWallet { inner: inner? }))
}

fn height(network: Network, restore_point: Option<String>) -> Result<Option<u64>> {
    restore_point
        .filter(|t| !t.trim().is_empty())
        .map(|t| fob_core::restore_point(network.into(), &t))
        .transpose()
        .map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
impl FobWallet {
    /// A new wallet with a fresh 16-word seed, saved at `path` under `key`
    /// (32 bytes the app keeps in the Keystore).
    #[uniffi::constructor]
    pub fn create(path: String, key: Vec<u8>, network: Network) -> Result<Arc<Self>> {
        wallet(fob_core::Wallet::create(&PathBuf::from(path), file_key(key)?, network.into()))
    }

    /// From 16 or 25 seed words. `restore_point` is a date (YYYY-MM-DD) or
    /// block height to start from; a 16-word seed knows its own.
    #[uniffi::constructor]
    pub fn restore_words(path: String, key: Vec<u8>, network: Network, words: String, restore_point: Option<String>) -> Result<Arc<Self>> {
        let height = height(network, restore_point)?;
        wallet(fob_core::Wallet::restore_words(&PathBuf::from(path), file_key(key)?, network.into(), &words, height))
    }

    /// From a scanned wallet code.
    #[uniffi::constructor]
    pub fn restore_code(path: String, key: Vec<u8>, network: Network, code: String, restore_point: Option<String>) -> Result<Arc<Self>> {
        let height = height(network, restore_point)?;
        wallet(fob_core::Wallet::restore_code(&PathBuf::from(path), file_key(key)?, network.into(), &code, height))
    }

    /// A wallet that watches: primary address and private view key.
    #[uniffi::constructor]
    pub fn restore_view_only(
        path: String,
        key: Vec<u8>,
        network: Network,
        address: String,
        view_key: String,
        restore_point: Option<String>,
    ) -> Result<Arc<Self>> {
        let height = height(network, restore_point)?;
        wallet(fob_core::Wallet::restore_view_only(&PathBuf::from(path), file_key(key)?, network.into(), &address, &view_key, height))
    }

    #[uniffi::constructor]
    pub fn open(path: String, key: Vec<u8>) -> Result<Arc<Self>> {
        wallet(fob_core::Wallet::open(&PathBuf::from(path), file_key(key)?))
    }

    pub fn network(&self) -> Network {
        self.inner.network().into()
    }

    pub fn can_spend(&self) -> bool {
        self.inner.can_spend()
    }

    pub fn primary_address(&self) -> String {
        self.inner.primary_address()
    }

    pub fn seed_phrase(&self) -> Option<SeedPhrase> {
        self.inner.seed_words().map(|(kind, words)| SeedPhrase {
            kind: match kind {
                fob_core::SeedKind::Polyseed => "polyseed".into(),
                fob_core::SeedKind::Classic => "classic".into(),
            },
            words: words.split(' ').map(str::to_owned).collect(),
        })
    }

    pub async fn wallet_code(&self) -> String {
        self.inner.wallet_code().await.to_string()
    }

    pub async fn receive_address(&self) -> Result<String> {
        Ok(self.inner.receive_address().await?)
    }

    pub async fn new_receive_address(&self) -> Result<String> {
        Ok(self.inner.new_receive_address().await?)
    }

    pub async fn receive_index(&self) -> u32 {
        self.inner.receive_index().await
    }

    pub async fn node(&self) -> String {
        self.inner.node().await
    }

    pub async fn set_node(&self, url: String) -> Result<()> {
        Ok(self.inner.set_node(&url).await?)
    }

    /// The node's ZMQ publisher: the one set, else the usual port on the
    /// node's host, to show and edit.
    pub async fn zmq_endpoint(&self) -> String {
        self.inner.zmq_endpoint().await
    }

    /// Turns instant updates on or off; `endpoint`, if given, replaces the
    /// one set (it must be on the node's own host).
    pub async fn set_zmq(&self, on: bool, endpoint: Option<String>) -> Result<()> {
        Ok(self.inner.set_zmq(on, endpoint.as_deref()).await?)
    }

    pub async fn balance(&self) -> Balance {
        let b = self.inner.balance().await;
        Balance {
            total: b.total,
            unlocked: b.unlocked,
            total_text: xmr(b.total),
            unlocked_text: xmr(b.unlocked),
            ready_text: b.ready_text(),
            fiat_text: self.inner.worth_now(b.total).await,
            fiat_first: self.inner.price_settings().await.fiat_first,
        }
    }

    /// Every payment, grouped by the phone's day, newest first. `now` is
    /// seconds since the epoch, `utc_offset` the phone's offset in seconds,
    /// `hours_24` its clock setting.
    pub async fn activity_days(&self, now: u64, utc_offset: i32, hours_24: bool) -> Vec<ActivityDay> {
        let clock = activity::Clock { now, utc_offset, hours_24 };
        activity::by_day(self.inner.activity().await, clock)
            .into_iter()
            .map(|day| ActivityDay {
                label: day.label,
                rows: day
                    .rows
                    .into_iter()
                    .map(|r| ActivityRow { activity: activity(r.activity), title: r.title, line: r.line, amount_text: r.amount, fiat_text: r.fiat })
                    .collect(),
            })
            .collect()
    }

    /// The money not ready to spend yet, piece by piece, with about when
    /// it will be; `None` when all of it is. The clock as for `activity_days`.
    pub async fn not_ready(&self, now: u64, utc_offset: i32, hours_24: bool) -> Option<NotReady> {
        self.inner.not_ready(activity::Clock { now, utc_offset, hours_24 }).await.map(Into::into)
    }

    pub fn status(&self) -> SyncStatus {
        self.inner.status().into()
    }

    /// Keeps the wallet synced in the background until `stop_sync`.
    pub fn start_sync(&self, observer: Arc<dyn SyncObserver>) {
        self.inner.start_sync(Arc::new(Observer(observer)));
    }

    pub fn stop_sync(&self) {
        self.inner.stop_sync();
    }

    /// Pull to refresh: asks the node for new blocks and its pool now, and
    /// the price if a minute has passed; returns when that round is over,
    /// or after 20 s.
    pub async fn sync_now(&self) {
        self.inner.pull().await;
    }

    /// Asks for prices while Fob is on screen (every 5 minutes, and at once
    /// when a price setting changes), until `stop_prices`. The observer
    /// hears of each new price.
    pub fn start_prices(&self, observer: Arc<dyn SyncObserver>) {
        self.inner.start_prices(Arc::new(Observer(observer)));
    }

    pub fn stop_prices(&self) {
        self.inner.stop_prices();
    }

    /// Settings' "Display" and "Prices" cards.
    pub async fn price_settings(&self) -> PriceSettings {
        let s = self.inner.price_settings().await;
        let sources = [price::Transport::Web, price::Transport::Tor]
            .into_iter()
            .map(|t| PriceSourceChoice {
                transport: t.into(),
                text: price::Settings { transport: t, ..s.clone() }.source_text(),
                note: (!t.available()).then(|| "Not available yet".to_owned()),
                available: t.available(),
                chosen: t == s.transport,
            })
            .collect();
        PriceSettings {
            on: s.on,
            currencies: s
                .provider
                .currencies()
                .iter()
                .map(|code| CurrencyChoice { code: (*code).to_owned(), name: price::currency(code).name.to_owned() })
                .collect(),
            currency: s.currency.clone(),
            fiat_first: s.fiat_first,
            source_text: s.source_text(),
            sources,
            error: if s.on { self.inner.price_error() } else { None },
        }
    }

    /// "Show prices". Off, Fob never asks for one.
    pub async fn set_prices_on(&self, on: bool) -> Result<()> {
        Ok(self.inner.set_prices_on(on).await?)
    }

    /// Prices in `code`, one of `price_settings().currencies`.
    pub async fn set_currency(&self, code: String) -> Result<()> {
        Ok(self.inner.set_currency(&code).await?)
    }

    /// How price requests travel; one not available is refused.
    pub async fn set_price_transport(&self, transport: PriceTransport) -> Result<()> {
        Ok(self.inner.set_price_transport(transport.into()).await?)
    }

    /// "Show amounts in" the currency first (true) or XMR.
    pub async fn set_fiat_first(&self, fiat_first: bool) -> Result<()> {
        Ok(self.inner.set_fiat_first(fiat_first).await?)
    }

    /// Builds a payment for the confirm screen. `amount` is text as typed;
    /// `receipt_json` comes from the scanned `PaymentRequest`.
    /// `note` is the payment code's description, kept with the payment.
    pub async fn prepare(&self, to: String, amount: String, receipt_json: Option<String>, note: Option<String>) -> Result<Prepared> {
        let atomic = amount::parse(&amount)?;
        let p = self.inner.prepare(&to, atomic, receipt_json, note).await?;
        Ok(Prepared {
            id: p.id,
            to: p.to,
            amount_text: exact(p.amount),
            fee_text: amount::format(p.fee),
            total_text: amount::format(p.amount + p.fee),
            amount_fiat: self.inner.worth_now(p.amount).await,
            fee_fiat: self.inner.worth_now(p.fee).await,
            total_fiat: self.inner.worth_now(p.amount + p.fee).await,
            receipt: p.receipt.map(Into::into),
        })
    }

    /// Keeps the note typed on Receive for the address at `index`, so
    /// payments to it say what they were for.
    pub async fn remember_receive_note(&self, index: u32, note: String) -> Result<()> {
        Ok(self.inner.remember_receive_note(index, &note).await?)
    }

    /// The money Receive's amount can be typed in; `None` with prices off
    /// or no current price, when it takes XMR only.
    pub async fn receive_currency(&self) -> Option<ReceiveCurrency> {
        self.inner.receive_currency().await.map(|(prefix, code)| ReceiveCurrency { prefix, code })
    }

    /// Receive's amount switched between money and XMR (`to_fiat`): the
    /// typed text converted, or blank.
    pub async fn convert_amount(&self, text: String, to_fiat: bool) -> String {
        self.inner.convert_amount(&text, to_fiat).await
    }

    /// A payment code to show: the address and, if typed, an amount (in
    /// money when `in_fiat`) and a note for the payer. Throws when the
    /// amount isn't one.
    pub async fn receive_code(&self, address: String, amount: Option<String>, in_fiat: bool, note: Option<String>) -> Result<ReceiveCode> {
        let r = self.inner.receive_code(&address, amount.as_deref(), in_fiat, note.as_deref()).await?;
        Ok(ReceiveCode { uri: r.uri, headline: r.headline, detail: r.detail, asks: r.asks })
    }

    /// The payment `tx` for its own screen; the clock as for `activity_days`.
    pub async fn payment(&self, tx: String, now: u64, utc_offset: i32, hours_24: bool) -> Option<PaymentDetail> {
        let d = self.inner.payment(&tx, activity::Clock { now, utc_offset, hours_24 }).await?;
        Some(PaymentDetail {
            amount_text: d.amount,
            when: d.when,
            priced: d.priced,
            worth_now: d.worth_now,
            rate_now: d.rate_now,
            then_label: d.then_label,
            worth_then: d.worth_then,
            rate_then: d.rate_then,
            change: d.change,
            up: d.up,
            status: d.status,
            progress: d.progress,
            party_label: d.party_label,
            party: d.party,
            pieces_label: d.pieces_label,
            pieces: d.pieces,
            fee: d.fee,
            tx_short: d.tx_short,
            tx,
            footer: d.footer,
        })
    }

    /// Signs and broadcasts; returns the transaction hash.
    pub async fn send(&self, id: u64) -> Result<String> {
        Ok(self.inner.send(id).await?)
    }

    pub async fn cancel(&self, id: u64) {
        self.inner.cancel(id).await;
    }
}

/// Reads a scanned or pasted payment code.
#[uniffi::export]
pub fn parse_payment(network: Network, text: String) -> Result<PaymentRequest> {
    let r = uri::parse_payment(network.into(), &text)?;
    Ok(PaymentRequest {
        amount_text: r.amount.map(exact),
        receipt_json: r.receipt.as_ref().and_then(receipt::to_json),
        receipt: r.receipt.as_ref().map(|receipt| receipt::ReceiptView::from(receipt).into()),
        address: r.address,
        amount: r.amount,
        recipient_name: r.recipient_name,
        description: r.description,
    })
}

/// Receive's payment code and the words around it.
#[derive(uniffi::Record)]
pub struct ReceiveCode {
    /// The payment code: `monero:<address>?tx_amount=…&tx_description=…`,
    /// always in XMR.
    pub uri: String,
    /// The big line on the expanded code: "$20.00" or "0.5 XMR", when
    /// there's an amount.
    pub headline: Option<String>,
    /// The small line under it: "0.1204 XMR · Coffee beans", or the note.
    pub detail: Option<String>,
    /// Under the field for an amount in money: "The code asks for 0.1204
    /// XMR · 1 XMR = $166.10".
    pub asks: Option<String>,
}

/// The money Receive's amount can be typed in.
#[derive(uniffi::Record)]
pub struct ReceiveCurrency {
    /// Before what's typed: "$", or the code for a currency without a symbol.
    pub prefix: String,
    /// "USD", for the toggle.
    pub code: String,
}

/// A payment's own screen, every word from Rust.
#[derive(uniffi::Record)]
pub struct PaymentDetail {
    /// "+0.5", "−0.1421"
    pub amount_text: String,
    /// "Today 08:40 · Coffee beans"
    pub when: String,
    /// Prices are on: show the two boxes.
    pub priced: bool,
    /// "$83.03" or "Not available"
    pub worth_now: String,
    /// "1 XMR = $166.06"
    pub rate_now: Option<String>,
    /// "When received", "When sent"
    pub then_label: String,
    pub worth_then: String,
    pub rate_then: Option<String>,
    /// "Up $3.63 (4.6%) since then"
    pub change: Option<String>,
    /// Green when up, muted when down.
    pub up: bool,
    pub status: String,
    /// While not ready: 0 to 1, for a thin bar.
    pub progress: Option<f32>,
    /// "From", "To"
    pub party_label: String,
    pub party: Option<String>,
    /// "Pieces", "Pieces spent"
    pub pieces_label: String,
    /// "2 · 0.3 + 0.2"
    pub pieces: Option<String>,
    /// "0.00003 XMR", for a payment out.
    pub fee: Option<String>,
    /// "a1b2c3 … d4e5f6"
    pub tx_short: String,
    /// The whole id, to copy.
    pub tx: String,
    /// "Prices from Kraken."
    pub footer: Option<String>,
}

/// Receive's note as typed, cut to what a payment code holds.
#[derive(uniffi::Record)]
pub struct ReceiveNote {
    /// The note to keep in the field.
    pub text: String,
    /// "That's as long as a note can be", when what was typed was cut.
    pub hint: Option<String>,
}

/// Receive's note cut to what a payment code holds (120 characters, fewer
/// for emoji and other scripts that take more room in the code), so the
/// code always encodes. Call it with each change to the field.
#[uniffi::export]
pub fn fit_receive_note(text: String) -> ReceiveNote {
    let (text, cut) = uri::fit_note(&text);
    ReceiveNote { text, hint: cut.then(|| "That's as long as a note can be".to_owned()) }
}

/// A piece of an address, as Receive shows it.
#[derive(uniffi::Record)]
pub struct AddressGroup {
    pub text: String,
    /// The first, middle and last groups: the ones people check.
    pub highlighted: bool,
}

/// An address in groups of five, the first, middle and last highlighted.
#[uniffi::export]
pub fn address_groups(address: String) -> Vec<AddressGroup> {
    uri::address_groups(&address).into_iter().map(|g| AddressGroup { text: g.text, highlighted: g.highlighted }).collect()
}

#[uniffi::export]
pub fn classify_code(network: Network, text: String) -> CodeKind {
    match uri::classify(network.into(), &text) {
        uri::CodeKind::Payment => CodeKind::Payment,
        uri::CodeKind::Wallet => CodeKind::Wallet,
        uri::CodeKind::Unknown => CodeKind::Unknown,
    }
}

#[uniffi::export]
pub fn preview_wallet_code(network: Network, code: String) -> Result<WalletCodePreview> {
    let p = fob_core::preview_wallet_code(network.into(), &code)?;
    Ok(WalletCodePreview {
        restore_time: p.restore_height.map(|h| fob_core::Network::from(network).timestamp_for(h)),
        address: p.address,
        can_spend: p.can_spend,
        restore_height: p.restore_height,
    })
}

#[uniffi::export]
pub fn qr_code(text: String) -> Result<QrCode> {
    let m = qr::encode(&text)?;
    Ok(QrCode { size: m.size, dark: m.dark })
}

/// An address's pattern: dots on a 5 × 5 grid in one of eight colours.
#[derive(uniffi::Record)]
pub struct AddressPattern {
    /// 15 bits: 5 rows of 3 columns, bit `3 × row + column`.
    pub cells: u16,
    /// 0–7, an index into the app's fixed palette.
    pub colour: u8,
    /// The 5 × 5 grid as drawn, row by row (mirrored): `true` for a dot.
    pub dots: Vec<bool>,
}

/// The pattern for an address, to show beside it on Receive and when paying it.
#[uniffi::export]
pub fn address_pattern(address: String) -> AddressPattern {
    let p = pattern::address_pattern(&address);
    AddressPattern { cells: p.cells, colour: p.colour, dots: p.dots() }
}

/// Whether text someone is typing is an amount above zero.
#[uniffi::export]
pub fn is_amount(text: String) -> bool {
    amount::parse(&text).is_ok_and(|a| a > 0)
}

#[uniffi::export]
pub fn default_nodes(network: Network) -> Vec<String> {
    fob_core::Network::from(network).default_nodes().iter().map(|n| (*n).to_owned()).collect()
}

#[derive(uniffi::Record)]
pub struct NodeCheck {
    pub url: String,
    pub reachable: bool,
    pub right_network: bool,
    pub synced: bool,
    pub height: u64,
    pub latency_ms: u64,
    pub blocks_per_second: f64,
    pub error: Option<String>,
}

/// Checks nodes at once: reachable, right network, synced, and how fast.
#[uniffi::export(async_runtime = "tokio")]
pub async fn check_nodes(network: Network, urls: Vec<String>) -> Vec<NodeCheck> {
    fob_core::node::check_all(network.into(), &urls)
        .await
        .into_iter()
        .map(|c| NodeCheck {
            url: c.url,
            reachable: c.reachable,
            right_network: c.right_network,
            synced: c.synced,
            height: c.height,
            latency_ms: c.latency_ms,
            blocks_per_second: c.blocks_per_second,
            error: c.error,
        })
        .collect()
}
