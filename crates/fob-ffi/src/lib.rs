//! [`fob_core`] for the apps, through UniFFI. This crate only converts types
//! and turns fob-core's async calls into Kotlin `suspend` functions; every
//! decision about the wallet is made in fob-core, so the apps stay thin.

use std::{path::PathBuf, sync::Arc};

use fob_core::{activity, amount, contacts, keys, not_ready, pattern, price, profile, qr, receipt, split, uri};

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
            E::Send(_) | E::Storage(_) | E::NoSuchPayment | E::Price(_) | E::PriceFailed(_) => FobError::Other { reason },
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
    /// What the balance is worth now, "$412.06"; only with a currency besides XMR and a
    /// price from within the day.
    pub fiat_text: Option<String>,
    /// That price's age once it's missed a refresh: "2 h old".
    pub fiat_age: Option<String>,
    /// The worth with its age, where it sits small: "$412.06 · 2 h old".
    pub fiat_line: Option<String>,
    /// Tapping the balance swaps XMR and the default currency: one
    /// besides XMR, with its worth to show. With XMR, never.
    pub can_swap: bool,
    /// Show [`fiat_text`](Self::fiat_text) first, the XMR at the line's
    /// end; only while it can swap.
    pub fiat_first: bool,
    /// Under Send's sheet: "1.8592 XMR ready to spend · $308.70", with the
    /// price's age once old ("… · 2 h old").
    pub ready_line: String,
}

/// Receive's own address and its index (a note typed on Receive is kept
/// with it).
#[derive(uniffi::Record)]
pub struct ReceiveAddress {
    pub address: String,
    pub index: u32,
}

/// Everything the screens open with, read together.
#[derive(uniffi::Record)]
pub struct Screens {
    /// Settings' money and price rows, and the amount fields' blank entry.
    pub prices: PriceSettings,
    pub balance: Balance,
    /// Home's Activity, by the phone's day, newest first.
    pub activity: Vec<ActivityDay>,
    /// The money not ready to spend yet; none when all of it is.
    pub not_ready: Option<NotReady>,
    /// Home's card while a new wallet's words aren't written down.
    pub reminder: Option<BackupReminder>,
    /// Settings' Node line: "Working · node.example:38081".
    pub node_line: String,
    /// Settings' Backup and keys line: "Seed phrase, wallet code, keys".
    pub backup_line: String,
    /// Receive's own address, while it has one to show; none while one is
    /// still to be handed out, which only Receive opening does.
    pub receive: Option<ReceiveAddress>,
    /// Every contact by name.
    pub contacts: Vec<ContactRow>,
    /// Contacts' "Paid, not saved".
    pub unsaved: Vec<PaidBefore>,
    /// The contacts Send's sheet offers.
    pub send_contacts: Vec<ContactRow>,
    /// Who was paid before, for Send's sheet while there's no contact.
    pub paid_before: Vec<PaidBefore>,
    pub coins: Coins,
    /// The price toast while lookups fail.
    pub price_toast: Option<PriceToast>,
}

/// Home's card while a new wallet's seed phrase isn't written down.
#[derive(uniffi::Record)]
pub struct BackupReminder {
    /// "Back up your seed phrase"
    pub title: String,
    /// "Your 16 words aren't written down yet. …"
    pub line: String,
    /// Over the words wherever they're shown until they're written down:
    /// "These 16 words are your wallet. Write them down in order …"
    pub seed_warning: String,
}

/// Backup and keys, every word from fob-core, and Settings' line for it.
#[derive(uniffi::Record)]
pub struct BackupHub {
    /// The warning card: "Anyone who sees these can take or watch your money. …"
    pub warning: String,
    /// Under "Back up": Seed phrase (not on a view-only wallet), Wallet code.
    pub backup: Vec<HubRow>,
    /// Under "For other wallets": Keys.
    pub other: Vec<HubRow>,
    /// Under the cards: "The seed phrase is all you need. …"
    pub footer: String,
    /// Settings' Backup and keys row: "Seed phrase, wallet code, keys".
    pub settings_line: String,
}

#[derive(uniffi::Record)]
pub struct HubRow {
    pub item: HubItem,
    /// "Seed phrase"
    pub title: String,
    /// "16 words that restore everything · backed up"
    pub line: String,
}

/// What a row of Backup and keys opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HubItem {
    SeedPhrase,
    WalletCode,
    Keys,
}

impl From<keys::HubRow> for HubRow {
    fn from(r: keys::HubRow) -> Self {
        let item = match r.item {
            keys::HubItem::SeedPhrase => HubItem::SeedPhrase,
            keys::HubItem::WalletCode => HubItem::WalletCode,
            keys::HubItem::Keys => HubItem::Keys,
        };
        HubRow { item, title: r.title, line: r.line }
    }
}

