//! [`fob_core`] for the apps, through UniFFI. This crate only converts types
//! and turns fob-core's async calls into Kotlin `suspend` functions; every
//! decision about the wallet is made in fob-core, so the apps stay thin.

use std::{path::PathBuf, sync::Arc};

use fob_core::{activity, amount, not_ready, qr, receipt, uri};

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
            E::Send(_) | E::Storage(_) | E::NoSuchPayment => FobError::Other { reason },
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
}

#[derive(uniffi::Record)]
pub struct Activity {
    pub tx: String,
    pub outgoing: bool,
    /// "0.1421", without a sign.
    pub amount_text: String,
    pub fee_text: Option<String>,
    pub to: Option<String>,
    /// Seconds since the epoch.
    pub time: u64,
    pub confirmations: u64,
    /// Sent but not yet in a block.
    pub pending: bool,
    /// For its own screen: "Sending", "Not ready · 3 of 10", "Confirmed · 24".
    pub status_text: String,
    pub receipt: Option<Receipt>,
}

fn activity(a: fob_core::Activity) -> Activity {
    Activity {
        outgoing: a.direction == fob_core::Direction::Out,
        amount_text: xmr(a.amount),
        fee_text: a.fee.map(amount::format),
        pending: a.height.is_none(),
        status_text: activity::status(&a),
        tx: a.tx,
        to: a.to,
        time: a.time,
        confirmations: a.confirmations,
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

    /// Asks the node for new blocks and its pool now (pull to refresh);
    /// returns when that round is over, or after 20 s.
    pub async fn sync_now(&self) {
        self.inner.sync_now().await;
    }

    /// Builds a payment for the confirm screen. `amount` is text as typed;
    /// `receipt_json` comes from the scanned `PaymentRequest`.
    pub async fn prepare(&self, to: String, amount: String, receipt_json: Option<String>) -> Result<Prepared> {
        let atomic = amount::parse(&amount)?;
        let p = self.inner.prepare(&to, atomic, receipt_json).await?;
        Ok(Prepared {
            id: p.id,
            to: p.to,
            amount_text: exact(p.amount),
            fee_text: amount::format(p.fee),
            total_text: amount::format(p.amount + p.fee),
            receipt: p.receipt.map(Into::into),
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

/// A payment code to show: address and, if given, an amount as typed.
#[uniffi::export]
pub fn payment_code(address: String, amount: Option<String>) -> Result<String> {
    let amount = amount.filter(|a| !a.trim().is_empty()).map(|a| amount::parse(&a)).transpose()?;
    Ok(uri::payment_uri(&address, amount))
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
