//! [`fob_core`] for the apps, through UniFFI. This crate only converts types
//! and turns fob-core's async calls into Kotlin `suspend` functions; every
//! decision about the wallet is made in fob-core, so the apps stay thin.

use std::{path::PathBuf, sync::Arc};

use fob_core::{
    activity, amount, contacts, not_ready, pattern,
    price::{self, Source as _},
    profile,
    qr, receipt, split, uri,
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
            E::BadSeed | E::BadAddress(_) | E::BadKey | E::BadAmount | E::BadCode | E::BadEndpoint(_) | E::WrongNetwork { .. } | E::Split(_) | E::Contact(_) | E::Request(_) | E::Profile(_) => {
                FobError::Invalid { reason }
            }
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
    /// What the balance is worth now, "$412.06"; only with prices on and a
    /// current price.
    pub fiat_text: Option<String>,
    /// Show [`fiat_text`](Self::fiat_text) first, the XMR under it.
    pub fiat_first: bool,
    /// Under Send's sheet: "1.8592 XMR ready to spend · $308.70".
    pub ready_line: String,
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

/// One row of Home's Activity, ready to draw: a payment, a request or a
/// share of your profile.
#[derive(uniffi::Record)]
pub struct ActivityRow {
    /// The payment, for its own screen; none for a request or a share.
    pub activity: Option<Activity>,
    /// The request, for its own screen; none for a payment.
    pub request: Option<u64>,
    /// The share of your profile, whose code it shows again.
    pub share: Option<u64>,
    /// The shop, who was paid, "From Sam", "Received", "Request to Sam",
    /// "Shared your profile", "Via your profile".
    pub title: String,
    /// "Receipt, 3 items · 09:12", "Pizza · not ready · 3 of 10 · 08:40".
    pub line: String,
    /// With its sign: "+0.5", "−0.1421"; a request's ask: "$80.00" (blank
    /// for any amount).
    pub amount_text: String,
    /// The receipt's total: "$13.50"; "asked" under a request's.
    pub fiat_text: Option<String>,
}

impl From<activity::Row> for ActivityRow {
    fn from(r: activity::Row) -> Self {
        ActivityRow { activity: r.activity.map(activity), request: r.request, share: r.share, title: r.title, line: r.line, amount_text: r.amount, fiat_text: r.fiat }
    }
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
    /// "Received · 08:40", "Change · Corner Café", "Split up · 08:31".
    pub source: String,
    /// "3 of 10"
    pub confirmations_text: String,
    /// "09:02"
    pub ready: String,
    /// "in 14 min"
    pub eta: String,
}

/// A line of Home's opened card.
#[derive(uniffi::Record)]
pub struct NotReadySum {
    /// "Ready to spend", "Not ready · 3 coins", "All of it".
    pub label: String,
    /// "1.8592"
    pub amount_text: String,
    /// Worth now, with prices on: "$308.70".
    pub fiat_text: Option<String>,
}

/// The money not ready yet beside what is, for Home's card.
#[derive(uniffi::Record)]
pub struct NotReady {
    /// Ready to spend and not ready, in atomic units: the bar's two parts.
    pub ready: u64,
    pub not_ready: u64,
    /// "1.8592 ready", in bold.
    pub ready_text: String,
    /// "0.6221 not yet"
    pub not_yet_text: String,
    /// "09:02"
    pub all_ready: String,
    /// "All ready about 09:02 · in 14 min"
    pub all_ready_line: String,
    /// The folded card for TalkBack.
    pub description: String,
    /// Ready to spend, not ready, all of it.
    pub sums: Vec<NotReadySum>,
    /// Weighted by amount, 0 to 1: how much of the not-ready part is green.
    pub progress: f32,
    /// Soonest ready first.
    pub pieces: Vec<NotReadyPiece>,
}

impl From<not_ready::NotReady> for NotReady {
    fn from(n: not_ready::NotReady) -> Self {
        NotReady {
            ready: n.ready,
            not_ready: n.total,
            ready_text: n.ready_text,
            not_yet_text: n.not_yet_text,
            all_ready: n.all_ready,
            all_ready_line: n.all_ready_line,
            description: n.description,
            sums: n.sums.into_iter().map(|s| NotReadySum { label: s.label, amount_text: s.amount, fiat_text: s.fiat }).collect(),
            progress: n.progress,
            pieces: n.pieces.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<not_ready::PieceView> for NotReadyPiece {
    fn from(p: not_ready::PieceView) -> Self {
        NotReadyPiece {
            amount_text: p.amount,
            fiat_text: p.fiat,
            confirmations: p.confirmations,
            needed: p.needed,
            progress: p.progress,
            source: p.source,
            confirmations_text: p.confirmations_text,
            ready: p.ready,
            eta: p.eta,
        }
    }
}

/// A coin ready to spend.
#[derive(uniffi::Record)]
pub struct Coin {
    /// "1.2"
    pub amount_text: String,
    /// Worth now, with prices on: "$199.27".
    pub fiat_text: Option<String>,
    /// "Received · 22 Oct", "Split up · 08:31", "Change · Ridge Hardware".
    pub source: String,
}

/// The Coins screen, every word from Rust.
#[derive(uniffi::Record)]
pub struct Coins {
    /// "Your 2.4813 XMR is held as 8 coins. A payment uses whole coins; …"
    pub intro: String,
    /// Ready and not ready, in atomic units: the bar's two parts.
    pub ready: u64,
    pub not_ready: u64,
    /// The not-ready part's weighted progress, 0 to 1.
    pub progress: f32,
    /// "Ready · 5 coins"
    pub ready_title: String,
    /// "1.8592 · $308.70"
    pub ready_sum: String,
    /// Largest first.
    pub ready_coins: Vec<Coin>,
    /// "Not ready · 3 coins"
    pub not_ready_title: String,
    /// "all by 09:02"
    pub not_ready_sum: String,
    /// Soonest ready first.
    pub not_ready_coins: Vec<NotReadyPiece>,
}

impl From<not_ready::Coins> for Coins {
    fn from(c: not_ready::Coins) -> Self {
        Coins {
            intro: c.intro,
            ready: c.ready,
            not_ready: c.not_ready,
            progress: c.progress,
            ready_title: c.ready_title,
            ready_sum: c.ready_sum,
            ready_coins: c.ready_coins.into_iter().map(|c| Coin { amount_text: c.amount, fiat_text: c.fiat, source: c.source }).collect(),
            not_ready_title: c.not_ready_title,
            not_ready_sum: c.not_ready_sum,
            not_ready_coins: c.not_ready_coins.into_iter().map(Into::into).collect(),
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
    /// Set for a split: what the confirm and done screens say instead.
    pub split: Option<SplitSummary>,
}

/// A split on the confirm screen, every word from Rust.
#[derive(uniffi::Record)]
pub struct SplitSummary {
    /// "Split up"
    pub title: String,
    /// "This wallet"
    pub to: String,
    /// Largest first.
    pub pieces: Vec<SplitLine>,
    /// "12 pieces"
    pub pieces_text: String,
    /// "Swipe to split into 12 pieces"
    pub swipe: String,
    /// "12 pieces on their way back to this wallet"
    pub done: String,
}

/// One kind of piece in a split: "$20 × 4", "0.2667 XMR".
#[derive(uniffi::Record)]
pub struct SplitLine {
    pub label: String,
    pub amount_text: String,
}

/// What Split up opens with.
#[derive(uniffi::Record)]
pub struct SplitSuggestion {
    /// Pieces of each note, in the order of `SplitView::tiles`.
    pub counts: Vec<u32>,
    /// "Picked from your last 30 days: most payments were $5–$25."
    pub text: String,
}

/// A note on Split up's grid.
#[derive(uniffi::Record)]
pub struct SplitTile {
    /// "$20", "0.25"
    pub label: String,
    pub count: u32,
    /// "× 4", or "tap to add"
    pub count_text: String,
    /// "Add a $20 piece, 4 so far"
    pub add_label: String,
    /// "Remove a $20 piece"
    pub remove_label: String,
}

/// A block of Split up's bar.
#[derive(uniffi::Record)]
pub struct SplitBlock {
    /// Share of the width, 0 to 1.
    pub width: f32,
    /// Share of the tallest's height, by the square root of its value.
    pub height: f32,
    /// What's left as one piece, drawn lighter.
    pub left_over: bool,
}

/// The Split up screen, every word from Rust.
#[derive(uniffi::Record)]
pub struct SplitView {
    pub title: String,
    pub intro: String,
    pub tiles: Vec<SplitTile>,
    /// What the screen shows, as Rust read it: keep these.
    pub counts: Vec<u32>,
    pub pieces: u32,
    pub bar: Vec<SplitBlock>,
    /// "11 pieces · $220"
    pub summary: Option<String>,
    /// "+ $88.70 left as one piece"
    pub left_over: Option<String>,
    /// "Split into 11 pieces", or "Add a piece to split".
    pub button: String,
    pub ready: bool,
    /// "Network fee about $0.03 · pieces are set in XMR at today's price"
    pub footer: String,
}

impl From<split::View> for SplitView {
    fn from(v: split::View) -> Self {
        SplitView {
            title: v.title,
            intro: v.intro,
            tiles: v
                .tiles
                .into_iter()
                .map(|t| SplitTile { label: t.label, count: t.count, count_text: t.count_text, add_label: t.add_label, remove_label: t.remove_label })
                .collect(),
            counts: v.counts,
            pieces: v.pieces,
            bar: v.bar.into_iter().map(|b| SplitBlock { width: b.width, height: b.height, left_over: b.left_over }).collect(),
            summary: v.summary,
            left_over: v.left_over,
            button: v.button,
            ready: v.ready,
            footer: v.footer,
        }
    }
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

/// A prepared payment or split for the confirm screen, with what it's
/// worth now.
async fn prepared(wallet: &fob_core::Wallet, p: fob_core::Prepared) -> Prepared {
    let split = p.pieces.map(|n| SplitSummary {
        title: split::TITLE.into(),
        to: split::TO.into(),
        pieces: p.split.iter().map(|(label, amount)| SplitLine { label: label.clone(), amount_text: amount.clone() }).collect(),
        pieces_text: split::pieces_text(n),
        swipe: split::swipe_text(n),
        done: split::done_text(n),
    });
    Prepared {
        id: p.id,
        to: p.to,
        amount_text: exact(p.amount),
        fee_text: amount::format(p.fee),
        total_text: amount::format(p.amount + p.fee),
        amount_fiat: wallet.worth_now(p.amount).await,
        fee_fiat: wallet.worth_now(p.fee).await,
        total_fiat: wallet.worth_now(p.amount + p.fee).await,
        receipt: p.receipt.map(Into::into),
        split,
    }
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
            fiat_text: self.inner.worth_now(b.total).await,
            fiat_first: self.inner.price_settings().await.fiat_first,
            ready_line: self.inner.ready_line().await,
        }
    }

    /// Every payment and request, grouped by the phone's day, newest first.
    /// `now` is seconds since the epoch, `utc_offset` the phone's offset in
    /// seconds, `hours_24` its clock setting.
    pub async fn activity_days(&self, now: u64, utc_offset: i32, hours_24: bool) -> Vec<ActivityDay> {
        let clock = activity::Clock { now, utc_offset, hours_24 };
        self.inner
            .activity_days(clock)
            .await
            .into_iter()
            .map(|day| ActivityDay { label: day.label, rows: day.rows.into_iter().map(Into::into).collect() })
            .collect()
    }

    /// The money not ready to spend yet, piece by piece, with about when
    /// it will be; `None` when all of it is. The clock as for `activity_days`.
    pub async fn not_ready(&self, now: u64, utc_offset: i32, hours_24: bool) -> Option<NotReady> {
        self.inner.not_ready(activity::Clock { now, utc_offset, hours_24 }).await.map(Into::into)
    }

    /// Every coin, ready to spend or not yet, for the Coins screen; the
    /// clock as for `activity_days`.
    pub async fn coins(&self, now: u64, utc_offset: i32, hours_24: bool) -> Coins {
        self.inner.coins(activity::Clock { now, utc_offset, hours_24 }).await.into()
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

    /// Builds a payment for the confirm screen. `amount` is text as typed
    /// in the amount field, in money when `in_fiat` (converted at the
    /// current price) or XMR. `note` (typed on Send, or the payment code's
    /// description) is kept with the payment. `code` is the payment code
    /// the form was filled from, as read: its name goes with the payment
    /// while `to` is its address, its receipt while the amount is its too.
    pub async fn prepare(&self, to: String, amount: String, in_fiat: bool, note: Option<String>, code: Option<String>) -> Result<Prepared> {
        let p = self.inner.prepare_typed(&to, &amount, in_fiat, note, code.as_deref()).await?;
        Ok(prepared(&self.inner, p).await)
    }

    /// The people this wallet paid before, the most recent first, for
    /// Send's sheet; the clock as for `activity_days`.
    pub async fn paid_before(&self, now: u64, utc_offset: i32, hours_24: bool) -> Vec<PaidBefore> {
        self.inner
            .paid_before(activity::Clock { now, utc_offset, hours_24 })
            .await
            .into_iter()
            .map(|p| PaidBefore { address: p.address, name: p.name, line: p.line })
            .collect()
    }

    /// What Split up opens with: pieces learned from the last 30 days'
    /// payments, or an even mix, and the words saying which.
    pub async fn split_suggestion(&self) -> SplitSuggestion {
        let s = self.inner.split_suggestion().await;
        SplitSuggestion { counts: s.counts, text: s.text }
    }

    /// Split up's screen for `counts` pieces of each note.
    pub async fn split_view(&self, counts: Vec<u32>) -> SplitView {
        self.inner.split_view(&counts).await.into()
    }

    /// One more piece of note `index`; throws why not (15 pieces at most,
    /// or not enough ready to spend with the fee).
    pub async fn split_add(&self, counts: Vec<u32>, index: u32) -> Result<Vec<u32>> {
        Ok(self.inner.split_add(&counts, index as usize).await?)
    }

    /// One fewer piece of note `index`.
    pub async fn split_remove(&self, counts: Vec<u32>, index: u32) -> Vec<u32> {
        self.inner.split_remove(&counts, index as usize).await
    }

    /// Asks the node its fee rate, so Split up's fee estimate is the node's.
    pub async fn refresh_fee_rate(&self) {
        self.inner.refresh_fee_rate().await;
    }

    /// Builds the split for the confirm screen: `counts` pieces of each
    /// note paid back to this wallet, the rest as change.
    pub async fn prepare_split(&self, counts: Vec<u32>) -> Result<Prepared> {
        let p = self.inner.prepare_split(&counts).await?;
        Ok(prepared(&self.inner, p).await)
    }

    /// Keeps the note typed on Receive for the address at `index`, so
    /// payments to it say what they were for.
    pub async fn remember_receive_note(&self, index: u32, note: String) -> Result<()> {
        Ok(self.inner.remember_receive_note(index, &note).await?)
    }

    /// An amount field for `text`, typed in money (`in_fiat`) or XMR; in
    /// money only with prices on and a current price.
    pub async fn amount_entry(&self, text: String, in_fiat: bool) -> AmountEntry {
        let f = self.inner.amount_field(&text, in_fiat).await;
        AmountEntry {
            in_fiat: f.in_fiat,
            sign: f.sign,
            unit: f.unit,
            can_switch: f.can_switch,
            line: f.line,
            label: f.label,
            switch_label: f.switch_label,
            sendable: f.sendable,
            for_text: f.for_text,
            for_in_fiat: f.for_in_fiat,
        }
    }

    /// An amount field's text switched between money and XMR (`to_fiat`):
    /// the typed text converted, or blank.
    pub async fn convert_amount(&self, text: String, to_fiat: bool) -> String {
        self.inner.convert_amount(&text, to_fiat).await
    }

    /// A payment code to show: the address and, if typed, an amount (in
    /// money when `in_fiat`) and a note for the payer. Throws when the
    /// amount isn't one.
    pub async fn receive_code(&self, address: String, amount: Option<String>, in_fiat: bool, note: Option<String>) -> Result<ReceiveCode> {
        let r = self.inner.receive_code(&address, amount.as_deref(), in_fiat, note.as_deref()).await?;
        Ok(ReceiveCode { uri: r.uri, headline: r.headline, detail: r.detail })
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

    /// Contacts by name, those whose name or note has `search` in it (any
    /// case; blank for all); the clock as for `activity_days`.
    pub async fn contacts(&self, search: String, now: u64, utc_offset: i32, hours_24: bool) -> Vec<ContactRow> {
        self.inner.contacts(&search, activity::Clock { now, utc_offset, hours_24 }).await.into_iter().map(Into::into).collect()
    }

    /// The contacts Send's sheet offers (with a send address, the most
    /// recently paid first, at most 3).
    pub async fn contacts_on_send(&self, now: u64, utc_offset: i32, hours_24: bool) -> Vec<ContactRow> {
        self.inner.contacts_on_send(activity::Clock { now, utc_offset, hours_24 }).await.into_iter().map(Into::into).collect()
    }

    /// Addresses paid that are no contact's, newest first, for "Paid, not
    /// saved".
    pub async fn paid_not_saved(&self, now: u64, utc_offset: i32, hours_24: bool) -> Vec<PaidBefore> {
        self.inner
            .paid_not_saved(activity::Clock { now, utc_offset, hours_24 })
            .await
            .into_iter()
            .map(|p| PaidBefore { address: p.address, name: p.name, line: p.line })
            .collect()
    }

    pub async fn contact(&self, id: u64) -> Option<Contact> {
        self.inner.contact(id).await.map(Into::into)
    }

    /// A contact's card, or none once it's deleted.
    pub async fn contact_card(&self, id: u64, now: u64, utc_offset: i32, hours_24: bool) -> Option<ContactCard> {
        let c = self.inner.contact_card(id, activity::Clock { now, utc_offset, hours_24 }).await?;
        Some(ContactCard {
            contact: c.contact.into(),
            send_label: c.send_label,
            send_short: c.send_short,
            send_button: c.send_button,
            requests_title: c.requests_title,
            requests_note: c.requests_note,
            requests: c
                .requests
                .into_iter()
                .map(|r| ContactRequest { id: r.id, address: r.address, note: r.note, line: r.line, state: r.state, received: r.received, asked: r.asked })
                .collect(),
            also_paid_at: c.also_paid_at.into_iter().map(|a| ContactPaidAt { address: a.address, short: a.short, line: a.line }).collect(),
            also_note: c.also_note,
            paid_title: c.paid_title,
            paid: c
                .paid
                .into_iter()
                .map(|p| ContactPayment { activity: activity(p.activity), title: p.title, when: p.when, amount_text: p.amount, fiat_text: p.fiat })
                .collect(),
            paid_empty: c.paid_empty,
        })
    }

    /// Saves a new contact; throws what's wrong with it.
    pub async fn add_contact(&self, draft: ContactDraft) -> Result<Contact> {
        Ok(self.inner.add_contact(draft.into()).await?.into())
    }

    /// Saves a contact's name, note, photo and send address.
    pub async fn edit_contact(&self, id: u64, draft: ContactDraft) -> Result<Contact> {
        Ok(self.inner.edit_contact(id, draft.into()).await?.into())
    }

    /// A contact's send address, scanned or pasted (an address or a
    /// payment code).
    pub async fn set_contact_send_address(&self, id: u64, text: String) -> Result<Contact> {
        Ok(self.inner.set_contact_send_address(id, &text).await?.into())
    }

    /// Keeps an address paid, from a code named for the contact, with it.
    pub async fn add_contact_paid_at(&self, id: u64, address: String) -> Result<Contact> {
        Ok(self.inner.add_contact_paid_at(id, &address).await?.into())
    }

    /// What the Paid screen offers for the payment `tx`: Add to Sam, or
    /// Save as a contact; none for a split or an address that's already a
    /// contact's.
    pub async fn save_offer(&self, tx: String) -> Option<SaveOffer> {
        let o = self.inner.save_offer(&tx).await?;
        Some(SaveOffer {
            address: o.address,
            add_to: o.add_to.map(Into::into),
            button: o.button,
            other: o.other,
            named: o.named,
            footnote: o.footnote,
            draft: o.draft.into(),
        })
    }

    /// New contact for an address under "Paid, not saved", filled as the
    /// Paid screen would have.
    pub async fn new_contact_for(&self, address: String) -> ContactDraft {
        self.inner.new_contact_for(&address).await.into()
    }

    /// Deletes a contact; payments to its addresses stay in Activity.
    pub async fn delete_contact(&self, id: u64) -> Result<()> {
        Ok(self.inner.delete_contact(id).await?)
    }

    /// Makes a request, when Share or Copy is pressed on Receive: a new
    /// address, for `contact` (or no one), asking `amount` (in money when
    /// `in_fiat`) with `note`. Throws when the amount isn't one, or the
    /// contact was deleted.
    pub async fn make_request(&self, contact: Option<u64>, amount: Option<String>, in_fiat: bool, note: Option<String>) -> Result<MadeRequest> {
        let (r, code) = self.inner.make_request(contact, amount.as_deref(), in_fiat, note.as_deref()).await?;
        Ok(MadeRequest {
            id: r.id,
            address: r.address,
            index: r.subaddress,
            contact: r.contact,
            code: ReceiveCode { uri: code.uri, headline: code.headline, detail: code.detail },
        })
    }

    /// A request's code as it was made, for "Show the code again"; none
    /// once it's deleted.
    pub async fn request_code(&self, id: u64) -> Option<ReceiveCode> {
        self.inner.request_code(id).await.map(|c| ReceiveCode { uri: c.uri, headline: c.headline, detail: c.detail })
    }

    /// A request's own screen, or none once it's deleted; the clock as for
    /// `activity_days`.
    pub async fn request(&self, id: u64, now: u64, utc_offset: i32, hours_24: bool) -> Option<RequestDetail> {
        let d = self.inner.request(id, activity::Clock { now, utc_offset, hours_24 }).await?;
        Some(RequestDetail {
            id: d.request.id,
            address: d.request.address,
            headline: d.headline,
            asked_line: d.asked_line,
            when: d.when,
            received_label: d.received_label,
            received: d.received,
            received_any: d.received_any,
            address_label: d.address_label,
            address_short: d.address_short,
            payments: d.payments.into_iter().map(Into::into).collect(),
            payments_empty: d.payments_empty,
            delete_title: d.delete_title,
            delete_body: d.delete_body,
        })
    }

    /// Deletes a request: it leaves Activity and its contact's card; its
    /// address still receives, what comes later shown without a name.
    pub async fn delete_request(&self, id: u64) -> Result<()> {
        Ok(self.inner.delete_request(id).await?)
    }

    /// Share's sheet ("Who is this for?"): every contact by name, each
    /// with its requests in a line ("2 requests paid, 1 waiting").
    pub async fn request_contacts(&self) -> Vec<ContactRow> {
        self.inner.request_contacts().await.into_iter().map(Into::into).collect()
    }

    /// Signs and broadcasts; returns the transaction hash.
    pub async fn send(&self, id: u64) -> Result<String> {
        Ok(self.inner.send(id).await?)
    }

    pub async fn cancel(&self, id: u64) {
        self.inner.cancel(id).await;
    }

    /// Your profile, once it's set up.
    pub async fn profile(&self) -> Option<Profile> {
        self.inner.profile().await.map(Into::into)
    }

    /// Settings' row: "Your profile" with "Henry · name and photo, share as
    /// a code", or "Set up your profile".
    pub async fn profile_row(&self) -> ProfileRow {
        let (title, line) = self.inner.profile_row().await;
        ProfileRow { title, line }
    }

    /// Under Receive's note while requests carry your name: "The code names
    /// you as “Henry” · change in Settings"; none while they don't.
    pub async fn receive_name_line(&self) -> Option<String> {
        self.inner.receive_name_line().await
    }

    /// Saves your name and photo (a WebP that `contact_photo_fits`); throws
    /// what's wrong with them.
    pub async fn set_profile(&self, name: String, photo: Option<Vec<u8>>) -> Result<Profile> {
        Ok(self.inner.set_profile(&name, photo).await?.into())
    }

    /// "Name in payment requests", on or off.
    pub async fn set_name_in_requests(&self, on: bool) -> Result<()> {
        Ok(self.inner.set_name_in_requests(on).await?)
    }

    /// "Share my profile": a share on a new address, kept from now on
    /// however briefly its code shows, with its code.
    pub async fn share_profile(&self) -> Result<ProfileShare> {
        let (share, code) = self.inner.share_profile().await?;
        Ok(ProfileShare { id: share.id, address: share.address, code, can_touch: true })
    }

    /// A share's code again, from Your shares or Activity; none for no such
    /// share.
    pub async fn share_code(&self, id: u64) -> Option<ProfileShare> {
        let (share, code, can_touch) = self.inner.share_code(id).await?;
        Some(ProfileShare { id: share.id, address: share.address, code, can_touch })
    }

    /// Copy link or Share pressed on a share's screen.
    pub async fn share_went_out(&self, id: u64, kind: ShareKind) -> Result<()> {
        Ok(self.inner.share_went_out(id, kind.into()).await?)
    }

    /// Touch phones, while share `id`'s screen is in front: the swap the
    /// card service and the reader both talk to, on that share's address.
    pub async fn start_swap(&self, id: u64) -> Result<Arc<TouchSwap>> {
        Ok(Arc::new(TouchSwap { inner: self.inner.start_swap(id).await? }))
    }

    /// The swap is over, done or the screen closed: kept as a share when
    /// anything went out; their profile when it came (once).
    pub async fn end_swap(&self, swap: Arc<TouchSwap>) -> Result<Option<Swapped>> {
        Ok(self.inner.end_swap(&swap.inner).await?.map(|(share, scanned, sure)| {
            let (title, line) = profile::swapped_words(&scanned.code.name, sure);
            let text = uri::profile_uri(&scanned.code.address, &scanned.code.name, scanned.code.photo.as_deref());
            Swapped { share: share.id, title, line, text, profile: scanned.into() }
        }))
    }

    /// Your shares, newest first; the clock as for `activity_days`.
    pub async fn profile_shares(&self, now: u64, utc_offset: i32, hours_24: bool) -> Vec<ShareLine> {
        let clock = activity::Clock { now, utc_offset, hours_24 };
        self.inner
            .profile_shares(clock)
            .await
            .into_iter()
            .map(|l| ShareLine { id: l.id, address: l.address, kind: l.kind.into(), title: l.title, when: l.when, received: l.received, any: l.any })
            .collect()
    }

    /// A scanned profile code ("Another Fob scans the code"), with the
    /// contact it would change; throws when it isn't a profile.
    pub async fn scanned_profile(&self, text: String) -> Result<ScannedProfile> {
        Ok(self.inner.scanned_profile(&text).await?.into())
    }

    /// "Add contact" on a scanned profile, or "Add as new contact" once
    /// asked (`as_new`). Throws when its address is a contact's, or its
    /// name is (or looks like) one's and it wasn't asked.
    pub async fn add_contact_from_profile(&self, text: String, as_new: bool) -> Result<Contact> {
        Ok(self.inner.add_contact_from_profile(&text, as_new).await?.into())
    }

    /// "Update Sam" on a scanned profile, once asked.
    pub async fn update_contact_from_profile(&self, id: u64, text: String) -> Result<Contact> {
        Ok(self.inner.update_contact_from_profile(id, &text).await?.into())
    }
}

/// Your name and photo.
#[derive(uniffi::Record)]
pub struct Profile {
    pub name: String,
    /// The avatar's letter without a photo.
    pub initial: String,
    /// A small WebP.
    pub photo: Option<Vec<u8>>,
    /// "Name in payment requests"
    pub name_in_requests: bool,
}

impl From<profile::Profile> for Profile {
    fn from(p: profile::Profile) -> Self {
        Profile { initial: p.initial(), name: p.name, photo: p.photo, name_in_requests: p.name_in_requests }
    }
}

/// Settings' row for the profile.
#[derive(uniffi::Record)]
pub struct ProfileRow {
    /// "Your profile", "Set up your profile"
    pub title: String,
    /// "Henry · name and photo, share as a code"
    pub line: String,
}

/// How a share went out.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ShareKind {
    CodeShown,
    LinkCopied,
    LinkShared,
    /// By touching phones, with whom (none when theirs didn't come).
    TouchedPhones { name: Option<String> },
}

impl From<ShareKind> for profile::ShareKind {
    fn from(k: ShareKind) -> Self {
        match k {
            ShareKind::CodeShown => profile::ShareKind::CodeShown,
            ShareKind::LinkCopied => profile::ShareKind::LinkCopied,
            ShareKind::LinkShared => profile::ShareKind::LinkShared,
            ShareKind::TouchedPhones { name } => profile::ShareKind::TouchedPhones { name },
        }
    }
}

impl From<profile::ShareKind> for ShareKind {
    fn from(k: profile::ShareKind) -> Self {
        match k {
            profile::ShareKind::CodeShown => ShareKind::CodeShown,
            profile::ShareKind::LinkCopied => ShareKind::LinkCopied,
            profile::ShareKind::LinkShared => ShareKind::LinkShared,
            profile::ShareKind::TouchedPhones { name } => ShareKind::TouchedPhones { name },
        }
    }
}

/// Touch phones' swap for one Share my profile screen. The app's card
/// service and reader hand it the bytes NFC brings and send what it gives
/// back; the protocol is all here.
#[derive(uniffi::Object)]
pub struct TouchSwap {
    inner: Arc<fob_core::swap::Session>,
}

/// What the reader sends next.
#[derive(uniffi::Enum)]
pub enum ReaderStep {
    /// Send this command, and give back the answer.
    Send { apdu: Vec<u8> },
    /// Both phones have each other's profile.
    Done,
    /// This touch took nothing; say why.
    Failed { reason: String },
}

#[uniffi::export]
impl TouchSwap {
    /// The card's answer to a reader's command.
    pub fn card_answer(&self, apdu: Vec<u8>) -> Vec<u8> {
        self.inner.card_answer(&apdu)
    }

    /// The card's link went (the service was deactivated).
    pub fn card_lost(&self) {
        self.inner.card_lost()
    }

    /// The reader found a card: the first command, or none once over.
    pub fn reader_start(&self) -> Option<Vec<u8>> {
        self.inner.reader_start()
    }

    /// The card's answer to the last command: what to do next.
    pub fn reader_next(&self, response: Vec<u8>) -> ReaderStep {
        match self.inner.reader_next(&response) {
            fob_core::swap::Step::Send(apdu) => ReaderStep::Send { apdu },
            fob_core::swap::Step::Done(_) => ReaderStep::Done,
            fob_core::swap::Step::Failed(problem) => ReaderStep::Failed { reason: problem.text() },
        }
    }

    /// The reader's link went (the card moved away, or timed out).
    pub fn reader_lost(&self) {
        self.inner.reader_lost()
    }

    /// Midway through an exchange: don't switch roles now.
    pub fn busy(&self) -> bool {
        self.inner.busy()
    }

    /// Their profile came, as card or reader.
    pub fn swapped(&self) -> bool {
        self.inner.got().is_some()
    }
}

/// What the card service answers while no Share my profile is in front:
/// "not available", whoever asks.
#[uniffi::export]
pub fn swap_not_available() -> Vec<u8> {
    fob_core::swap::NOT_AVAILABLE.to_vec()
}

/// Touched phones: both got each other's profile.
#[derive(uniffi::Record)]
pub struct Swapped {
    /// The share it's kept as.
    pub share: u64,
    /// "Swapped with Sam"
    pub title: String,
    /// "Sam got your profile, and you got Sam's."
    pub line: String,
    /// Their profile as a code, for Add contact and Pay.
    pub text: String,
    pub profile: ScannedProfile,
}

/// A share of your profile and its code.
#[derive(uniffi::Record)]
pub struct ProfileShare {
    pub id: u64,
    /// Its own address, used for nothing else.
    pub address: String,
    /// `monero:<address>?recipient_name=…&recipient_avatar=…`
    pub code: String,
    /// Touch phones still swaps on it: the share Share my profile just
    /// made, only shown so far. A share opened again never does.
    pub can_touch: bool,
}

/// A share on Your profile.
#[derive(uniffi::Record)]
pub struct ShareLine {
    pub id: u64,
    pub address: String,
    pub kind: ShareKind,
    /// "Code shown", "Link copied"
    pub title: String,
    /// "Today 09:15", "28 Oct"
    pub when: String,
    /// "+$4.98", or "nothing yet".
    pub received: String,
    /// Something came: green.
    pub any: bool,
}

/// What a scanned profile would change in a contact.
#[derive(uniffi::Record)]
pub struct ProfileChange {
    /// "Name", "Photo", "Send address"
    pub label: String,
    pub now: String,
    pub new: String,
}

/// The contact a scanned profile matches, asked about before it changes.
#[derive(uniffi::Record)]
pub struct ProfileClash {
    pub contact: Contact,
    /// "You have a contact named Henry."
    pub why: String,
    /// Empty when nothing would change.
    pub changes: Vec<ProfileChange>,
    /// Draw both photos.
    pub photo_changes: bool,
    /// "Update Sam? Their name and send address change to the profile's."
    pub ask: String,
    /// "Update Sam", or "Open Sam" when nothing would change.
    pub update: String,
    /// "Keep as is"
    pub keep: String,
    /// "Add as new contact", for a name only; none when the address is
    /// the contact's (never two contacts with one send address from a
    /// profile).
    pub add_new: Option<String>,
}

/// Another Fob's profile, scanned.
#[derive(uniffi::Record)]
pub struct ScannedProfile {
    pub address: String,
    pub name: String,
    pub photo: Option<Vec<u8>>,
    pub initial: String,
    /// "89Hn2 … Tq4Lp … 6wKeR"
    pub address_short: String,
    /// "A Fob profile. The name and photo are Henry's own, not checked by Fob."
    pub note: String,
    /// "Pay Henry"
    pub pay: String,
    /// "Add contact"
    pub add: String,
    pub clash: Option<ProfileClash>,
}

impl From<profile::Scanned> for ScannedProfile {
    fn from(s: profile::Scanned) -> Self {
        ScannedProfile {
            address: s.code.address,
            name: s.code.name,
            photo: s.code.photo,
            initial: s.initial,
            address_short: s.address_short,
            note: s.note,
            pay: s.pay,
            add: s.add,
            clash: s.clash.map(|c| ProfileClash {
                contact: c.contact.into(),
                why: c.why,
                changes: c.changes.into_iter().map(|c| ProfileChange { label: c.label, now: c.now, new: c.new }).collect(),
                photo_changes: c.photo_changes,
                ask: c.ask,
                update: c.update,
                keep: c.keep,
                add_new: c.add_new,
            }),
        }
    }
}

/// Whether a scanned code is a profile (a name, maybe a photo, nothing to
/// pay) rather than a payment code; one with a bad photo isn't.
#[uniffi::export]
pub fn is_profile_code(network: Network, text: String) -> bool {
    uri::parse_profile(network.into(), &text).is_ok()
}

/// Why a scanned code isn't a profile, for the contacts' scanner.
#[uniffi::export]
pub fn profile_code_problem(network: Network, text: String) -> Option<String> {
    uri::parse_profile(network.into(), &text).err().map(|e| e.to_string())
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
}

/// An amount field (toggle B), every word from Rust.
#[derive(uniffi::Record)]
pub struct AmountEntry {
    /// Typing in money: there's a price and the field is switched to it.
    pub in_fiat: bool,
    /// Before what's typed: "$", "€", "ɱ" for XMR; none for a currency
    /// without a symbol.
    pub sign: Option<String>,
    /// The chip: "USD", "XMR".
    pub unit: String,
    /// The chip switches: there's a price.
    pub can_switch: bool,
    /// Under the field: "≈ 0.1204 XMR · 1 XMR = $166.10", "That isn't an
    /// amount".
    pub line: Option<String>,
    /// "Amount in US dollars"
    pub label: String,
    /// "Amount in US dollars. Switch to XMR"
    pub switch_label: Option<String>,
    /// What's typed comes to more than nothing: Send can review it.
    pub sendable: bool,
    /// The text and unit it was made for: it stands for the field only
    /// while they're what the field holds. Nothing is sent, shared or
    /// copied from one that's behind (the chip just tapped, say).
    pub for_text: String,
    pub for_in_fiat: bool,
}

/// Someone this wallet paid before, on Send's sheet.
#[derive(uniffi::Record)]
pub struct PaidBefore {
    pub address: String,
    /// "Corner Café", "86Ux … 7kQa"
    pub name: String,
    /// "Lunch money · 27 Oct", "Today · $24.50"
    pub line: String,
}

/// Someone this wallet pays, kept in the wallet file.
#[derive(uniffi::Record)]
pub struct Contact {
    pub id: u64,
    pub name: String,
    /// The avatar's letter without a photo: "S".
    pub initial: String,
    pub note: Option<String>,
    /// A small WebP.
    pub photo: Option<Vec<u8>>,
    /// Where you pay them.
    pub send_address: Option<String>,
    /// Addresses of codes named for them that were paid; never their send
    /// address.
    pub also_paid_at: Vec<String>,
}

impl From<contacts::Contact> for Contact {
    fn from(c: contacts::Contact) -> Self {
        Contact { initial: c.initial(), id: c.id, name: c.name, note: c.note, photo: c.photo, send_address: c.send_address, also_paid_at: c.also_paid_at }
    }
}

/// A contact as the edit card saves it.
#[derive(uniffi::Record)]
pub struct ContactDraft {
    pub name: String,
    pub note: Option<String>,
    /// A WebP that `contact_photo_fits`.
    pub photo: Option<Vec<u8>>,
    /// An address or a payment code, as scanned or pasted; blank for none.
    pub send_address: Option<String>,
    /// For a new contact saved after paying a code named for them.
    pub also_paid_at: Vec<String>,
}

impl From<contacts::Draft> for ContactDraft {
    fn from(d: contacts::Draft) -> Self {
        ContactDraft { name: d.name, note: d.note, photo: d.photo, send_address: d.send_address, also_paid_at: d.also_paid_at }
    }
}

/// What the Paid screen offers for a payment, every word from Rust.
#[derive(uniffi::Record)]
pub struct SaveOffer {
    /// The address paid.
    pub address: String,
    /// The contact the code's name matches: the button adds the address to
    /// them.
    pub add_to: Option<Contact>,
    /// "Add to Sam", "Save as a contact"
    pub button: String,
    /// "A different Sam? New contact", beside Add to.
    pub other: Option<String>,
    /// "to a code named “Sam”"
    pub named: Option<String>,
    /// For a named code: "The name comes from their code, not checked by
    /// Fob. …"
    pub footnote: Option<String>,
    /// New contact, filled.
    pub draft: ContactDraft,
}

/// Under the send address on New contact, when it's saved with addresses
/// from codes named for them: "Paid at 84Bv9 … 99fQ2 from their code: kept
/// with them, not as their send address."
#[uniffi::export]
pub fn contact_paid_at_note(addresses: Vec<String>) -> Option<String> {
    contacts::paid_at_note(&addresses)
}

impl From<ContactDraft> for contacts::Draft {
    fn from(d: ContactDraft) -> Self {
        contacts::Draft { name: d.name, note: d.note, photo: d.photo, send_address: d.send_address, also_paid_at: d.also_paid_at }
    }
}

/// A contact in a list.
#[derive(uniffi::Record)]
pub struct ContactRow {
    pub id: u64,
    pub name: String,
    pub initial: String,
    pub photo: Option<Vec<u8>>,
    pub send_address: Option<String>,
    /// "Lunch money · paid 27 Oct" on Contacts, "Lunch · 27 Oct" on Send.
    pub line: String,
}

impl From<contacts::Row> for ContactRow {
    fn from(r: contacts::Row) -> Self {
        ContactRow { id: r.id, name: r.name, initial: r.initial, photo: r.photo, send_address: r.send_address, line: r.line }
    }
}

/// An address a contact was also paid at, on their card.
#[derive(uniffi::Record)]
pub struct ContactPaidAt {
    pub address: String,
    /// "84Bv9 … 2kLmQ … 99fQ2"
    pub short: String,
    /// "Not their send address · paid once, 25 Oct"
    pub line: String,
}

/// A payment to a contact, or from them, on their card.
#[derive(uniffi::Record)]
pub struct ContactPayment {
    pub activity: Activity,
    /// Its note, the shop, or "Sent"; "From Sam".
    pub title: String,
    /// "Today", "27 Oct"; "Concert tickets · Today".
    pub when: String,
    /// "−0.1204", "+0.5"
    pub amount_text: String,
    pub fiat_text: Option<String>,
}

/// One of a contact's requests, on their card.
#[derive(uniffi::Record)]
pub struct ContactRequest {
    pub id: u64,
    /// Its address, for the pattern.
    pub address: String,
    /// "Concert tickets", or "Request".
    pub note: String,
    /// "Address 9 · today 08:30"
    pub line: String,
    /// "Received $83.03" (green), or "Waiting".
    pub state: String,
    pub received: bool,
    /// "asked $80.00"
    pub asked: Option<String>,
}

/// A request just made on Receive: its new address and code.
#[derive(uniffi::Record)]
pub struct MadeRequest {
    pub id: u64,
    pub address: String,
    /// Its address's index: "Address 9".
    pub index: u32,
    pub contact: Option<u64>,
    pub code: ReceiveCode,
}

/// A request's own screen (Round 5 › "A request: no edits, can be
/// deleted"), every word from Rust.
#[derive(uniffi::Record)]
pub struct RequestDetail {
    pub id: u64,
    pub address: String,
    /// "$80.00", "0.5 XMR", "Any amount"
    pub headline: String,
    /// "asked of Sam · Concert tickets"
    pub asked_line: String,
    /// "Today 08:30 · 0.4817 XMR at the time"
    pub when: String,
    /// "Received"
    pub received_label: String,
    /// "$83.03 · 0.5 XMR", or "Waiting".
    pub received: String,
    pub received_any: bool,
    /// "Address 9"
    pub address_label: String,
    /// "84Kp7 … Q2mXa … t9VbL"
    pub address_short: String,
    /// "Payments to it", newest first.
    pub payments: Vec<ActivityRow>,
    /// "Nothing paid to it yet."
    pub payments_empty: Option<String>,
    /// "Delete this request?"
    pub delete_title: String,
    pub delete_body: String,
}

/// A contact's card, every word from Rust.
#[derive(uniffi::Record)]
pub struct ContactCard {
    pub contact: Contact,
    /// "Send address · you pay here", "No send address yet"
    pub send_label: String,
    /// "86Uxm … 3EkYw … e7kQa"
    pub send_short: Option<String>,
    /// "Change", "Add"
    pub send_button: String,
    /// "Requests · Sam pays you"
    pub requests_title: String,
    /// "3 addresses"
    pub requests_note: String,
    /// Their requests, newest first; the section shows only with one.
    pub requests: Vec<ContactRequest>,
    pub also_paid_at: Vec<ContactPaidAt>,
    /// "codes named Sam"
    pub also_note: String,
    /// "Paid to Sam", "Payments with Sam"
    pub paid_title: String,
    pub paid: Vec<ContactPayment>,
    /// "Nothing paid to Sam yet."
    pub paid_empty: Option<String>,
}

/// The edit card's words for the name typed so far.
#[derive(uniffi::Record)]
pub struct ContactEditTexts {
    /// "New contact", "Edit contact"
    pub title: String,
    pub address_hint: String,
    /// "Delete Sam?"
    pub delete_title: String,
    pub delete_body: String,
}

#[uniffi::export]
pub fn contact_edit_texts(name: String, new: bool) -> ContactEditTexts {
    let t = contacts::edit_texts(&name, new);
    ContactEditTexts { title: t.title, address_hint: t.address_hint, delete_title: t.delete_title, delete_body: t.delete_body }
}

/// The send address scanner's title and hint.
#[derive(uniffi::Record)]
pub struct SendAddressScan {
    /// "Sam's send address", "Their send address"
    pub title: String,
    pub hint: String,
}

#[uniffi::export]
pub fn send_address_scan(name: Option<String>) -> SendAddressScan {
    let t = contacts::scan_texts(name.as_deref());
    SendAddressScan { title: t.title, hint: t.hint }
}

/// A send address found by the scanner or pasted.
#[derive(uniffi::Record)]
pub struct FoundAddress {
    pub address: String,
    /// "Found an address"
    pub title: String,
    /// "87Wq2 … Lm4Zx … 2bNcT"
    pub short: String,
    /// "This replaces Sam's current send address (…). Check the pattern
    /// with Sam."
    pub warning: Option<String>,
}

/// What `text` (an address or a payment code, scanned or pasted) gives as
/// `name`'s send address, `current` being the one it has; throws when it
/// isn't an address of this network.
#[uniffi::export]
pub fn found_send_address(network: Network, name: Option<String>, current: Option<String>, text: String) -> Result<FoundAddress> {
    let f = contacts::found(network.into(), name.as_deref(), current.as_deref(), &text)?;
    Ok(FoundAddress { address: f.address, title: f.title, short: f.short, warning: f.warning })
}

/// An address on a contact's card or edit card: "86Uxm … 3EkYw … e7kQa".
#[uniffi::export]
pub fn contact_address(address: String) -> String {
    contacts::three_part(&address)
}

/// Whether a photo the app encoded is one Fob keeps (a WebP of at most
/// 1536 bytes); the app lowers the quality, then the size, until it is.
#[uniffi::export]
pub fn contact_photo_fits(photo: Vec<u8>) -> bool {
    contacts::photo_fits(&photo)
}

/// The avatar's letter for a name being typed: "S", "?" for none.
#[uniffi::export]
pub fn contact_initial(name: String) -> String {
    contacts::initial(&name)
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

/// An address in a field, its first and last five: "84Bv9 … 99fQ2".
#[uniffi::export]
pub fn short_address(address: String) -> String {
    uri::short(&address)
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