/// A card of Keys: "Secret: keep to yourself" or "Public".
#[derive(uniffi::Record)]
pub struct KeyGroup {
    pub title: String,
    pub keys: Vec<KeyRow>,
}

/// One key, in full, with its Copy button's label.
#[derive(uniffi::Record)]
pub struct KeyRow {
    /// "Secret spend key"
    pub title: String,
    /// Sets the badge's colour.
    pub badge: KeyBadge,
    /// "Spends money", "Sees payments", "Public"
    pub badge_text: String,
    /// "With the view key, restores this wallet anywhere."; none for the public keys.
    pub line: Option<String>,
    /// 64 hex digits, the whole address, or "1690400 · 2 Sep 2026".
    pub value: String,
    /// What Copy puts on the clipboard: the value, the restore height without its date.
    pub clip: String,
    /// "Copy the secret spend key"
    pub copy: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum KeyBadge {
    SpendsMoney,
    SeesPayments,
    Public,
}

impl From<keys::KeyGroup> for KeyGroup {
    fn from(g: keys::KeyGroup) -> Self {
        let keys = g
            .keys
            .into_iter()
            .map(|k| KeyRow {
                title: k.title,
                badge: match k.badge {
                    keys::KeyBadge::SpendsMoney => KeyBadge::SpendsMoney,
                    keys::KeyBadge::SeesPayments => KeyBadge::SeesPayments,
                    keys::KeyBadge::Public => KeyBadge::Public,
                },
                badge_text: k.badge_text,
                line: k.line,
                value: k.value.to_string(),
                clip: k.clip.to_string(),
                copy: k.copy,
            })
            .collect();
        KeyGroup { title: g.title, keys }
    }
}

/// One row of the "Default currency" list.
#[derive(uniffi::Record)]
pub struct CurrencyRow {
    /// The code to choose ("AUD"), or none for XMR.
    pub code: Option<String>,
    /// In the circle: "A$", "CHF", "ɱ".
    pub sign: String,
    /// "Australian dollar", "Monero (XMR)".
    pub name: String,
    /// "AUD · Kraken, CoinGecko", "Fob never looks up a price".
    pub line: String,
    pub chosen: bool,
}

/// A titled part of the list: "MONERO", "THIS PHONE’S", "ALL 45".
#[derive(uniffi::Record)]
pub struct CurrencyGroup {
    pub title: String,
    pub rows: Vec<CurrencyRow>,
}

/// The "Default currency" list.
#[derive(uniffi::Record)]
pub struct CurrencyList {
    pub groups: Vec<CurrencyGroup>,
    /// Instead of the groups when a search finds nothing.
    pub empty: Option<String>,
}

impl From<price::CurrencyList> for CurrencyList {
    fn from(l: price::CurrencyList) -> Self {
        CurrencyList {
            groups: l
                .groups
                .into_iter()
                .map(|g| CurrencyGroup {
                    title: g.title,
                    rows: g.rows.into_iter().map(|r| CurrencyRow { code: r.code, sign: r.sign, name: r.name, line: r.line, chosen: r.chosen }).collect(),
                })
                .collect(),
            empty: l.empty,
        }
    }
}

/// Whose prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PriceProvider {
    Kraken,
    CoinGecko,
}

impl From<PriceProvider> for price::Provider {
    fn from(p: PriceProvider) -> Self {
        match p {
            PriceProvider::Kraken => price::Provider::Kraken,
            PriceProvider::CoinGecko => price::Provider::CoinGecko,
        }
    }
}

impl From<price::Provider> for PriceProvider {
    fn from(p: price::Provider) -> Self {
        match p {
            price::Provider::Kraken => PriceProvider::Kraken,
            price::Provider::CoinGecko => PriceProvider::CoinGecko,
        }
    }
}

/// Price from's checks once they're in.
#[derive(uniffi::Record)]
pub struct PriceChecks {
    pub lines: Vec<PriceCheck>,
    /// What TalkBack says as they come in: "Kraken A$845.10, CoinGecko not answering".
    pub summary: String,
}

/// One provider's line on Price from, checked as it opens.
#[derive(uniffi::Record)]
pub struct PriceCheck {
    pub provider: PriceProvider,
    /// "Kraken"
    pub name: String,
    /// It answers over the web, and over Tor.
    pub web: bool,
    pub tor: bool,
    pub chosen: bool,
    /// "A$845.10"; none when it failed or wasn't asked.
    pub rate: Option<String>,
    /// "Not answering", "Busy, try again soon".
    pub failure: Option<String>,
    pub can_pick: bool,
    /// For TalkBack: "Kraken, web only, A$845.10".
    pub label: String,
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

/// One way to ask, on Price from's "Ask over".
#[derive(uniffi::Record)]
pub struct PriceSourceChoice {
    pub transport: PriceTransport,
    /// "Web", "Tor"
    pub title: String,
    /// "The provider sees this phone's IP address", "Kraken doesn't answer
    /// over Tor".
    pub line: String,
    /// It can be picked; greyed otherwise, the line saying why.
    pub available: bool,
    pub chosen: bool,
}

/// Settings' money and price rows.
#[derive(uniffi::Record)]
pub struct PriceSettings {
    /// "Default currency": "USD", or none for XMR.
    pub currency: Option<String>,
    /// That row's value, short enough never to wrap: "AUD · A$", "CHF", "XMR".
    pub currency_text: String,
    /// What TalkBack says for it: "Australian dollar", "Monero (XMR)".
    pub currency_name: String,
    /// Price from's intro: "Prices in " **"Australian dollars"** ", checked
    /// as this page opens. …", or with XMR one sentence, unbolded.
    pub intro_before: String,
    pub intro_bold: Option<String>,
    pub intro_after: String,
    pub provider: PriceProvider,
    /// Price from's row: "Kraken · over the internet", or while lookups
    /// fail the toast's line ("Kraken didn't answer · using the 14:02
    /// price").
    pub source_text: String,
    /// Lookups are failing: the row is red, with Try again.
    pub failing: bool,
    pub sources: Vec<PriceSourceChoice>,
    /// When the price shown was asked, none without one: new with every
    /// new price, so screens converting money (Receive's code) convert again.
    pub price_at: Option<u64>,
    /// The amount field as Send and Receive open it, blank (in money while
    /// there's a price): drawn whole on their first frame, before their
    /// own `amount_entry` answers.
    pub blank_entry: AmountEntry,
}

/// The price toast while lookups fail, every word from Rust.
#[derive(uniffi::Record)]
pub struct PriceToast {
    /// New each time Rust says to show it; the same while the background
    /// meets the same run of failures again (newer words only).
    pub id: u64,
    /// "Couldn't get the USD price", "Couldn't get 29 Oct's USD price"
    pub title: String,
    /// "Kraken didn't answer · using the 14:02 price"
    pub line: String,
    /// While Retry asks: "Trying again…", "Asking Kraken for the USD price".
    pub retrying_title: String,
    pub retrying_line: String,
}

impl From<price::alert::Toast> for PriceToast {
    fn from(t: price::alert::Toast) -> Self {
        PriceToast { id: t.id, title: t.title, line: t.line, retrying_title: t.retrying_title, retrying_line: t.retrying_line }
    }
}

/// What the toast's Retry came to.
#[derive(uniffi::Enum)]
pub enum PriceRetry {
    /// It worked: "USD price updated", "1 XMR = $166.10", shown for 2 s.
    Updated { title: String, line: String },
    /// It failed again.
    Failed { toast: PriceToast },
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
    /// Worth now, with a currency besides XMR: "$108.44".
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
}

/// A line of Home's opened card.
#[derive(uniffi::Record)]
pub struct NotReadySum {
    /// "Ready to spend", "Not ready · 3 coins", "All of it".
    pub label: String,
    /// "1.8592"
    pub amount_text: String,
    /// Worth now, with a currency besides XMR: "$308.70".
    pub fiat_text: Option<String>,
    /// The line for TalkBack: "Ready to spend, 1.8592 XMR, $308.70".
    pub description: String,
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
    /// How old the price the sums are worth at is, once it's missed a
    /// refresh: "23 h old".
    pub price_age: Option<String>,
    /// Weighted by amount, 0 to 1: how much of the not-ready part is green.
    pub progress: f32,
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
            sums: n.sums.into_iter().map(|s| NotReadySum { description: s.description(), label: s.label, amount_text: s.amount, fiat_text: s.fiat }).collect(),
            price_age: n.price_age,
            progress: n.progress,
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
        }
    }
}

/// A coin ready to spend.
#[derive(uniffi::Record)]
pub struct Coin {
    /// "1.2"
    pub amount_text: String,
    /// Worth now, with a currency besides XMR: "$199.27".
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
    /// The bar for TalkBack: "1.8592 XMR ready, 0.6221 XMR not yet, 37%
    /// of the way".
    pub bar_description: String,
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
    /// Split up can be pressed: a coin is ready to spend in a wallet that
    /// can spend. Otherwise it shows greyed out.
    pub can_split: bool,
}

impl From<not_ready::Coins> for Coins {
    fn from(c: not_ready::Coins) -> Self {
        Coins {
            intro: c.intro,
            ready: c.ready,
            not_ready: c.not_ready,
            progress: c.progress,
            bar_description: c.bar_description,
            ready_title: c.ready_title,
            ready_sum: c.ready_sum,
            ready_coins: c.ready_coins.into_iter().map(|c| Coin { amount_text: c.amount, fiat_text: c.fiat, source: c.source }).collect(),
            not_ready_title: c.not_ready_title,
            not_ready_sum: c.not_ready_sum,
            not_ready_coins: c.not_ready_coins.into_iter().map(Into::into).collect(),
            can_split: c.can_split,
        }
    }
}

#[derive(uniffi::Record)]
pub struct SyncStatus {
    pub scanned: u64,
    pub tip: u64,
    pub synced: bool,
    /// 0 to 1: how far the chip's ring goes.
    pub progress: f32,
    pub error: Option<String>,
    /// How Home's sync chip looks.
    pub chip_look: ChipLook,
    /// "Starting", "6 min left", "Synced", "Can't reach node", "Can't save".
    pub chip_text: String,
    /// What TalkBack reads for the chip: "Catching up, 34%, about 6 minutes left".
    pub chip_label: String,
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

/// Home's sync chip: a ring while catching up, a green dot once synced, a
/// red one when the node doesn't answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChipLook {
    Ring,
    Synced,
    Offline,
}

/// One of the sync card's rows.
#[derive(uniffi::Record)]
pub struct SyncRow {
    /// "Blocks".
    pub label: String,
    /// "3,360 of 9,880 · 28/s".
    pub value: String,
    /// What TalkBack reads: "Blocks: 3,360 of 9,880, 28 a second".
    pub spoken: String,
}

/// The card the sync chip opens.
#[derive(uniffi::Record)]
pub struct SyncDetails {
    /// "Catching up", "Synced", "Can't reach node", "Can't save the wallet".
    pub title: String,
    /// "34%", while catching up.
    pub percent: Option<String>,
    /// The bar, while catching up.
    pub progress: Option<f32>,
    /// "Time left" · "about 4 min", "Blocks" · "3,360 of 9,880 · 28/s"…
    pub rows: Vec<SyncRow>,
    /// "Fob keeps trying on its own." while it's failing.
    pub note: Option<String>,
    /// What went wrong, as the node or network said it.
    pub error: Option<String>,
}

impl From<fob_core::catch_up::Details> for SyncDetails {
    fn from(d: fob_core::catch_up::Details) -> Self {
        SyncDetails {
            title: d.title,
            percent: d.percent,
            progress: d.progress,
            rows: d.rows.into_iter().map(|r| SyncRow { label: r.label, value: r.value, spoken: r.spoken }).collect(),
            note: d.note,
            error: d.error,
        }
    }
}

impl From<fob_core::SyncStatus> for SyncStatus {
    fn from(s: fob_core::SyncStatus) -> Self {
        let chip = s.chip();
        SyncStatus {
            scanned: s.scanned,
            tip: s.tip,
            synced: s.synced,
            progress: s.progress,
            error: s.error,
            chip_look: match chip.look {
                fob_core::catch_up::Look::Ring => ChipLook::Ring,
                fob_core::catch_up::Look::Synced => ChipLook::Synced,
                fob_core::catch_up::Look::Offline => ChipLook::Offline,
            },
            chip_text: chip.text,
            chip_label: chip.label,
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
    /// What they're worth now, with a currency besides XMR: "$20.00".
    pub amount_fiat: Option<String>,
    pub fee_fiat: Option<String>,
    /// With the price's age once it's missed a refresh: "$20.03 · 23 h old".
    pub total_fiat: Option<String>,
    /// How old the price they're worth at is, once it's missed a refresh:
    /// "23 h old".
    pub fiat_age: Option<String>,
    pub receipt: Option<Receipt>,
    /// Set for a split: what the confirm and done screens say instead.
    pub split: Option<SplitSummary>,
}

/// What the confirm screen shows while the fee is worked out: a
/// [`Prepared`] without its fee, total or id.
#[derive(uniffi::Record)]
pub struct PaymentPreview {
    pub to: String,
    pub amount_text: String,
    /// What it's worth now, with a currency besides XMR: "$20.00".
    pub amount_fiat: Option<String>,
    pub receipt: Option<Receipt>,
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

/// Split up's Auto button, offered with 3 or more payments in the last 30
/// days to learn from.
#[derive(uniffi::Record)]
pub struct SplitAuto {
    /// What tapping it fills: pieces of each note, in the order of `SplitView::tiles`.
    pub counts: Vec<u32>,
    /// "Auto"
    pub label: String,
    /// What TalkBack reads: "Auto, fill coins like your last 30 days".
    pub spoken: String,
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
    /// Auto in the top bar; none without enough payments to learn from.
    pub auto: Option<SplitAuto>,
    /// The counts are Auto's mix: Auto is drawn lit and read as selected.
    pub is_auto: bool,
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
    /// "Network fee about $0.03", with " · 23 h old price" once the price is old
    pub footer: String,
}

impl From<split::View> for SplitView {
    fn from(v: split::View) -> Self {
        SplitView {
            title: v.title,
            intro: v.intro,
            auto: v.auto.map(|a| SplitAuto { counts: a.counts, label: a.label, spoken: a.spoken }),
            is_auto: v.is_auto,
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
    let age = wallet.price_age_now().await;
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
        // The total says how old the price is, once it's missed a refresh.
        total_fiat: match (wallet.worth_now(p.amount + p.fee).await, &age) {
            (Some(total), Some(age)) => Some(format!("{total} · {age}")),
            (total, _) => total,
        },
        fiat_age: age,
        receipt: p.receipt.map(Into::into),
        split,
    }
}

/// What [`FobWallet::screens`] reads, one by one.
impl FobWallet {
    /// Home's reminder while a wallet made here hasn't had its seed phrase
    /// written down; none once it has.
    async fn backup_reminder(&self) -> Option<BackupReminder> {
        self.inner.backup_reminder().await.map(|r| BackupReminder { title: r.title, line: r.line, seed_warning: r.seed_warning })
    }

    /// The balance at `now` (seconds since the epoch), which dates the price.
    async fn balance(&self, now: u64) -> Balance {
        let b = self.inner.balance().await;
        let (worth, order) = self.inner.balance_worth(b.total, now).await;
        Balance {
            total: b.total,
            unlocked: b.unlocked,
            total_text: xmr(b.total),
            unlocked_text: xmr(b.unlocked),
            fiat_line: worth.as_ref().map(|(w, old)| match old {
                Some(old) => format!("{w} · {old}"),
                None => w.clone(),
            }),
            fiat_age: worth.as_ref().and_then(|(_, old)| old.clone()),
            fiat_text: worth.map(|(w, _)| w),
            can_swap: order.can_swap,
            fiat_first: order.fiat_first,
            ready_line: self.inner.ready_line(now).await,
        }
    }

    /// The money not ready to spend yet, piece by piece, with about when
    /// it will be; `None` when all of it is. The clock as for `activity_days`.
    async fn not_ready(&self, now: u64, utc_offset: i32, hours_24: bool) -> Option<NotReady> {
        self.inner.not_ready(activity::Clock { now, utc_offset, hours_24 }).await.map(Into::into)
    }

    /// Every coin, ready to spend or not yet, for the Coins screen; the
    /// clock as for `activity_days`.
    async fn coins(&self, now: u64, utc_offset: i32, hours_24: bool) -> Coins {
        self.inner.coins(activity::Clock { now, utc_offset, hours_24 }).await.into()
    }

    /// Settings' money and price rows, and Price from's "Ask over". The
    /// clock as for `activity_days`, for when a failing row's price is from.
    async fn price_settings(&self, now: u64, utc_offset: i32, hours_24: bool) -> PriceSettings {
        let s = self.inner.price_settings().await;
        let sources = price::transport_rows(&s)
            .into_iter()
            .map(|r| PriceSourceChoice { transport: r.transport.into(), title: r.title.into(), line: r.line, available: r.available, chosen: r.chosen })
            .collect();
        let (source_text, failing) = self.inner.price_from_line(activity::Clock { now, utc_offset, hours_24 }).await;
        let intro = price::price_from_intro(&s);
        PriceSettings {
            currency_text: price::default_currency_text(&s),
            currency_name: price::default_currency_name(&s),
            intro_before: intro.before,
            intro_bold: intro.bold,
            intro_after: intro.after,
            currency: s.currency.clone(),
            provider: s.provider.into(),
            failing: failing && s.code().is_some(),
            source_text,
            sources,
            price_at: self.inner.price_at(now).await,
            blank_entry: self.inner.blank_amount_field(activity::Clock { now, utc_offset, hours_24 }).await.into(),
        }
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

    /// "I've written them down": the reminder goes.
    pub async fn backup_done(&self) -> Result<()> {
        Ok(self.inner.backup_done().await?)
    }

    pub async fn wallet_code(&self) -> String {
        self.inner.wallet_code().await.to_string()
    }

    /// Backup and keys' words (no secrets), and Settings' line for its row;
    /// `phone_lock`: the phone has a screen lock set (its warning promises
    /// the lock only then).
    pub async fn backup_hub(&self, phone_lock: bool) -> BackupHub {
        let hub = self.inner.backup_hub(phone_lock).await;
        BackupHub {
            warning: hub.warning,
            backup: hub.backup.into_iter().map(Into::into).collect(),
            other: hub.other.into_iter().map(Into::into).collect(),
            footer: hub.footer,
            settings_line: hub.settings_line,
        }
    }

    /// Keys: every key in full, secret ones first; a view-only wallet has
    /// no secret spend key.
    pub async fn keys_page(&self) -> Vec<KeyGroup> {
        self.inner.keys_page().await.into_iter().map(Into::into).collect()
    }

    /// Receive's address as it opens, handing out a new one when the last
    /// went to a request, a split or a share (or there's none yet).
    pub async fn receive_address(&self) -> Result<ReceiveAddress> {
        let address = self.inner.receive_address().await?;
        Ok(ReceiveAddress { address, index: self.inner.receive_index().await })
    }

    /// Receive's New address: a fresh one nobody has seen.
    pub async fn new_receive_address(&self) -> Result<ReceiveAddress> {
        let address = self.inner.new_receive_address().await?;
        Ok(ReceiveAddress { address, index: self.inner.receive_index().await })
    }

    /// Everything the screens open with, in one call and from one look at
    /// the activity, so a screen draws whole on its first frame. The clock
    /// as for `payment`; `phone_lock` as for `backup_hub`.
    pub async fn screens(&self, now: u64, utc_offset: i32, hours_24: bool, phone_lock: bool) -> Screens {
        let clock = activity::Clock { now, utc_offset, hours_24 };
        let lists = self.inner.lists(clock).await;
        let paid = |p: Vec<activity::PaidBefore>| p.into_iter().map(|p| PaidBefore { address: p.address, name: p.name, line: p.line }).collect();
        Screens {
            prices: self.price_settings(now, utc_offset, hours_24).await,
            balance: self.balance(now).await,
            activity: lists.days.into_iter().map(|day| ActivityDay { label: day.label, rows: day.rows.into_iter().map(Into::into).collect() }).collect(),
            not_ready: self.not_ready(now, utc_offset, hours_24).await,
            reminder: self.backup_reminder().await,
            node_line: self.inner.node_line().await,
            backup_line: self.inner.backup_hub(phone_lock).await.settings_line,
            receive: self.inner.shown_receive_address().await.map(|(address, index)| ReceiveAddress { address, index }),
            contacts: lists.contacts.into_iter().map(Into::into).collect(),
            unsaved: paid(lists.unsaved),
            send_contacts: lists.on_send.into_iter().map(Into::into).collect(),
            paid_before: paid(lists.paid_before),
            coins: self.coins(now, utc_offset, hours_24).await,
            price_toast: self.inner.price_toast(clock).await.map(Into::into),
        }
    }

    pub async fn node(&self) -> NodeSetting {
        self.inner.node().await.into()
    }

    /// Settings' Node line: "Working · node.example:38081".
    pub async fn node_line(&self) -> String {
        self.inner.node_line().await
    }

    /// Switches to `url`; `self_signed` accepts its certificate unverified.
    pub async fn set_node(&self, url: String, self_signed: bool) -> Result<()> {
        Ok(self.inner.set_node(&url, self_signed).await?)
    }

    /// Runs the Node screen's checks on `node`, telling `observer` as each
    /// one finishes; gives the last report.
    pub async fn check_node(&self, node: NodeSetting, observer: Arc<dyn NodeCheckObserver>) -> NodeReport {
        self.inner.check_node(&node.into(), &|report| observer.on_progress(report.clone().into())).await.into()
    }

    /// The other nodes to offer (the network's, then `added`), each saying
    /// "Checking…" until [`check_nodes`](Self::check_nodes) answers.
    pub async fn other_nodes(&self, added: Vec<NodeSetting>) -> Vec<NodeChoice> {
        let network = self.inner.network();
        let added: Vec<_> = added.into_iter().map(Into::into).collect();
        self.inner
            .other_nodes(&added)
            .await
            .into_iter()
            .map(|n| {
                let report = fob_core::node::Report::pending(network, &n);
                NodeChoice::new(&report, report.line(false))
            })
            .collect()
    }

    /// Checks the other nodes at once: each one's line and whether it can be used.
    pub async fn check_nodes(&self, nodes: Vec<NodeSetting>) -> Vec<NodeChoice> {
        let nodes: Vec<_> = nodes.into_iter().map(Into::into).collect();
        self.inner.check_nodes(&nodes).await.iter().map(|(report, line)| NodeChoice::new(report, line.clone())).collect()
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

    pub fn status(&self) -> SyncStatus {
        self.inner.status().into()
    }

    /// The card Home's sync chip opens, for the status now; the clock as
    /// for `activity_days`.
    pub fn sync_details(&self, now: u64, utc_offset: i32, hours_24: bool) -> SyncDetails {
        self.inner.sync_details(activity::Clock { now, utc_offset, hours_24 }).into()
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

    /// The price toast while lookups fail; none once one works. Show it
    /// when its `id` is one not shown (or swiped away) yet. The clock as
    /// for `activity_days`.
    pub async fn price_toast(&self, now: u64, utc_offset: i32, hours_24: bool) -> Option<PriceToast> {
        self.inner.price_toast(activity::Clock { now, utc_offset, hours_24 }).await.map(Into::into)
    }

    /// The toast's Retry: asks for the price again and says what came of it.
    pub async fn retry_prices(&self, now: u64, utc_offset: i32, hours_24: bool) -> PriceRetry {
        match self.inner.retry_prices(activity::Clock { now, utc_offset, hours_24 }).await {
            fob_core::Retried::Updated { title, line } => PriceRetry::Updated { title, line },
            fob_core::Retried::Failed(toast) => PriceRetry::Failed { toast: toast.into() },
        }
    }

    /// The "Default currency" list, the chosen one ticked: `phone` is
    /// the phone's own currency code (its locale's), `query` what's typed
    /// in the search.
    pub async fn currency_list(&self, phone: Option<String>, query: String) -> CurrencyList {
        self.inner.currency_list(phone.as_deref(), &query).await.into()
    }

    /// "Default currency" `code` (a row's), or XMR (none), when Fob
    /// never asks for a price. Returns words to show when the provider had
    /// to change for it: "Kraken doesn't price New Zealand dollars, so Fob
    /// now asks CoinGecko".
    pub async fn set_currency(&self, code: Option<String>) -> Result<Option<String>> {
        Ok(self.inner.set_currency(code.as_deref()).await?)
    }

    /// Prices from `provider`, one that prices the currency. Returns words
    /// to show when the transport had to change for it.
    pub async fn set_price_provider(&self, provider: PriceProvider) -> Result<Option<String>> {
        Ok(self.inner.set_price_provider(provider.into()).await?)
    }

    /// Price from, as it opens: every provider that prices the chosen
    /// currency asked at once (8 s each at most); none with XMR.
    pub async fn check_prices(&self) -> Option<PriceChecks> {
        let checks = self.inner.check_prices().await?;
        let summary = price::checks_summary(&checks);
        let lines = checks
            .into_iter()
            .filter_map(|c| {
                Some(PriceCheck {
                    provider: price::Provider::named(c.name)?.into(),
                    name: c.name.into(),
                    web: c.web,
                    tor: c.tor,
                    chosen: c.chosen,
                    rate: c.rate,
                    failure: c.failure,
                    can_pick: c.can_pick,
                    label: c.label,
                })
            })
            .collect();
        Some(PriceChecks { lines, summary })
    }

    /// How price requests travel; one not available is refused.
    pub async fn set_price_transport(&self, transport: PriceTransport) -> Result<()> {
        Ok(self.inner.set_price_transport(transport.into()).await?)
    }

    /// Tapping Home's balance: the default currency first (true) or XMR; refused with XMR.
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

    /// What [`prepare`](FobWallet::prepare) would pay, checked the same,
    /// without asking the node: Confirm shows it at once, the fee to come.
    pub async fn preview(&self, to: String, amount: String, in_fiat: bool, code: Option<String>) -> Result<PaymentPreview> {
        let p = self.inner.preview_typed(&to, &amount, in_fiat, code.as_deref()).await?;
        Ok(PaymentPreview { to: p.to, amount_text: exact(p.amount), amount_fiat: self.inner.worth_now(p.amount).await, receipt: p.receipt.map(Into::into) })
    }

    /// Split up's screen for `counts` pieces of each note (none as it
    /// opens), with Auto learned from the 30 days to `now`.
    pub async fn split_view(&self, counts: Vec<u32>, now: u64, utc_offset: i32, hours_24: bool) -> SplitView {
        self.inner.split_view(&counts, activity::Clock { now, utc_offset, hours_24 }).await.into()
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
    /// money only with a currency besides XMR and a price from within the
    /// day, its age said once it's aged (the clock as for `activity_days`).
    /// Money typed against a price 5 minutes old has Rust ask again; a
    /// failure comes as the price toast.
    pub async fn amount_entry(&self, text: String, in_fiat: bool, now: u64, utc_offset: i32, hours_24: bool) -> AmountEntry {
        self.inner.amount_field(&text, in_fiat, activity::Clock { now, utc_offset, hours_24 }).await.into()
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

    /// Settings' row: "Your profile" over "Henry · share as a code", or
    /// only "Add a shareable Fob contact" before there's a name.
    pub async fn profile_row(&self) -> ProfileRow {
        let row = self.inner.profile_row().await;
        ProfileRow { title: row.title, name: row.name, suffix: row.suffix }
    }

    /// Saves your name and photo (a WebP from `contact_photo_kept`); throws
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
    /// "Your profile", "Add a shareable Fob contact"
    pub title: String,
    /// Under it, with a name: "Henry" (cut with an ellipsis when long) then
    /// " · share as a code" (always shown). None without a name.
    pub name: Option<String>,
    pub suffix: Option<String>,
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

impl From<price::AmountField> for AmountEntry {
    fn from(f: price::AmountField) -> Self {
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
    /// A WebP from `contact_photo_kept`.
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

/// A photo the app encoded as Fob keeps it (a WebP of at most 1536 bytes,
/// without the colour profile newer Androids add), or none when it isn't
/// one; the app lowers the quality, then the size, until it is.
#[uniffi::export]
pub fn contact_photo_kept(photo: Vec<u8>) -> Option<Vec<u8>> {
    contacts::photo_kept(&photo)
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
    /// A default currency besides XMR: show the two boxes.
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
    Ok(qr::encode(&text)?.into())
}

impl From<qr::QrMatrix> for QrCode {
    fn from(m: qr::QrMatrix) -> Self {
        QrCode { size: m.size, dark: m.dark }
    }
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

/// A node, and whether its self-signed certificate is accepted.
#[derive(uniffi::Record)]
pub struct NodeSetting {
    pub url: String,
    pub self_signed: bool,
}

impl From<fob_core::node::Setting> for NodeSetting {
    fn from(s: fob_core::node::Setting) -> Self {
        NodeSetting { url: s.url, self_signed: s.self_signed }
    }
}

impl From<NodeSetting> for fob_core::node::Setting {
    fn from(s: NodeSetting) -> Self {
        fob_core::node::Setting { url: s.url, self_signed: s.self_signed }
    }
}

/// How a node check came out: its icon and colour.
#[derive(Clone, Copy, uniffi::Enum)]
pub enum CheckMark {
    Pass,
    Fail,
    Checking,
    /// Not run: one it needs failed.
    Skipped,
}

impl From<fob_core::node::Mark> for CheckMark {
    fn from(m: fob_core::node::Mark) -> Self {
        use fob_core::node::Mark as M;
        match m {
            M::Pass => CheckMark::Pass,
            M::Fail => CheckMark::Fail,
            M::Checking => CheckMark::Checking,
            M::Skipped => CheckMark::Skipped,
        }
    }
}

/// One of a node's checks: "Up to date", "block 1,792,004 · 1 min ago".
#[derive(uniffi::Record)]
pub struct NodeCheckRow {
    pub title: String,
    pub result: String,
    /// A line of its own under the result: "1.8 MB/s" under "31 blocks/s".
    pub second: Option<String>,
    pub mark: CheckMark,
    /// For TalkBack: "Up to date: 2 days behind".
    pub label: String,
}

/// Whether a checked node can be switched to.
#[derive(uniffi::Enum)]
pub enum NodeUse {
    Use,
    /// Only Up to date failed: "Use it anyway".
    UseAnyway,
    No,
}

/// A node's checks as far as they've got, every word from Rust.
#[derive(uniffi::Record)]
pub struct NodeReport {
    pub node: NodeSetting,
    /// "node.example:38081": no scheme, no login.
    pub address: String,
    /// "Working", "Checking…", or the failed check's words.
    pub verdict: String,
    /// The verdict's mark: Working's tick, Checking's dots, else the failure's "!".
    pub mark: CheckMark,
    pub rows: Vec<NodeCheckRow>,
    pub usable: NodeUse,
    pub checking: bool,
}

impl From<fob_core::node::Report> for NodeReport {
    fn from(r: fob_core::node::Report) -> Self {
        use fob_core::node::Usable;
        NodeReport {
            verdict: r.verdict().into(),
            mark: r.mark().into(),
            usable: match r.usable() {
                Usable::Use => NodeUse::Use,
                Usable::UseAnyway => NodeUse::UseAnyway,
                Usable::No => NodeUse::No,
            },
            checking: r.checking(),
            rows: r.rows.iter().map(|row| NodeCheckRow { title: row.title().into(), result: row.result.clone(), second: row.second.clone(), mark: row.mark.into(), label: row.label() }).collect(),
            address: r.address,
            node: NodeSetting { url: r.url, self_signed: r.self_signed },
        }
    }
}

/// Another node on the Node screen's list: its dot, address and one line.
#[derive(uniffi::Record)]
pub struct NodeChoice {
    pub node: NodeSetting,
    pub address: String,
    /// "120 ms · in sync · fastest", "Didn't answer in 5 s", "Checking…".
    pub line: String,
    /// The dot: Pass when it can be used, Checking, else Fail.
    pub mark: CheckMark,
    /// Offer Use.
    pub can_use: bool,
}

impl NodeChoice {
    fn new(report: &fob_core::node::Report, line: String) -> NodeChoice {
        let can_use = report.usable() == fob_core::node::Usable::Use;
        NodeChoice {
            node: NodeSetting { url: report.url.clone(), self_signed: report.self_signed },
            address: report.address.clone(),
            line,
            mark: if report.checking() { CheckMark::Checking } else if can_use { CheckMark::Pass } else { CheckMark::Fail },
            can_use,
        }
    }
}

/// Hears a node's checks as each finishes; implemented by the app. Called
/// from Fob's own threads.
#[uniffi::export(with_foreign)]
pub trait NodeCheckObserver: Send + Sync {
    fn on_progress(&self, report: NodeReport);
}

