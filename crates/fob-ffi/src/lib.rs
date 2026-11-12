//! [`fob_wallet`] for the apps, through UniFFI. This crate only converts types
//! and turns fob-wallet's async calls into Kotlin `suspend` functions; every
//! decision about the wallet is made in fob-wallet, so the apps stay thin.

use std::{path::PathBuf, sync::Arc};

use fob_wallet::{activity, address, amount, backup, clock, contacts, not_ready, pattern, price, profile, qr, receipt, split, uri};

uniffi::setup_scaffolding!();

#[cfg(feature = "wipe-freed")]
pub mod wiping;

/// Freed memory is zeroed, so secrets copied on their way to and from the
/// app don't outlive their use (see [`wiping`]).
#[cfg(feature = "wipe-freed")]
#[global_allocator]
static ALLOCATOR: wiping::Wiping = wiping::Wiping;

pub mod form;
pub mod photo;

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
    /// Not enough ready to spend: `needed`, the amount with about the fee,
    /// in atomic units, for [`FobWallet::shortfall`].
    #[error("{reason}")]
    NotEnoughFunds { reason: String, needed: u64 },
    /// The wallet file and key don't match.
    #[error("{reason}")]
    WrongKey { reason: String },
    #[error("{reason}")]
    ViewOnly { reason: String },
    /// With Manual Sync on, the node refused a payment: a coin in it may have
    /// been spent since the last scan.
    #[error("{reason}")]
    SpendRejected { reason: String },
    /// A Trezor couldn't be reached or refused, and why.
    #[error("{reason}")]
    Trezor { reason: String },
    /// The person said no on the Trezor, or to its PIN or passphrase.
    #[error("{reason}")]
    TrezorCancelled { reason: String },
    /// This wallet's spend key is on a Trezor, which has to be connected
    /// for this: to give key images, or to sign.
    #[error("{reason}")]
    TrezorNeeded { reason: String },
    #[error("{reason}")]
    Other { reason: String },
}

impl From<fob_wallet::FobError> for FobError {
    fn from(e: fob_wallet::FobError) -> Self {
        use fob_wallet::{
            FobError as E,
            error::{ScreenError, VaultError},
        };
        let reason = e.to_string();
        match e {
            E::Node(_) | E::Light(_) => FobError::Node { reason },
            E::Screen(ScreenError::NotEnoughFunds { needed, .. }) => FobError::NotEnoughFunds { reason, needed },
            E::Vault(VaultError::WrongKey) => FobError::WrongKey { reason },
            E::Vault(VaultError::ViewOnly) => FobError::ViewOnly { reason },
            E::SpendRejected(_) => FobError::SpendRejected { reason },
            E::Trezor(_) => FobError::Trezor { reason },
            E::TrezorCancelled => FobError::TrezorCancelled { reason },
            E::TrezorNeeded => FobError::TrezorNeeded { reason },
            E::Send(_) | E::ReorgTooDeep | E::Price(_) | E::Vault(VaultError::Storage(_) | VaultError::Sealing | VaultError::NotAWalletFile) | E::Screen(ScreenError::NoSuchPayment | ScreenError::NothingToSwap | ScreenError::NoPriceToConvert) => {
                FobError::Other { reason }
            }
            E::Monero(_) | E::Vault(_) | E::Code(_) | E::Screen(_) | E::Endpoint(_) => FobError::Invalid { reason },
        }
    }
}

/// The kinds of error fob-wallet's parts raise, as fob-wallet's own.
macro_rules! from_kind {
    ($($kind:ident),*) => {$(
        impl From<fob_wallet::error::$kind> for FobError {
            fn from(e: fob_wallet::error::$kind) -> Self {
                fob_wallet::FobError::from(e).into()
            }
        }
    )*};
}
from_kind!(MoneroError, VaultError, CodeError, ScreenError, PriceError, EndpointProblem);

type Result<T> = std::result::Result<T, FobError>;

/// What Fob runs on, for sentences that name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Device {
    Phone,
    Computer,
}

/// Says which device Fob is on, once, as an app starts. Phone unless told.
#[uniffi::export]
pub fn set_device(device: Device) {
    fob_wallet::words::set_device(match device {
        Device::Phone => fob_wallet::words::Device::Phone,
        Device::Computer => fob_wallet::words::Device::Computer,
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Network {
    Mainnet,
    Stagenet,
}

impl From<Network> for fob_wallet::Network {
    fn from(n: Network) -> Self {
        match n {
            Network::Mainnet => fob_wallet::Network::Mainnet,
            Network::Stagenet => fob_wallet::Network::Stagenet,
        }
    }
}

impl From<fob_wallet::Network> for Network {
    fn from(n: fob_wallet::Network) -> Self {
        match n {
            fob_wallet::Network::Mainnet => Network::Mainnet,
            fob_wallet::Network::Stagenet => Network::Stagenet,
        }
    }
}

fn file_key(key: Vec<u8>) -> Result<[u8; 32]> {
    // The app's copy is zeroed by the app; this one is zeroed here.
    let key = zeroize::Zeroizing::new(key);
    <[u8; 32]>::try_from(key.as_slice()).map_err(|_| FobError::Other { reason: "the wallet key must be 32 bytes".into() })
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
    /// The receipt's own total, as the code says: on Confirm it sits on the
    /// receipt, never in the XMR amount's or Fob's valuation's place.
    pub total: String,
    pub currency: String,
    /// On Confirm, the receipt's currency when it isn't the wallet's
    /// ("USD"), for the receipt's heading. Its money, and the wallet's
    /// beside it, then name their dollar ("US$8.25", "A$12.62").
    pub other_currency: Option<String>,
    /// On Confirm, in another currency: the receipt's last row, the total
    /// in the wallet's currency at Fob's prices ("US$8.25 in AUD" beside
    /// "A$12.62"), the same money as under the XMR amount when they agree.
    pub converted_label: Option<String>,
    pub converted: Option<String>,
    /// On Confirm, when the total and what the XMR amount is worth at Fob's
    /// prices are more than 5% apart: the red line under the receipt (and
    /// its converted row red too).
    pub mismatch: Option<String>,
    /// On Confirm, when Fob has no price in the receipt's currency: a muted
    /// line, "No USD price right now".
    pub no_price: Option<String>,
}

impl Receipt {
    /// The receipt for paying `atomic`, set against Fob's prices; `ask`
    /// asks the provider for a price in its currency if none is kept.
    async fn checked(wallet: &fob_wallet::Wallet, r: receipt::ReceiptView, atomic: u64, ask: bool) -> Receipt {
        let shown = wallet.shown_currency().await;
        let wallet_price = wallet.price_now().await.map(|(_, p)| p);
        let receipt_price = wallet.receipt_price(&r.currency, ask).await;
        let c = receipt::check(&r, atomic, shown.as_ref(), wallet_price, receipt_price);
        let (converted_label, converted) = c.converted.unzip();
        Receipt {
            lines: c.lines.into_iter().map(|l| ReceiptLine { label: l.label, amount: l.amount }).collect(),
            tax: c.tax,
            total: c.total,
            other_currency: c.other_currency,
            converted_label,
            converted,
            mismatch: c.mismatch,
            // The preview asks for no price, but prepare will: not "no price" for a moment first.
            no_price: if ask { c.no_price } else { None },
            ..Receipt::from(r)
        }
    }
}

/// What `atomic` XMR is worth at the shown price, written distinctly
/// ("US$8.00") when `distinct`: beside a receipt in another currency.
async fn worth(wallet: &fob_wallet::Wallet, atomic: u64, distinct: bool) -> Option<String> {
    wallet.price_now().await.map(|(c, p)| fob_wallet::fiat::value(atomic, p, &if distinct { c.distinct() } else { c }))
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
            other_currency: None,
            converted_label: None,
            converted: None,
            mismatch: None,
            no_price: None,
        }
    }
}

#[derive(uniffi::Record)]
pub struct Balance {
    pub total: u64,
    pub unlocked: u64,
    /// Of `unlocked`, what a Trezor hasn't been asked about yet, so the
    /// wallet can't vouch for it; 0 in every other wallet.
    pub unsynced: u64,
    /// "2.4813"
    pub total_text: String,
    pub unlocked_text: String,
    pub unsynced_text: String,
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
    /// Syncing through a light wallet server: Settings greys the Node row
    /// and instant updates, and hides Manual Sync.
    pub light_mode: bool,
    /// Settings' Light wallet server row: "lws.example:8443 · synced"; none
    /// before a server is chosen.
    pub light_line: Option<String>,
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
    /// A Trezor wallet's outputs waiting for the device to give their key
    /// images: Home asks to connect it. 0 for other wallets.
    pub trezor_unsynced: u32,
    /// A Trezor wallet's outputs kept before Fob recorded transaction keys,
    /// which the device can't be asked about until a rescan finds them again.
    pub trezor_frozen: u32,
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

/// Backup and keys, every word from fob-wallet, and Settings' line for it.
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

impl From<backup::HubRow> for HubRow {
    fn from(r: backup::HubRow) -> Self {
        let item = match r.item {
            backup::HubItem::SeedPhrase => HubItem::SeedPhrase,
            backup::HubItem::WalletCode => HubItem::WalletCode,
            backup::HubItem::Keys => HubItem::Keys,
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
    /// 64 hex digits, the whole address, or "1690400 · 2 Sep 2026", as
    /// UTF-8. Bytes rather than a string so the app can zero them once the
    /// screen is gone; a string can't be wiped.
    pub value: Vec<u8>,
    /// What Copy puts on the clipboard: the value, the restore height
    /// without its date. UTF-8 bytes, as `value`.
    pub clip: Vec<u8>,
    /// "Copy the secret spend key"
    pub copy: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum KeyBadge {
    SpendsMoney,
    SeesPayments,
    Public,
}

impl From<backup::KeyGroup> for KeyGroup {
    fn from(g: backup::KeyGroup) -> Self {
        let keys = g
            .keys
            .into_iter()
            .map(|k| KeyRow {
                title: k.title,
                badge: match k.badge {
                    backup::KeyBadge::SpendsMoney => KeyBadge::SpendsMoney,
                    backup::KeyBadge::SeesPayments => KeyBadge::SeesPayments,
                    backup::KeyBadge::Public => KeyBadge::Public,
                },
                badge_text: k.badge_text,
                line: k.line,
                value: k.value.as_bytes().to_vec(),
                clip: k.clip.as_bytes().to_vec(),
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
        let empty = fob_wallet::words::prices::no_match(&l);
        CurrencyList {
            groups: l
                .groups
                .into_iter()
                .map(|g| CurrencyGroup {
                    title: fob_wallet::words::prices::currency_heading(g.heading),
                    rows: g.rows.into_iter().map(|r| CurrencyRow { code: r.code, sign: r.sign, name: r.name, line: fob_wallet::words::prices::currency_line(&r.line), chosen: r.chosen }).collect(),
                })
                .collect(),
            empty,
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

impl From<fob_wallet::price_alert::Toast> for PriceToast {
    fn from(t: fob_wallet::price_alert::Toast) -> Self {
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
    pub status: PaymentStatus,
}

/// Where a payment stands, for a client to act on. Its words are in a row's line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PaymentStatus {
    Sending,
    OnTheWay,
    Confirming { done: u64, needed: u64 },
    NotReady { done: u64, needed: u64 },
    Confirmed { confirmations: u64 },
    NotConfirmed,
    Released,
}

impl From<activity::PaymentStatus> for PaymentStatus {
    fn from(s: activity::PaymentStatus) -> Self {
        use activity::PaymentStatus as S;
        match s {
            S::Released => PaymentStatus::Released,
            S::NotConfirmed => PaymentStatus::NotConfirmed,
            S::Sending => PaymentStatus::Sending,
            S::OnTheWay => PaymentStatus::OnTheWay,
            S::Confirming { done, needed } => PaymentStatus::Confirming { done, needed },
            S::NotReady { done, needed } => PaymentStatus::NotReady { done, needed },
            S::Confirmed { confirmations } => PaymentStatus::Confirmed { confirmations },
        }
    }
}

fn activity(a: fob_wallet::Activity) -> Activity {
    Activity {
        status: activity::payment_status(&a).into(),
        outgoing: a.direction == fob_wallet::Direction::Out,
        amount_text: xmr(a.amount),
        tx: a.tx,
        receipt: a.receipt.map(|r| std::sync::Arc::unwrap_or_clone(r).into()),
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
        fob_wallet::words::activity::row(r).into()
    }
}

impl From<fob_wallet::words::activity::Row> for ActivityRow {
    fn from(r: fob_wallet::words::activity::Row) -> Self {
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

/// One segment of the coins bar: a coin, or the smallest ready ones put
/// together.
#[derive(uniffi::Record)]
pub struct CoinSegment {
    /// In atomic units: how wide it is beside the others.
    pub atomic: u64,
    /// How far a coin not ready yet is along, 0 to 1, drawn as solid green
    /// over faint; none for money ready to spend, drawn in ink.
    pub progress: Option<f32>,
    /// How many coins it stands for.
    pub coins: u32,
}

impl From<fob_wallet::coin_bar::Segment> for CoinSegment {
    fn from(s: fob_wallet::coin_bar::Segment) -> Self {
        CoinSegment { atomic: s.atomic, progress: s.progress, coins: s.coins }
    }
}

/// The coins bar's gap between segments and its narrowest segment, in dp
/// (px on the desktop).
pub const COIN_BAR_GAP: f32 = 2.0;
pub const COIN_BAR_MIN: f32 = 4.0;

/// The gap between the coins bar's segments, for drawing them.
#[uniffi::export]
pub fn coin_bar_gap() -> f32 {
    COIN_BAR_GAP
}

/// Each segment's width across a bar `width` wide: [`COIN_BAR_GAP`] apart,
/// none narrower than [`COIN_BAR_MIN`], the rest shared by amount.
#[uniffi::export]
pub fn coin_bar_widths(segments: Vec<CoinSegment>, width: f32) -> Vec<f32> {
    fob_wallet::coin_bar::widths(&segments.iter().map(|s| s.atomic).collect::<Vec<_>>(), width, COIN_BAR_GAP, COIN_BAR_MIN)
}

/// The money not ready yet beside what is, for Home's card: what's ready
/// on the left in ink; what's on its way and when it'll all be ready on
/// the right in green; the coins bar under them.
#[derive(uniffi::Record)]
pub struct NotReady {
    /// Ready to spend and not ready, in atomic units.
    pub ready: u64,
    pub not_ready: u64,
    /// "1.8592 XMR"
    pub ready_xmr: String,
    /// "1.253 XMR"
    pub incoming_xmr: String,
    /// The same in money, once there's a price, longest first: show the
    /// first that fits ("A$1,080.92", "A$1,081", "A$1.08K"). Empty without
    /// a price. Shown in place of the XMR while the balance shows money
    /// first.
    pub ready_money: Vec<String>,
    pub incoming_money: Vec<String>,
    /// "09:02", beside the clock.
    pub all_ready: String,
    /// The card for TalkBack: "1.8592 XMR ready to spend, 1.253 XMR not
    /// yet, all ready about 09:02".
    pub description: String,
    /// The coins bar.
    pub segments: Vec<CoinSegment>,
}

impl From<not_ready::NotReady> for NotReady {
    fn from(n: not_ready::NotReady) -> Self {
        let n = fob_wallet::words::not_ready::not_ready(n);
        NotReady {
            ready: n.ready,
            not_ready: n.not_ready,
            ready_xmr: n.ready_xmr,
            incoming_xmr: n.incoming_xmr,
            ready_money: n.ready_money,
            incoming_money: n.incoming_money,
            all_ready: n.all_ready,
            description: n.description,
            segments: n.segments.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<fob_wallet::words::not_ready::PieceView> for NotReadyPiece {
    fn from(p: fob_wallet::words::not_ready::PieceView) -> Self {
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
    /// The coins bar, coin by coin.
    pub segments: Vec<CoinSegment>,
}

impl From<not_ready::Coins> for Coins {
    fn from(c: not_ready::Coins) -> Self {
        let c = fob_wallet::words::not_ready::coins(c);
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
            segments: c.segments.into_iter().map(Into::into).collect(),
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
    /// Manual Sync is on: Home shows Sync rather than the chip, but for a failure or a scan.
    pub manual_sync: bool,
    /// A scan someone asked for, while it runs.
    pub scan: Option<ScanView>,
    /// Something to say once.
    pub notice: Option<String>,
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

impl From<fob_wallet::ZmqStatus> for ZmqStatus {
    fn from(z: fob_wallet::ZmqStatus) -> Self {
        match z {
            fob_wallet::ZmqStatus::Off => ZmqStatus::Off,
            fob_wallet::ZmqStatus::Connecting => ZmqStatus::Connecting,
            fob_wallet::ZmqStatus::Listening => ZmqStatus::Listening,
            fob_wallet::ZmqStatus::CantConnect(problem) => ZmqStatus::CantConnect { reason: fob_wallet::words::node::zmq_problem(&problem) },
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
    /// "Scan the whole wallet again", when that's the way past the failure.
    pub scan_again: Option<String>,
}

impl From<fob_wallet::catch_up::Details> for SyncDetails {
    fn from(d: fob_wallet::catch_up::Details) -> Self {
        let d = fob_wallet::words::catch_up::details(d);
        SyncDetails {
            title: d.title,
            percent: d.percent,
            progress: d.progress,
            rows: d.rows.into_iter().map(|r| SyncRow { label: r.label, value: r.value, spoken: r.spoken }).collect(),
            note: d.note,
            error: d.error,
            scan_again: d.scan_again,
        }
    }
}

/// The fewest Settled Confirmations can be.
#[uniffi::export]
pub fn manual_settled_min() -> u32 {
    fob_wallet::manual::SETTLED_MIN
}

/// The most Settled Confirmations can be.
#[uniffi::export]
pub fn manual_settled_max() -> u32 {
    fob_wallet::manual::SETTLED_MAX
}

/// The most Incoming Scan Blocks can be.
#[uniffi::export]
pub fn manual_incoming_max() -> u32 {
    fob_wallet::manual::INCOMING_MAX
}

/// A scan someone asked for, ready to draw.
#[derive(uniffi::Record)]
pub struct ScanView {
    /// "Scanning from last scan".
    pub title: String,
    /// "893 of 1,440 blocks".
    pub blocks: String,
    /// 0 to 1.
    pub progress: f32,
    /// Where the bar marks a payment.
    pub marks: Vec<ScanMark>,
}

/// A payment found, on the scan's bar: money in, or money out.
#[derive(uniffi::Record)]
pub struct ScanMark {
    /// 0 to 1 along the bar.
    pub position: f32,
    pub incoming: bool,
}

impl From<fob_wallet::wallet::ScanProgress> for ScanView {
    fn from(s: fob_wallet::wallet::ScanProgress) -> Self {
        let total = s.to.saturating_sub(s.from).max(1);
        let done = s.at.saturating_sub(s.from).min(total);
        let position = |height: u64| (height.saturating_sub(s.from) as f32 / total as f32).clamp(0.0, 1.0);
        ScanView {
            title: fob_wallet::words::manual::scan_title(s.title),
            blocks: fob_wallet::words::manual::scan_blocks(done, total),
            progress: done as f32 / total as f32,
            marks: s.marks.iter().map(|(height, incoming)| ScanMark { position: position(*height), incoming: *incoming }).collect(),
        }
    }
}

/// What pulling down on Home does with Manual Sync on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PullMode {
    Disabled,
    Send,
    Auto,
}

impl From<PullMode> for fob_wallet::manual::PullMode {
    fn from(m: PullMode) -> Self {
        match m {
            PullMode::Disabled => Self::Disabled,
            PullMode::Send => Self::Send,
            PullMode::Auto => Self::Auto,
        }
    }
}

impl From<fob_wallet::manual::PullMode> for PullMode {
    fn from(m: fob_wallet::manual::PullMode) -> Self {
        match m {
            fob_wallet::manual::PullMode::Disabled => Self::Disabled,
            fob_wallet::manual::PullMode::Send => Self::Send,
            fob_wallet::manual::PullMode::Auto => Self::Auto,
        }
    }
}

/// A wallet's Manual Sync settings.
#[derive(uniffi::Record)]
pub struct ManualSettings {
    pub on: bool,
    pub pull_mode: PullMode,
    pub incoming_scan_blocks: u32,
    pub settled_confirmations: u32,
}

/// What the Scan drawer can be asked for.
#[derive(Clone, Copy, uniffi::Enum)]
pub enum ScanRequest {
    Unsettled,
    FromLast,
    Back { blocks: u64 },
    Whole,
    Height { height: u64 },
    Dates { from: u64, to: Option<u64> },
}

impl From<ScanRequest> for fob_wallet::manual::ScanRequest {
    fn from(r: ScanRequest) -> Self {
        match r {
            ScanRequest::Unsettled => Self::Unsettled,
            ScanRequest::FromLast => Self::FromLast,
            ScanRequest::Back { blocks } => Self::Back(blocks),
            ScanRequest::Whole => Self::Whole,
            ScanRequest::Height { height } => Self::Height(height),
            ScanRequest::Dates { from, to } => Self::Dates { from, to },
        }
    }
}

impl From<fob_wallet::manual::ScanRequest> for ScanRequest {
    fn from(r: fob_wallet::manual::ScanRequest) -> Self {
        use fob_wallet::manual::ScanRequest as R;
        match r {
            R::Unsettled => Self::Unsettled,
            R::FromLast => Self::FromLast,
            R::Back(blocks) => Self::Back { blocks },
            R::Whole => Self::Whole,
            R::Height(height) => Self::Height { height },
            R::Dates { from, to } => Self::Dates { from, to },
        }
    }
}

/// One of the Scan drawer's quick choices.
#[derive(uniffi::Record)]
pub struct ScanOption {
    pub request: ScanRequest,
    pub label: String,
    pub note: String,
    pub cost: String,
    pub action: String,
}

/// The Scan drawer's words as it opens.
#[derive(uniffi::Record)]
pub struct ScanDrawer {
    /// "Last scan Today, 14:20 · Block 3,811,200".
    pub last_scan: String,
    pub options: Vec<ScanOption>,
}

/// The Block tab's facts about a typed block.
#[derive(uniffi::Record)]
pub struct BlockInfo {
    pub mined: Option<String>,
    pub newest: String,
    pub first: String,
}

/// What a pull found, in words.
#[derive(uniffi::Record)]
pub struct PullOutcome {
    pub title: String,
    pub line: String,
}

impl From<fob_wallet::manual::PullOutcome> for PullOutcome {
    fn from(p: fob_wallet::manual::PullOutcome) -> Self {
        let said = fob_wallet::words::manual::pull(&p);
        PullOutcome { title: said.title, line: said.line }
    }
}

impl From<fob_wallet::SyncStatus> for SyncStatus {
    fn from(s: fob_wallet::SyncStatus) -> Self {
        let chip = fob_wallet::words::catch_up::chip(s.chip());
        let zmq_text = fob_wallet::words::catch_up::zmq_text(&s.zmq, s.zmq_endpoint.as_deref());
        SyncStatus {
            scanned: s.scanned,
            tip: s.tip,
            synced: s.synced,
            progress: s.progress,
            error: s.error,
            chip_look: match chip.look {
                fob_wallet::catch_up::Look::Ring => ChipLook::Ring,
                fob_wallet::catch_up::Look::Synced => ChipLook::Synced,
                fob_wallet::catch_up::Look::Offline => ChipLook::Offline,
            },
            chip_text: chip.text,
            chip_label: chip.label,
            zmq: s.zmq.into(),
            zmq_text,
            manual_sync: s.manual_sync,
            scan: s.scan.map(Into::into),
            notice: s.notice.map(fob_wallet::words::manual::notice),
        }
    }
}

/// Hears about syncing; implemented by the app. Called from Fob's own
/// threads: hand the status to the UI, don't call back into the wallet here.
///
/// It returns a Result so that an exception thrown in the app comes back as
/// an error rather than a panic: UniFFI panics on an exception from a method
/// that can't fail, and that would end the sync task silently.
#[uniffi::export(with_foreign)]
pub trait SyncObserver: Send + Sync {
    fn on_status(&self, status: SyncStatus) -> Result<()>;
}

struct Observer(Arc<dyn SyncObserver>);

/// The bytes to and from a Trezor, one 64-byte USB report at a time;
/// implemented by the app over Android's USB host API. `read` blocks until
/// the device answers, which may be when a finger does.
#[uniffi::export(with_foreign)]
pub trait TrezorLink: Send + Sync {
    fn write(&self, report: Vec<u8>) -> Result<()>;
    fn read(&self) -> Result<Vec<u8>>;
}

/// What the person says to a Trezor's passphrase prompt.
#[derive(uniffi::Enum)]
pub enum TrezorPassphrase {
    /// Typed on the device itself.
    OnDevice,
    /// Typed on the phone; empty for the wallet without one.
    Typed { text: String },
    Cancelled,
}

/// A Trezor's questions, put to the person by the app. Called on Fob's
/// own thread, which waits for the answer: show the prompt and block until
/// it's answered. Each returns a Result so an exception in the app comes
/// back as an error, not a panic (see [`SyncObserver`]).
#[uniffi::export(with_foreign)]
pub trait TrezorPrompts: Send + Sync {
    /// The device answered the hello: its name, and whether it has a PIN
    /// and passphrase protection, which decide the steps the screen counts.
    fn on_hello(&self, name: String, pin_protection: bool, passphrase_protection: bool) -> Result<()>;
    /// The device is about to show this address for the person to compare
    /// with the one Fob received: show it in full.
    fn on_address(&self, address: String) -> Result<()>;
    /// The device shows something to confirm; `what` names its screen.
    fn on_button(&self, what: String) -> Result<()>;
    /// The device wants its PIN: the positions (1 to 9) of the digits on
    /// the scrambled keypad it shows. `None` cancels.
    fn pin(&self) -> Result<Option<String>>;
    /// `on_device` says the device can take the passphrase itself.
    fn passphrase(&self, on_device: bool) -> Result<TrezorPassphrase>;
    /// `done` of `total` steps of a long exchange are through.
    fn progress(&self, done: u32, total: u32) -> Result<()>;
}

struct Link(Arc<dyn TrezorLink>);

impl fob_wallet::trezor::Link for Link {
    fn write(&self, report: &[u8]) -> std::result::Result<(), String> {
        self.0.write(report.to_vec()).map_err(|e| e.to_string())
    }
    fn read(&self) -> std::result::Result<Vec<u8>, String> {
        self.0.read().map_err(|e| e.to_string())
    }
}

struct Prompts(Arc<dyn TrezorPrompts>);

impl fob_wallet::trezor::Prompts for Prompts {
    fn hello(&self, device: &fob_wallet::trezor::Hello) {
        let _ = self.0.on_hello(device.name.clone(), device.pin_protection, device.passphrase_protection);
    }
    fn address(&self, address: &str) {
        let _ = self.0.on_address(address.to_owned());
    }
    fn button(&self, what: &str) {
        let _ = self.0.on_button(what.to_owned());
    }
    fn pin(&self) -> Option<zeroize::Zeroizing<String>> {
        // An app that can't ask is a cancelled prompt.
        self.0.pin().ok().flatten().map(zeroize::Zeroizing::new)
    }
    fn passphrase(&self, on_device: bool) -> fob_wallet::trezor::Passphrase {
        match self.0.passphrase(on_device) {
            Ok(TrezorPassphrase::OnDevice) => fob_wallet::trezor::Passphrase::OnDevice,
            Ok(TrezorPassphrase::Typed { text }) => fob_wallet::trezor::Passphrase::Typed(zeroize::Zeroizing::new(text)),
            Ok(TrezorPassphrase::Cancelled) | Err(_) => fob_wallet::trezor::Passphrase::Cancelled,
        }
    }
    fn progress(&self, done: u32, total: u32) {
        let _ = self.0.progress(done, total);
    }
}

impl fob_wallet::SyncListener for Observer {
    fn on_status(&self, status: fob_wallet::SyncStatus) {
        // The next status replaces this one, so a failure to hear it only
        // costs the screen one update; syncing goes on.
        if let Err(e) = self.0.on_status(status.into()) {
            eprintln!("fob: the app couldn't take a sync status: {e}");
        }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for FobError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        FobError::Other { reason: e.reason }
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
        let v = fob_wallet::words::split::view(v);
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
    /// The words, one space between each, as UTF-8. Bytes rather than
    /// strings so the app can zero them once it has the words on screen.
    pub phrase: Vec<u8>,
}

/// A QR code holding a secret (the wallet code): as [`QrCode`], but with the
/// modules as bytes the app can zero once the code is off the screen.
#[derive(uniffi::Record)]
pub struct SecretQr {
    pub size: u32,
    /// Row by row, 1 for dark and 0 for light.
    pub dark: Vec<u8>,
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

/// A wallet on this phone.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct WalletEntry {
    /// The wallet's file name, which never changes: its id.
    pub file: String,
    pub name: String,
    pub network: Network,
}

impl From<&fob_wallet::wallets::Entry> for WalletEntry {
    fn from(e: &fob_wallet::wallets::Entry) -> Self {
        WalletEntry { file: e.file.clone(), name: e.name.clone(), network: e.network.into() }
    }
}

/// Starts the GPU for scanning, in the background, for every wallet in this
/// process: it's used once it passes its checks, and the CPU scans until then
/// or if it can't be used. `cache_dir` keeps the compiled shader, which is the
/// slow part of a GPU's first use. An app calls this once, as it starts.
#[uniffi::export]
pub fn start_scanning(cache_dir: String) {
    fob_wallet::scanning::set_cache_dir(cache_dir.into());
    fob_wallet::scanning::warm_up();
}

/// What's scanning, for Settings to show: there's nothing to choose.
#[derive(uniffi::Record)]
pub struct ScanEngine {
    /// Whether the GPU is doing the work.
    pub gpu: bool,
    /// "GPU · Mali-G715", or "CPU · " and why.
    pub line: String,
}

/// What's scanning. `wallet_refused` is the open wallet's
/// `scan_gpu_check_failed`: a GPU that failed it doesn't scan that wallet.
#[uniffi::export]
pub fn scan_engine(wallet_refused: bool) -> ScanEngine {
    let engine = fob_wallet::scanning::engine();
    let gpu = matches!(engine, fob_wallet::scanning::Engine::Gpu { .. }) && !wallet_refused;
    ScanEngine { gpu, line: fob_wallet::words::scanning::engine_line(&engine, wallet_refused) }
}

/// Whether the folder `dir` holds a wallet on `network`; asks nothing of the
/// keystore, so it can be asked while the first screen is chosen.
#[uniffi::export]
pub fn wallets_on(dir: String, network: Network) -> bool {
    fob_wallet::wallets::any_on(&PathBuf::from(dir), network.into())
}

/// The wallets on this phone: names, networks, and which was last open.
#[derive(uniffi::Object)]
pub struct WalletList {
    inner: std::sync::Mutex<fob_wallet::wallets::Wallets>,
}

impl WalletList {
    fn book(&self) -> std::sync::MutexGuard<'_, fob_wallet::wallets::Wallets> {
        self.inner.lock().expect("wallet list lock")
    }
}

#[uniffi::export]
impl WalletList {
    /// The list kept in `dir`, sealed under `key`; wallets from before there
    /// was a list join it.
    #[uniffi::constructor]
    pub fn open(dir: String, key: Vec<u8>) -> Result<Arc<Self>> {
        Ok(Arc::new(WalletList { inner: std::sync::Mutex::new(fob_wallet::wallets::Wallets::open(&PathBuf::from(dir), file_key(key)?)?) }))
    }

    pub fn list(&self) -> Vec<WalletEntry> {
        self.book().list().iter().map(Into::into).collect()
    }

    /// The wallet to open on `network`, if it has any.
    pub fn current(&self, network: Network) -> Option<WalletEntry> {
        self.book().current(network.into()).map(Into::into)
    }

    pub fn select(&self, file: String) -> Result<()> {
        Ok(self.book().select(&file)?)
    }

    /// Where a wallet's file is.
    pub fn path(&self, file: String) -> String {
        self.book().path(&file).to_string_lossy().into_owned()
    }

    /// The name a new wallet gets unless it's given one: "Wallet 2".
    pub fn next_name(&self) -> String {
        self.book().next_name()
    }

    /// A file name for a new wallet on `network`.
    pub fn new_file(&self, network: Network) -> String {
        self.book().new_file(network.into())
    }

    /// Throws why a new wallet can't have `name`; blank takes the next name.
    pub fn check_name(&self, name: String) -> Result<()> {
        self.book().check_name(&name)?;
        Ok(())
    }

    /// Lists a wallet just made in `file`, as the one open on its network.
    pub fn add(&self, file: String, name: String) -> Result<WalletEntry> {
        Ok((&self.book().add(&file, &name)?).into())
    }

    pub fn rename(&self, file: String, name: String) -> Result<()> {
        Ok(self.book().rename(&file, &name)?)
    }

    /// Takes a wallet's file and line away.
    pub fn remove(&self, file: String) -> Result<()> {
        Ok(self.book().remove(&file)?)
    }
}

/// A Fob wallet, saved in one file.
#[derive(uniffi::Object)]
pub struct FobWallet {
    inner: fob_wallet::Wallet,
}

fn wallet(inner: fob_wallet::Result<fob_wallet::Wallet>) -> Result<Arc<FobWallet>> {
    Ok(Arc::new(FobWallet { inner: inner? }))
}

fn height(network: Network, restore_point: Option<String>) -> Result<Option<u64>> {
    restore_point
        .filter(|t| !t.trim().is_empty())
        .map(|t| fob_wallet::restore_point(network.into(), &t))
        .transpose()
        .map_err(Into::into)
}

/// A prepared payment or split for the confirm screen, with what it's
/// worth now.
async fn prepared(wallet: &fob_wallet::Wallet, p: fob_wallet::Prepared) -> Prepared {
    let age = wallet.price_age_now().await.map(fob_wallet::words::prices::age);
    let split = p.pieces.map(|n| SplitSummary {
        title: fob_wallet::words::split::TITLE.into(),
        to: fob_wallet::words::split::TO.into(),
        pieces: p.split.iter().map(|(label, amount)| SplitLine { label: label.clone(), amount_text: amount.clone() }).collect(),
        pieces_text: fob_wallet::words::split::coins(n),
        swipe: fob_wallet::words::split::swipe_text(n),
        done: fob_wallet::words::split::done_text(n),
    });
    let receipt = match p.receipt {
        Some(r) => Some(Receipt::checked(wallet, r, p.amount, true).await),
        None => None,
    };
    // Beside a receipt in another currency, the wallet's money names its dollar too.
    let distinct = receipt.as_ref().is_some_and(|r| r.other_currency.is_some());
    Prepared {
        id: p.id,
        to: p.to,
        amount_text: exact(p.amount),
        fee_text: amount::format(p.fee),
        total_text: amount::format(p.amount + p.fee),
        amount_fiat: worth(wallet, p.amount, distinct).await,
        fee_fiat: worth(wallet, p.fee, distinct).await,
        // The total says how old the price is, once it's missed a refresh.
        total_fiat: match (worth(wallet, p.amount + p.fee, distinct).await, &age) {
            (Some(total), Some(age)) => Some(format!("{total} · {age}")),
            (total, _) => total,
        },
        fiat_age: age,
        receipt,
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
        let worth = worth.map(|(w, old)| (w, old.map(fob_wallet::words::prices::age)));
        Balance {
            total: b.total,
            unlocked: b.unlocked,
            unsynced: b.unsynced,
            total_text: xmr(b.total),
            unlocked_text: xmr(b.unlocked),
            unsynced_text: xmr(b.unsynced),
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
        self.inner.not_ready(clock::Clock { now, utc_offset, hours_24 }).await.map(Into::into)
    }


    /// Every coin, ready to spend or not yet, for the Coins screen; the
    /// clock as for `activity_days`.
    async fn coins(&self, now: u64, utc_offset: i32, hours_24: bool) -> Coins {
        self.inner.coins(clock::Clock { now, utc_offset, hours_24 }).await.into()
    }

    /// Settings' money and price rows, and Price from's "Ask over". The
    /// clock as for `activity_days`, for when a failing row's price is from.
    async fn price_settings(&self, now: u64, utc_offset: i32, hours_24: bool) -> PriceSettings {
        let s = self.inner.price_settings().await;
        let sources = price::transport_rows(&s)
            .into_iter()
            .map(|r| PriceSourceChoice { transport: r.transport.into(), title: fob_wallet::words::prices::transport_name(r.transport).into(), line: fob_wallet::words::prices::transport_note(r.note), available: r.available, chosen: r.chosen })
            .collect();
        let (source_text, failing) = self.inner.price_from_line(clock::Clock { now, utc_offset, hours_24 }).await;
        let intro = fob_wallet::words::prices::intro(price::price_from_intro(&s));
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
            blank_entry: self.inner.blank_amount_field(clock::Clock { now, utc_offset, hours_24 }).await.into(),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FobWallet {
    /// A new wallet with a fresh 16-word seed, saved at `path` under `key`
    /// (32 bytes the app keeps in the Keystore).
    #[uniffi::constructor]
    pub fn create(path: String, key: Vec<u8>, network: Network) -> Result<Arc<Self>> {
        wallet(fob_wallet::Wallet::create(&PathBuf::from(path), file_key(key)?, network.into()))
    }

    /// From 16 or 25 seed words. `restore_point` is a date (YYYY-MM-DD) or
    /// block height to start from; a 16-word seed knows its own.
    #[uniffi::constructor]
    pub fn restore_words(path: String, key: Vec<u8>, network: Network, words: String, restore_point: Option<String>) -> Result<Arc<Self>> {
        let height = height(network, restore_point)?;
        wallet(fob_wallet::Wallet::restore_words(&PathBuf::from(path), file_key(key)?, network.into(), &words, height))
    }

    /// From a scanned wallet code.
    #[uniffi::constructor]
    pub fn restore_code(path: String, key: Vec<u8>, network: Network, code: String, restore_point: Option<String>) -> Result<Arc<Self>> {
        let height = height(network, restore_point)?;
        wallet(fob_wallet::Wallet::restore_code(&PathBuf::from(path), file_key(key)?, network.into(), &code, height))
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
        wallet(fob_wallet::Wallet::restore_view_only(&PathBuf::from(path), file_key(key)?, network.into(), &address, &view_key, height))
    }

    /// A wallet whose keys are on the Trezor reached over `link`: the device
    /// exports the view key (asking first) and keeps the spend key. Without
    /// a `restore_point` it scans from the first block, as every restore does.
    #[uniffi::constructor]
    pub fn restore_trezor(
        path: String,
        key: Vec<u8>,
        network: Network,
        link: Arc<dyn TrezorLink>,
        prompts: Arc<dyn TrezorPrompts>,
        restore_point: Option<String>,
    ) -> Result<Arc<Self>> {
        let height = height(network, restore_point)?;
        wallet(fob_wallet::Wallet::restore_trezor(&PathBuf::from(path), file_key(key)?, network.into(), &Link(link), &Prompts(prompts), height))
    }

    #[uniffi::constructor]
    pub fn open(path: String, key: Vec<u8>) -> Result<Arc<Self>> {
        wallet(fob_wallet::Wallet::open(&PathBuf::from(path), file_key(key)?))
    }

    /// Whether this wallet's spend key is on a Trezor, which signs its
    /// payments and gives its key images.
    pub fn is_trezor(&self) -> bool {
        self.inner.is_trezor()
    }

    /// How many outputs the Trezor hasn't given the key image of yet:
    /// they can't be spent, and a spend of them elsewhere goes unseen,
    /// until it has.
    pub async fn trezor_unsynced(&self) -> u32 {
        self.inner.trezor_unsynced().await
    }

    /// Asks the Trezor for the key images it hasn't given, then the node
    /// which are spent. How many outputs it covered.
    pub async fn trezor_sync(&self, link: Arc<dyn TrezorLink>, prompts: Arc<dyn TrezorPrompts>) -> Result<u32> {
        Ok(self.inner.trezor_sync(Arc::new(Link(link)), Arc::new(Prompts(prompts))).await?)
    }

    /// Signs a prepared payment on the Trezor and sends it. Its transaction
    /// hash. Cancelled on the device, the payment stays prepared.
    pub async fn send_with_trezor(&self, id: u64, link: Arc<dyn TrezorLink>, prompts: Arc<dyn TrezorPrompts>) -> Result<String> {
        Ok(self.inner.send_with_trezor(id, Arc::new(Link(link)), Arc::new(Prompts(prompts))).await?)
    }

    pub fn network(&self) -> Network {
        self.inner.network().into()
    }

    /// Takes this wallet's file off the phone for good, stopping its syncing.
    pub fn remove_file(&self) -> Result<()> {
        Ok(self.inner.remove_file()?)
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
                fob_wallet::SeedKind::Polyseed => "polyseed".into(),
                fob_wallet::SeedKind::Classic => "classic".into(),
            },
            phrase: words.as_bytes().to_vec(),
        })
    }

    /// "I've written them down": the reminder goes.
    pub async fn backup_done(&self) -> Result<()> {
        Ok(self.inner.backup_done().await?)
    }

    /// The wallet code as UTF-8 bytes, for the desktop's dialog; the phone
    /// shows [`wallet_code_qr`](Self::wallet_code_qr) and never has the text.
    pub async fn wallet_code(&self) -> Vec<u8> {
        self.inner.wallet_code().await.as_bytes().to_vec()
    }

    /// The wallet code as a QR code, drawn here so the code's text stays in
    /// Rust, where it's wiped.
    pub async fn wallet_code_qr(&self) -> Result<SecretQr> {
        let code = self.inner.wallet_code().await;
        let m = qr::encode(&code)?;
        Ok(SecretQr { size: m.size, dark: m.dark.iter().map(|&d| u8::from(d)).collect() })
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
        let clock = clock::Clock { now, utc_offset, hours_24 };
        // Kept by the wallet until something changes; taken apart for the app.
        let lists = std::sync::Arc::unwrap_or_clone(self.inner.lists(clock).await);
        let waiting = self.inner.trezor_waiting().await;
        let paid = |p: Vec<activity::PaidBefore>| p.into_iter().map(fob_wallet::words::activity::paid_before).map(|p| PaidBefore { address: p.address, name: p.name, line: p.line }).collect();
        Screens {
            prices: self.price_settings(now, utc_offset, hours_24).await,
            balance: self.balance(now).await,
            activity: lists.days.into_iter().map(|day| ActivityDay { label: fob_wallet::words::clock::heading(&day.label), rows: day.rows.into_iter().map(Into::into).collect() }).collect(),
            not_ready: self.not_ready(now, utc_offset, hours_24).await,
            reminder: self.backup_reminder().await,
            node_line: self.inner.node_line().await,
            light_mode: self.inner.light_mode().await,
            light_line: self.inner.light_line().await,
            backup_line: self.inner.backup_hub(phone_lock).await.settings_line,
            receive: self.inner.shown_receive_address().await.map(|(address, index)| ReceiveAddress { address, index }),
            contacts: lists.contacts.into_iter().map(Into::into).collect(),
            unsaved: paid(lists.unsaved),
            send_contacts: lists.on_send.into_iter().map(Into::into).collect(),
            paid_before: paid(lists.paid_before),
            coins: self.coins(now, utc_offset, hours_24).await,
            price_toast: self.inner.price_toast(clock).await.map(Into::into),
            trezor_unsynced: waiting.unsynced,
            trezor_frozen: waiting.frozen,
        }
    }

    pub async fn node(&self) -> NodeSetting {
        self.inner.node().await.into()
    }

    /// The key a self-signed node in use is held to (hex of its SHA3-256 pin),
    /// to show so it can be compared with the node's own; None until the node
    /// first answers, or when it isn't self-signed.
    pub async fn node_pin(&self) -> Option<String> {
        self.inner.node_pin().await
    }

    /// Whether the node in use last answered with a key other than its pin.
    /// Choosing the node again ([`set_node`](Self::set_node)) trusts the new one.
    pub fn certificate_changed(&self) -> bool {
        self.inner.certificate_changed()
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
        // A report the app couldn't take is only a missed update: the checks
        // go on and the last report is returned anyway.
        let progress = |report: &fob_wallet::node::Report| {
            if let Err(e) = observer.on_progress(report.clone().into()) {
                eprintln!("fob: the app couldn't take a node report: {e}");
            }
        };
        self.inner.check_node(&node.into(), &progress).await.into()
    }

    /// The other nodes to offer (the network's, then this wallet's own), each
    /// saying "Checking…" until [`check_nodes`](Self::check_nodes) answers.
    pub async fn other_nodes(&self) -> Vec<NodeChoice> {
        let network = self.inner.network();
        let custom = self.inner.custom_nodes().await;
        self.inner
            .other_nodes()
            .await
            .into_iter()
            .map(|n| {
                let report = fob_wallet::node::Report::pending(network, &n);
                NodeChoice::new(&report, fob_wallet::words::node::line(&report.line(false)), &custom)
            })
            .collect()
    }

    /// Checks the other nodes at once: each one's line and whether it can be used.
    pub async fn check_nodes(&self, nodes: Vec<NodeSetting>) -> Vec<NodeChoice> {
        let nodes: Vec<_> = nodes.into_iter().map(Into::into).collect();
        let custom = self.inner.custom_nodes().await;
        self.inner.check_nodes(&nodes).await.iter().map(|(report, line)| NodeChoice::new(report, fob_wallet::words::node::line(line), &custom)).collect()
    }

    /// Keeps a node on this wallet's list, to change to later.
    pub async fn add_node(&self, node: NodeSetting) -> Result<()> {
        Ok(self.inner.add_node(&node.url, node.self_signed).await?)
    }

    /// Takes a node off this wallet's list.
    pub async fn remove_node(&self, url: String) -> Result<()> {
        Ok(self.inner.remove_node(&url).await?)
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

    /// Syncing through a light wallet server now.
    pub async fn light_mode(&self) -> bool {
        self.inner.light_mode().await
    }

    /// The light wallet server chosen, in use or not.
    pub async fn light_server(&self) -> Option<LightSetting> {
        self.inner.light_server().await.map(|s| LightSetting { url: s.url, self_signed: s.self_signed })
    }

    /// Settings' Light wallet server row: "lws.example:8443 · synced", or
    /// None before a server is chosen.
    pub async fn light_line(&self) -> Option<String> {
        self.inner.light_line().await
    }

    /// The Light server screen's Test: asks the server about itself, sending
    /// nothing of the wallet's.
    pub async fn check_light_server(&self, url: String, self_signed: bool) -> LightReport {
        let said = fob_wallet::words::light::report(&self.inner.check_light_server(&url, self_signed).await);
        LightReport {
            address: said.address,
            verdict: said.verdict,
            usable: said.usable,
            rows: said.rows.into_iter().map(|r| LightReportRow { label: r.label, value: r.value, bad: r.bad }).collect(),
        }
    }

    /// What the Light server screen shows of the server in use before any
    /// Test: the key it's held to, what its amounts are worth, its last
    /// failure. Every word from Rust.
    pub async fn light_standing(&self) -> LightStanding {
        let said = self.inner.light_standing().await;
        LightStanding { rows: said.rows.into_iter().map(|r| LightReportRow { label: r.label, value: r.value, bad: r.bad }).collect(), notes: said.notes }
    }

    /// Chooses the light wallet server, refused with words when it doesn't
    /// answer, is on another network or has subaddresses off.
    pub async fn set_light_server(&self, url: String, self_signed: bool) -> Result<()> {
        Ok(self.inner.set_light_server(&url, self_signed).await?)
    }

    /// Syncs through the chosen server, or through the node again.
    pub async fn use_light_wallet(&self, on: bool) -> Result<()> {
        Ok(self.inner.use_light_wallet(on).await?)
    }

    pub fn status(&self) -> SyncStatus {
        self.inner.status().into()
    }

    /// The card Home's sync chip opens, for the status now; the clock as
    /// for `activity_days`.
    pub fn sync_details(&self, now: u64, utc_offset: i32, hours_24: bool) -> SyncDetails {
        self.inner.sync_details(clock::Clock { now, utc_offset, hours_24 }).into()
    }

    /// Frees the coins of a payment marked not confirmed.
    pub async fn release_payment(&self, tx: String) -> Result<()> {
        Ok(self.inner.release_payment(&tx).await?)
    }

    /// This wallet's Manual Sync settings.
    pub async fn manual_settings(&self) -> ManualSettings {
        let s = self.inner.manual_settings().await;
        ManualSettings { on: s.on, pull_mode: s.pull_mode.into(), incoming_scan_blocks: s.incoming_scan_blocks, settled_confirmations: s.settled_confirmations }
    }

    pub async fn set_manual_sync(&self, on: bool) -> Result<()> {
        Ok(self.inner.set_manual_sync(on).await?)
    }

    pub async fn set_pull_mode(&self, mode: PullMode) -> Result<()> {
        Ok(self.inner.set_pull_mode(mode.into()).await?)
    }

    pub async fn set_incoming_scan_blocks(&self, blocks: u32) -> Result<()> {
        Ok(self.inner.set_incoming_scan_blocks(blocks).await?)
    }

    pub async fn set_settled_confirmations(&self, confirmations: u32) -> Result<()> {
        Ok(self.inner.set_settled_confirmations(confirmations).await?)
    }

    /// The Scan drawer's words, having asked the node where the chain is.
    pub async fn scan_drawer(&self, now: u64, utc_offset: i32, hours_24: bool) -> ScanDrawer {
        self.inner.refresh_tip().await;
        self.scan_drawer_known(now, utc_offset, hours_24).await
    }

    /// The Scan drawer's words from where the chain was last seen, without
    /// asking the node: something to show at once while `scan_drawer` asks.
    pub async fn scan_drawer_known(&self, now: u64, utc_offset: i32, hours_24: bool) -> ScanDrawer {
        ScanDrawer {
            last_scan: fob_wallet::words::manual::last_scan(self.inner.last_scan(clock::Clock { now, utc_offset, hours_24 }).await.as_ref()),
            options: self
                .inner
                .scan_options()
                .await
                .into_iter()
                .map(|o| {
                    let said = fob_wallet::words::manual::option(&o);
                    ScanOption { request: o.request.into(), label: said.label, note: said.note, cost: said.cost, action: said.action }
                })
                .collect(),
        }
    }

    /// What the Block tab shows for a typed block.
    pub async fn block_info(&self, height: u64, now: u64, utc_offset: i32, hours_24: bool) -> BlockInfo {
        let info = fob_wallet::words::manual::block(&self.inner.block_info(height, clock::Clock { now, utc_offset, hours_24 }).await);
        BlockInfo { mined: info.mined, newest: info.newest, first: info.first }
    }

    /// Starts what the Scan drawer was asked for. A check of payments is done
    /// when this returns, with its words; a scan of blocks carries on, in
    /// `status`'s `scan`.
    pub async fn start_scan(&self, request: ScanRequest) -> Result<Option<PullOutcome>> {
        Ok(self.inner.start_scan(request.into()).await?.map(Into::into))
    }

    /// Stops the scan someone asked for. What it found stays.
    pub async fn stop_scan(&self) {
        self.inner.stop_scan().await;
    }

    /// Pulling down on Home with Manual Sync on. None when it's disabled.
    pub async fn pull_refresh(&self) -> Result<Option<PullOutcome>> {
        Ok(self.inner.pull_refresh().await?.map(Into::into))
    }

    /// `status`'s notice has been said.
    pub fn clear_notice(&self) {
        self.inner.clear_notice();
    }

    /// Scans the whole wallet again, past a reorganisation too deep to check.
    pub async fn scan_again(&self) -> Result<()> {
        Ok(self.inner.scan_again().await?)
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
        self.inner.price_toast(clock::Clock { now, utc_offset, hours_24 }).await.map(Into::into)
    }

    /// The toast's Retry: asks for the price again and says what came of it.
    pub async fn retry_prices(&self, now: u64, utc_offset: i32, hours_24: bool) -> PriceRetry {
        match self.inner.retry_prices(clock::Clock { now, utc_offset, hours_24 }).await {
            fob_wallet::Retried::Updated { title, line } => PriceRetry::Updated { title, line },
            fob_wallet::Retried::Failed(toast) => PriceRetry::Failed { toast: toast.into() },
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
        Ok(self.inner.set_currency(code.as_deref()).await?.map(fob_wallet::words::prices::moved))
    }

    /// Prices from `provider`, one that prices the currency. Returns words
    /// to show when the transport had to change for it.
    pub async fn set_price_provider(&self, provider: PriceProvider) -> Result<Option<String>> {
        Ok(self.inner.set_price_provider(provider.into()).await?.map(fob_wallet::words::prices::moved))
    }

    /// Price from, as it opens: every provider that prices the chosen
    /// currency asked at once (8 s each at most); none with XMR.
    pub async fn check_prices(&self) -> Option<PriceChecks> {
        let checks = self.inner.check_prices().await?;
        let summary = fob_wallet::words::prices::checks_summary(&checks);
        let lines = checks
            .into_iter()
            .filter_map(|c| {
                let label = fob_wallet::words::prices::check_label(&c);
                Some(PriceCheck {
                    provider: price::Provider::named(c.name)?.into(),
                    name: c.name.into(),
                    web: c.web,
                    tor: c.tor,
                    chosen: c.chosen,
                    rate: c.rate,
                    failure: c.failure.map(|f| fob_wallet::words::prices::failure(f).to_owned()),
                    can_pick: c.can_pick,
                    label,
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

    /// Builds a payment for the confirm screen. `amount` is as typed in the
    /// amount field: in money (converted at the current price) or XMR.
    /// `note` (typed on Send, or the payment code's description) is kept
    /// with the payment. `code` is the payment code the form was filled
    /// from, as read: its name goes with the payment while `to` is its
    /// address, its receipt while the amount is its too.
    pub async fn prepare(&self, to: String, amount: TypedAmount, note: Option<String>, code: Option<Arc<form::ScannedCode>>) -> Result<Prepared> {
        let p = self.inner.prepare_typed(&to, amount.typed(), note, code.as_deref().map(form::ScannedCode::request)).await?;
        Ok(prepared(&self.inner, p).await)
    }

    /// What [`prepare`](FobWallet::prepare) would pay, checked the same,
    /// without asking the node: Confirm shows it at once, the fee to come.
    pub async fn preview(&self, to: String, amount: TypedAmount, code: Option<Arc<form::ScannedCode>>) -> Result<PaymentPreview> {
        let p = self.inner.preview_typed(&to, amount.typed(), code.as_deref().map(form::ScannedCode::request)).await?;
        // Only a price already kept: the preview asks nothing.
        let receipt = match p.receipt {
            Some(r) => Some(Receipt::checked(&self.inner, r, p.amount, false).await),
            None => None,
        };
        let distinct = receipt.as_ref().is_some_and(|r| r.other_currency.is_some());
        Ok(PaymentPreview { to: p.to, amount_text: exact(p.amount), amount_fiat: worth(&self.inner, p.amount, distinct).await, receipt })
    }

    /// Split up's screen for `counts` pieces of each note (none as it
    /// opens), with Auto learned from the 30 days to `now`.
    pub async fn split_view(&self, counts: Vec<u32>, now: u64, utc_offset: i32, hours_24: bool) -> SplitView {
        self.inner.split_view(&counts, clock::Clock { now, utc_offset, hours_24 }).await.into()
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

    /// Gets the node ready for a payment, for when the Send screen opens:
    /// opens the connection if it has gone quiet and asks what building a
    /// payment needs, so that `prepare` a moment later waits for the node
    /// once, not several times. Changes nothing, fails quietly, and is cheap
    /// to call again (what it fetched is kept for twenty seconds).
    pub async fn warm_up_send(&self) {
        self.inner.warm_up_send().await;
    }

    /// Whether the GPU failed this wallet's check, so the CPU scans it.
    pub async fn scan_gpu_check_failed(&self) -> bool {
        self.inner.gpu_check_failed().await
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

    /// An amount field for `amount`, typed in money or XMR; in
    /// money only with a currency besides XMR and a price from within the
    /// day, its age said once it's aged (the clock as for `activity_days`).
    /// Money typed against a price 5 minutes old has Rust ask again; a
    /// failure comes as the price toast.
    pub async fn amount_entry(&self, amount: TypedAmount, now: u64, utc_offset: i32, hours_24: bool) -> AmountEntry {
        self.inner.amount_field(amount.typed(), clock::Clock { now, utc_offset, hours_24 }).await.into()
    }

    /// An amount field's text switched between money and XMR, to `to`:
    /// the typed text converted, or blank.
    pub async fn convert_amount(&self, text: String, to: AmountUnit) -> String {
        self.inner.convert_amount(&text, to.into()).await
    }

    /// A payment code to show: the address and, if typed, an amount (in
    /// XMR or money) and a note for the payer. Throws when the amount
    /// isn't one.
    pub async fn receive_code(&self, address: String, amount: Option<TypedAmount>, note: Option<String>) -> Result<ReceiveCode> {
        let r = self.inner.receive_code(&address, amount.as_ref().map(TypedAmount::typed), note.as_deref()).await?;
        Ok(ReceiveCode { uri: r.uri, headline: r.headline, detail: r.detail })
    }

    /// The payment `tx` for its own screen; the clock as for `activity_days`.
    pub async fn payment(&self, tx: String, now: u64, utc_offset: i32, hours_24: bool) -> Option<PaymentDetail> {
        let d = fob_wallet::words::activity::detail(self.inner.payment(&tx, clock::Clock { now, utc_offset, hours_24 }).await?);
        let unanswered = self.inner.unanswered(&tx).await.then(|| fob_wallet::words::activity::UNANSWERED.to_owned());
        Some(PaymentDetail {
            amount_text: d.amount,
            amount_exact: d.exact,
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
            can_release: d.can_release,
            unanswered,
        })
    }

    /// Contacts by name, those whose name or note has `search` in it (any
    /// case; blank for all); the clock as for `activity_days`.
    pub async fn contacts(&self, search: String, now: u64, utc_offset: i32, hours_24: bool) -> Vec<ContactRow> {
        self.inner.contacts(&search, clock::Clock { now, utc_offset, hours_24 }).await.into_iter().map(Into::into).collect()
    }

    pub async fn contact(&self, id: u64) -> Option<Contact> {
        self.inner.contact(id).await.map(Into::into)
    }

    /// A contact's photo (a WebP for `decode_photo`), for a row's
    /// `photo_key`: asked once per key and kept drawn by the app, rather
    /// than carried in every row. None without one, or once deleted.
    pub async fn contact_photo(&self, id: u64) -> Option<Vec<u8>> {
        self.inner.contact_photo(id).await
    }

    /// A contact's card, or none once it's deleted.
    pub async fn contact_card(&self, id: u64, now: u64, utc_offset: i32, hours_24: bool) -> Option<ContactCard> {
        let c = fob_wallet::words::contacts::card(self.inner.contact_card(id, clock::Clock { now, utc_offset, hours_24 }).await?);
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
            half_title: c.half.as_ref().map(|h| h.0.clone()),
            half_line: c.half.map(|h| h.1),
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
        let o = fob_wallet::words::contacts::offer(self.inner.save_offer(&tx).await?);
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
    /// address, for `contact` (or no one), asking `amount` (in XMR or
    /// money) with `note`. Throws when the amount isn't one, or the
    /// contact was deleted.
    pub async fn make_request(&self, contact: Option<u64>, amount: Option<TypedAmount>, note: Option<String>) -> Result<MadeRequest> {
        let (r, code) = self.inner.make_request(contact, amount.as_ref().map(TypedAmount::typed), note.as_deref()).await?;
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
        let d = fob_wallet::words::requests::detail(self.inner.request(id, clock::Clock { now, utc_offset, hours_24 }).await?);
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

    /// The note prepared payment `id` is recorded with, as typed on
    /// Confirm; empty clears it. Throws once it's sent or forgotten.
    pub async fn set_note(&self, id: u64, note: Option<String>) -> Result<()> {
        Ok(self.inner.set_note(id, note).await?)
    }

    /// Your profile, once it's set up.
    pub async fn profile(&self) -> Option<Profile> {
        self.inner.profile().await.map(Into::into)
    }

    /// Settings' row: "Your profile" over "Henry · share as a code", or
    /// only "Add a shareable Fob contact" before there's a name.
    pub async fn profile_row(&self) -> ProfileRow {
        let row = fob_wallet::words::profile::settings_row(self.inner.profile_row().await);
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

    /// Meet's QR code tab finished with `name`: share `id` is kept as met.
    pub async fn share_met(&self, id: u64, name: String) -> Result<()> {
        Ok(self.inner.share_met(id, &name).await?)
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
            let (title, line) = fob_wallet::words::profile::swapped_words(&scanned.code.name, sure);
            let text = if scanned.code.name.is_empty() {
                uri::nameless_uri(&scanned.code.address, false)
            } else {
                uri::profile_uri(&scanned.code.address, &scanned.code.name, scanned.code.photo.as_deref())
            };
            Swapped { share: share.id, title, line, text, profile: scanned.into() }
        }))
    }

    /// Your shares, newest first; the clock as for `activity_days`.
    pub async fn profile_shares(&self, now: u64, utc_offset: i32, hours_24: bool) -> Vec<ShareLine> {
        let clock = clock::Clock { now, utc_offset, hours_24 };
        self.inner
            .profile_shares(clock)
            .await
            .into_iter()
            .map(fob_wallet::words::profile::line)
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
    /// name is (or looks like) one's and it wasn't asked. `met`: it came
    /// from Meet (a touch, or both codes), so they have your address too;
    /// otherwise the contact is half set up.
    pub async fn add_contact_from_profile(&self, text: String, as_new: bool, met: bool) -> Result<Contact> {
        Ok(self.inner.add_contact_from_profile(&text, as_new, met).await?.into())
    }

    /// "Update Sam" on a scanned profile, once asked; `met` as above.
    pub async fn update_contact_from_profile(&self, id: u64, text: String, met: bool) -> Result<Contact> {
        Ok(self.inner.update_contact_from_profile(id, &text, met).await?.into())
    }

    /// A share's code as Meet's second one, once the other phone's was
    /// scanned: the same address, marked as a reply.
    pub async fn share_reply_code(&self, id: u64) -> Option<String> {
        self.inner.share_reply_code(id).await
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
    /// By two codes, with whom.
    Met { name: String },
}

impl From<ShareKind> for profile::ShareKind {
    fn from(k: ShareKind) -> Self {
        match k {
            ShareKind::CodeShown => profile::ShareKind::CodeShown,
            ShareKind::LinkCopied => profile::ShareKind::LinkCopied,
            ShareKind::LinkShared => profile::ShareKind::LinkShared,
            ShareKind::TouchedPhones { name } => profile::ShareKind::TouchedPhones { name },
            ShareKind::Met { name } => profile::ShareKind::Met { name },
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
            profile::ShareKind::Met { name } => ShareKind::Met { name },
        }
    }
}

/// Touch phones' swap for one Share my profile screen. The app's card
/// service and reader hand it the bytes NFC brings and send what it gives
/// back; the protocol is all here.
#[derive(uniffi::Object)]
pub struct TouchSwap {
    inner: Arc<fob_wallet::swap::Session>,
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
            fob_wallet::swap::Step::Send(apdu) => ReaderStep::Send { apdu },
            fob_wallet::swap::Step::Done(_) => ReaderStep::Done,
            fob_wallet::swap::Step::Failed(problem) => ReaderStep::Failed { reason: fob_wallet::words::errors::swap_failed(&problem) },
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

/// Touch to pay, the card side: Receive's payment code, answered chunk by
/// chunk to a reader on Send or Home. The protocol is all in Rust.
#[derive(uniffi::Object)]
pub struct PayCard {
    inner: fob_wallet::pay::PayCard,
}

#[uniffi::export]
impl PayCard {
    /// The card for `code`, the payment code Receive shows; throws when it's
    /// too big to send.
    #[uniffi::constructor]
    pub fn new(code: String) -> Result<Arc<PayCard>> {
        Ok(Arc::new(PayCard { inner: fob_wallet::pay::PayCard::new(&code)? }))
    }

    pub fn card_answer(&self, apdu: Vec<u8>) -> Vec<u8> {
        self.inner.answer(&apdu)
    }

    pub fn card_lost(&self) {
        self.inner.lost()
    }

    /// A reader took the whole code.
    pub fn read(&self) -> bool {
        self.inner.read()
    }
}

/// What a payment reader sends next.
#[derive(uniffi::Enum)]
pub enum PayStep {
    Send { apdu: Vec<u8> },
    /// The payment code the other phone shows.
    Done { code: String },
    Failed { reason: String },
}

/// Touch to pay, the reader side: Send's (or Home's) reading of a phone on
/// Receive.
#[derive(uniffi::Object)]
pub struct PayReader {
    inner: fob_wallet::pay::PayReader,
}

#[uniffi::export]
impl PayReader {
    #[uniffi::constructor]
    pub fn new(network: Network) -> Arc<PayReader> {
        Arc::new(PayReader { inner: fob_wallet::pay::PayReader::new(network.into()) })
    }

    pub fn start(&self) -> Vec<u8> {
        self.inner.start()
    }

    pub fn next(&self, response: Vec<u8>) -> PayStep {
        match self.inner.next(&response) {
            fob_wallet::pay::Step::Send(apdu) => PayStep::Send { apdu },
            fob_wallet::pay::Step::Done(code) => PayStep::Done { code },
            fob_wallet::pay::Step::Failed(problem) => PayStep::Failed { reason: fob_wallet::words::errors::pay_failed(&problem) },
        }
    }

    pub fn lost(&self) {
        self.inner.lost()
    }
}

/// Whether a command selects the payment application, not the swap's.
#[uniffi::export]
pub fn pay_is_select(apdu: Vec<u8>) -> bool {
    fob_wallet::pay::is_select(&apdu)
}

/// What the card service answers a payment select while Receive isn't in front.
#[uniffi::export]
pub fn pay_not_available() -> Vec<u8> {
    fob_wallet::pay::NOT_AVAILABLE.to_vec()
}

/// What the card service answers while no Share my profile is in front:
/// "not available", whoever asks.
#[uniffi::export]
pub fn swap_not_available() -> Vec<u8> {
    fob_wallet::swap::NOT_AVAILABLE.to_vec()
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
    pub kind: ProfileChangeKind,
    /// "Name", "Photo", "Send address"
    pub label: String,
    pub now: String,
    pub new: String,
}

/// What a change to a contact is to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ProfileChangeKind {
    Name,
    Photo,
    SendAddress,
}

impl From<profile::ChangeKind> for ProfileChangeKind {
    fn from(k: profile::ChangeKind) -> Self {
        match k {
            profile::ChangeKind::Name => ProfileChangeKind::Name,
            profile::ChangeKind::Photo => ProfileChangeKind::Photo,
            profile::ChangeKind::SendAddress => ProfileChangeKind::SendAddress,
        }
    }
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
    /// Meet's second code: their phone already scanned yours.
    pub reply: bool,
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
        let s = fob_wallet::words::profile::scanned(s);
        ScannedProfile {
            reply: s.code.reply,
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
                changes: c.changes.into_iter().map(|c| ProfileChange { kind: c.kind.into(), label: c.label, now: c.now, new: c.new }).collect(),
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
    uri::parse_profile(network.into(), &text).err().map(|e| fob_wallet::words::errors::code(&e))
}

/// Why Meet can't take a scanned code; none for a profile, or an address from
/// someone with no profile.
#[uniffi::export]
pub fn meet_code_problem(network: Network, text: String) -> Option<String> {
    uri::parse_meet_code(network.into(), &text).err().map(|e| fob_wallet::words::errors::code(&e))
}

/// Whether a code Meet took came with no name, so its receiver must give one.
#[uniffi::export]
pub fn meet_code_needs_name(network: Network, text: String) -> bool {
    uri::parse_meet_code(network.into(), &text).is_ok_and(|code| code.name.is_empty())
}

/// A nameless Meet code with the name its receiver chose, as a profile's code.
#[uniffi::export]
pub fn name_meet_code(network: Network, text: String, name: String) -> Result<String> {
    Ok(uri::name_meet_code(network.into(), &text, &name)?)
}

/// Reads a scanned or pasted payment code.
#[uniffi::export]
pub fn parse_payment(network: Network, text: String) -> Result<PaymentRequest> {
    Ok(uri::parse_payment(network.into(), &text)?.into())
}

impl From<uri::PaymentRequest> for PaymentRequest {
    fn from(r: uri::PaymentRequest) -> Self {
        PaymentRequest {
            amount_text: r.amount.map(exact),
            receipt_json: r.receipt.as_ref().and_then(receipt::to_json),
            receipt: r.receipt.as_ref().map(|receipt| receipt::ReceiptView::from(receipt).into()),
            address: r.address,
            amount: r.amount,
            recipient_name: r.recipient_name,
            description: r.description,
        }
    }
}

/// An amount as the amount field took it: typed in XMR, or in money that
/// Rust converts at the current price.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TypedAmount {
    Xmr { text: String },
    Fiat { text: String },
}

impl TypedAmount {
    /// As the core takes it.
    pub fn typed(&self) -> amount::Typed<'_> {
        match self {
            TypedAmount::Xmr { text } => amount::Typed::Xmr(text),
            TypedAmount::Fiat { text } => amount::Typed::Fiat(text),
        }
    }

    /// `text` typed in `unit`.
    pub fn new(text: String, unit: AmountUnit) -> TypedAmount {
        match unit {
            AmountUnit::Xmr => TypedAmount::Xmr { text },
            AmountUnit::Fiat => TypedAmount::Fiat { text },
        }
    }
}

/// What an amount field is typed in: XMR, or money at the current price.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum AmountUnit {
    #[default]
    Xmr,
    Fiat,
}

impl AmountUnit {
    /// What the field's chip switches it to.
    pub fn other(self) -> AmountUnit {
        amount::Unit::from(self).other().into()
    }
}

impl From<amount::Unit> for AmountUnit {
    fn from(u: amount::Unit) -> Self {
        match u {
            amount::Unit::Xmr => AmountUnit::Xmr,
            amount::Unit::Fiat => AmountUnit::Fiat,
        }
    }
}

impl From<AmountUnit> for amount::Unit {
    fn from(u: AmountUnit) -> Self {
        match u {
            AmountUnit::Xmr => amount::Unit::Xmr,
            AmountUnit::Fiat => amount::Unit::Fiat,
        }
    }
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
    /// What it's typed in: money only while there's a price and the field
    /// is switched to it.
    pub typed_in: AmountUnit,
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
    pub for_typed_in: AmountUnit,
    /// The line without the rate, for a field with no room for it.
    pub conversion: Option<String>,
}

impl From<price::AmountField> for AmountEntry {
    fn from(f: price::AmountField) -> Self {
        let f = fob_wallet::words::prices::field(f);
        AmountEntry {
            typed_in: f.typed_in.into(),
            sign: f.sign,
            unit: f.unit,
            can_switch: f.can_switch,
            line: f.line,
            label: f.label,
            switch_label: f.switch_label,
            sendable: f.sendable,
            for_text: f.for_text,
            for_typed_in: f.for_typed_in.into(),
            conversion: f.conversion,
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
    contacts::paid_at_note(&addresses).as_ref().map(fob_wallet::words::contacts::paid_at_note)
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
    /// Names the photo (the contact's id and a hash of it), for the app to
    /// keep it drawn by; its bytes come from `contact_photo`. None without one.
    pub photo_key: Option<String>,
    pub send_address: Option<String>,
    /// "Lunch money · paid 27 Oct" on Contacts, "Lunch · 27 Oct" on Send.
    pub line: String,
}

impl From<contacts::Row> for ContactRow {
    fn from(r: contacts::Row) -> Self {
        let r = fob_wallet::words::contacts::row(r);
        ContactRow { id: r.id, name: r.name, initial: r.initial, photo_key: r.photo_key, send_address: r.send_address, line: r.line }
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
    /// Half set up (they have no address of yours yet): "Half set up".
    pub half_title: Option<String>,
    /// "Sam doesn't have an address from you yet. Meet to give them one, …"
    pub half_line: Option<String>,
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
    let t = fob_wallet::words::contacts::edit_texts(&name, new);
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
    let t = fob_wallet::words::contacts::scan_texts(name.as_deref());
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
    let f = fob_wallet::words::contacts::found(contacts::found(network.into(), current.as_deref(), &text)?, name.as_deref());
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
    /// "+0.5", "−0.1421", cut to four decimals as in Activity.
    pub amount_text: String,
    /// Every digit, for the details: "0.142135791113 XMR".
    pub amount_exact: String,
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
    /// Sent, not confirmed: its coins can be released.
    pub can_release: bool,
    /// "The node didn't answer, so Fob keeps sending it until it does.": the
    /// node gave no answer when it was sent and Fob still offers it with each
    /// sync; none once it answered, or for a payment in.
    pub unanswered: Option<String>,
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
    address::short(&address)
}

/// An address in groups of five, the first, middle and last highlighted.
#[uniffi::export]
pub fn address_groups(address: String) -> Vec<AddressGroup> {
    address::address_groups(&address).into_iter().map(|g| AddressGroup { text: g.text, highlighted: g.highlighted }).collect()
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
    let p = fob_wallet::preview_wallet_code(network.into(), &code)?;
    Ok(WalletCodePreview {
        restore_time: p.restore_height.map(|h| fob_wallet::Network::from(network).timestamp_for(h)),
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
    /// Its colour: red, green and blue, one byte each. Never changes with the theme.
    pub rgb: u32,
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
    let (r, g, b) = pattern::COLOURS[usize::from(p.colour) % pattern::COLOURS.len()];
    AddressPattern { cells: p.cells, colour: p.colour, dots: p.dots(), rgb: u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b) }
}

/// A node, and whether its self-signed certificate is accepted.
#[derive(uniffi::Record)]
pub struct NodeSetting {
    pub url: String,
    pub self_signed: bool,
}

impl From<fob_wallet::node::Setting> for NodeSetting {
    fn from(s: fob_wallet::node::Setting) -> Self {
        NodeSetting { url: s.url, self_signed: s.self_signed }
    }
}

impl From<NodeSetting> for fob_wallet::node::Setting {
    fn from(s: NodeSetting) -> Self {
        fob_wallet::node::Setting { url: s.url, self_signed: s.self_signed }
    }
}

/// A light wallet server, and whether its own certificate is accepted.
#[derive(uniffi::Record)]
pub struct LightSetting {
    pub url: String,
    pub self_signed: bool,
}

/// One line of a light wallet server's Test: "Network" · "stagenet".
#[derive(uniffi::Record)]
pub struct LightReportRow {
    pub label: String,
    pub value: String,
    /// This line is why the server can't be used.
    pub bad: bool,
}

/// What the Light server screen's Test found, every word from Rust.
#[derive(uniffi::Record)]
pub struct LightReport {
    /// "lws.example:8443".
    pub address: String,
    /// "Ready to use", or why it can't be.
    pub verdict: String,
    /// "Sync through this server" can go ahead.
    pub usable: bool,
    pub rows: Vec<LightReportRow>,
}

/// The server in use, on the Light server screen before any Test: "Certificate
/// key" · "ab12 cd34 …", "Amounts" · "the server's word", and the sentences
/// that go under them.
#[derive(uniffi::Record)]
pub struct LightStanding {
    pub rows: Vec<LightReportRow>,
    pub notes: Vec<String>,
}

/// The words of Settings' Node card and the Light server screen.
#[derive(uniffi::Record)]
pub struct LightWords {
    /// "Sync through", and its two choices.
    pub sync_through: String,
    pub through_node: String,
    pub through_server: String,
    /// "Light wallet server": the row and the screen's title.
    pub server_title: String,
    pub none_chosen: String,
    pub url_label: String,
    pub url_hint: String,
    pub self_signed: String,
    pub test: String,
    pub not_tested: String,
    /// The orange button.
    pub use_server: String,
    /// On the screen, and before light mode turns on.
    pub privacy: String,
    /// Before light mode turns off.
    pub turning_off: String,
}

/// A step of connecting a Trezor wallet, counted off at the card's corner.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrezorStep {
    Unlock,
    Wallet,
    Allow,
    Address,
}

impl From<fob_wallet::words::trezor::Step> for TrezorStep {
    fn from(step: fob_wallet::words::trezor::Step) -> Self {
        use fob_wallet::words::trezor::Step as S;
        match step {
            S::Unlock => TrezorStep::Unlock,
            S::Wallet => TrezorStep::Wallet,
            S::Allow => TrezorStep::Allow,
            S::Address => TrezorStep::Address,
        }
    }
}

/// The Trezor card: an eyebrow, a line, the words under it.
#[derive(uniffi::Record, Clone, Debug)]
pub struct TrezorCard {
    pub eyebrow: String,
    pub title: String,
    pub body: String,
}

impl From<fob_wallet::words::trezor::Card> for TrezorCard {
    fn from(c: fob_wallet::words::trezor::Card) -> Self {
        TrezorCard { eyebrow: c.eyebrow, title: c.title, body: c.body }
    }
}

/// A failed exchange's card, and whether Try again is offered.
#[derive(uniffi::Record, Clone, Debug)]
pub struct TrezorProblem {
    pub card: TrezorCard,
    pub retry: bool,
    /// The address differed: the one red state.
    pub stop: bool,
}

/// The fixed words of the Trezor screen.
#[derive(uniffi::Record, Clone, Debug)]
pub struct TrezorWords {
    pub your_trezor: String,
    pub looking_hint: String,
    pub try_again: String,
    pub open_wallet: String,
    pub no_passphrase: String,
    pub on_the_trezor: String,
    pub type_here: String,
    pub enter_on_trezor_instead: String,
    pub passphrase_field: String,
    pub close: String,
    pub pin_enter: String,
    pub pin_delete: String,
    pub kept_if_cancelled: String,
}

#[uniffi::export]
pub fn trezor_words() -> TrezorWords {
    use fob_wallet::words::trezor as w;
    TrezorWords {
        your_trezor: w::YOUR_TREZOR.into(),
        looking_hint: w::LOOKING_HINT.into(),
        try_again: w::TRY_AGAIN.into(),
        open_wallet: w::OPEN_WALLET.into(),
        no_passphrase: w::NO_PASSPHRASE.into(),
        on_the_trezor: w::ON_THE_TREZOR.into(),
        type_here: w::TYPE_HERE.into(),
        enter_on_trezor_instead: w::ENTER_ON_TREZOR_INSTEAD.into(),
        passphrase_field: w::PASSPHRASE_FIELD.into(),
        close: w::CLOSE.into(),
        pin_enter: w::PIN_ENTER.into(),
        pin_delete: w::PIN_DELETE.into(),
        kept_if_cancelled: w::KEPT_IF_CANCELLED.into(),
    }
}

/// The steps a device with these protections takes a person through.
#[uniffi::export]
pub fn trezor_steps(pin_protection: bool, passphrase_protection: bool) -> Vec<TrezorStep> {
    fob_wallet::words::trezor::Device { pin: pin_protection, passphrase: passphrase_protection }.steps().into_iter().map(Into::into).collect()
}

/// The step a device screen belongs to, by the name its firmware gives it;
/// none for a screen outside connecting (a sync, a payment).
#[uniffi::export]
pub fn trezor_step_for(what: String) -> Option<TrezorStep> {
    fob_wallet::words::trezor::step_for(&what).map(Into::into)
}

#[uniffi::export]
pub fn trezor_step_name(step: TrezorStep) -> String {
    use fob_wallet::words::trezor::Step as S;
    match step {
        TrezorStep::Unlock => S::Unlock,
        TrezorStep::Wallet => S::Wallet,
        TrezorStep::Allow => S::Allow,
        TrezorStep::Address => S::Address,
    }
    .name()
    .into()
}

#[uniffi::export]
pub fn trezor_card_looking() -> TrezorCard {
    fob_wallet::words::trezor::looking().into()
}

#[uniffi::export]
pub fn trezor_card_button(what: String) -> TrezorCard {
    fob_wallet::words::trezor::button(&what).into()
}

/// The passphrase question; `typed` once the field is open.
#[uniffi::export]
pub fn trezor_card_passphrase(typed: bool) -> TrezorCard {
    if typed { fob_wallet::words::trezor::passphrase_typed().into() } else { fob_wallet::words::trezor::passphrase().into() }
}

#[uniffi::export]
pub fn trezor_card_pin() -> TrezorCard {
    fob_wallet::words::trezor::pin().into()
}

#[uniffi::export]
pub fn trezor_card_progress(done: u32, total: u32) -> TrezorCard {
    fob_wallet::words::trezor::progress(done, total).into()
}

/// What a failed exchange says: `reason` as Fob worded it, `cancelled`
/// whether the person said no.
#[uniffi::export]
pub fn trezor_problem(reason: String, cancelled: bool) -> TrezorProblem {
    let p = fob_wallet::words::trezor::problem(&reason, cancelled);
    TrezorProblem { card: p.card.into(), retry: p.retry, stop: p.stop }
}

#[uniffi::export]
pub fn trezor_card_connected(name: String) -> TrezorCard {
    fob_wallet::words::trezor::connected(&name).into()
}

/// The words of Settings' Node card and the Light server screen.
#[uniffi::export]
pub fn light_words() -> LightWords {
    use fob_wallet::words::light as w;
    LightWords {
        sync_through: w::SYNC_THROUGH.into(),
        through_node: w::THROUGH_NODE.into(),
        through_server: w::THROUGH_SERVER.into(),
        server_title: w::SERVER_TITLE.into(),
        none_chosen: w::NONE_CHOSEN.into(),
        url_label: w::URL_LABEL.into(),
        url_hint: w::URL_HINT.into(),
        self_signed: w::SELF_SIGNED.into(),
        test: w::TEST.into(),
        not_tested: w::NOT_TESTED.into(),
        use_server: w::USE.into(),
        privacy: w::PRIVACY.into(),
        turning_off: w::turning_off(),
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

impl From<fob_wallet::node::Mark> for CheckMark {
    fn from(m: fob_wallet::node::Mark) -> Self {
        use fob_wallet::node::Mark as M;
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

impl From<fob_wallet::node::Report> for NodeReport {
    fn from(r: fob_wallet::node::Report) -> Self {
        use fob_wallet::node::Usable;
        NodeReport {
            verdict: fob_wallet::words::node::verdict(r.verdict()).into(),
            mark: r.mark().into(),
            usable: match r.usable() {
                Usable::Use => NodeUse::Use,
                Usable::UseAnyway => NodeUse::UseAnyway,
                Usable::No => NodeUse::No,
            },
            checking: r.checking(),
            rows: r
                .rows
                .iter()
                .map(|row| {
                    use fob_wallet::words::node as said;
                    NodeCheckRow { title: said::title(row.step).into(), result: said::result(&row.outcome), second: said::second(&row.outcome), mark: row.mark.into(), label: said::label(row) }
                })
                .collect(),
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
    /// One this wallet was given, so it can be taken off the list.
    pub removable: bool,
}

impl NodeChoice {
    fn new(report: &fob_wallet::node::Report, line: String, custom: &[fob_wallet::node::Setting]) -> NodeChoice {
        let can_use = report.usable() == fob_wallet::node::Usable::Use;
        NodeChoice {
            node: NodeSetting { url: report.url.clone(), self_signed: report.self_signed },
            address: report.address.clone(),
            line,
            mark: if report.checking() { CheckMark::Checking } else if can_use { CheckMark::Pass } else { CheckMark::Fail },
            can_use,
            removable: custom.iter().any(|node| node.url == report.url),
        }
    }
}

/// Hears a node's checks as each finishes; implemented by the app. Called
/// from Fob's own threads. Fallible for the same reason as [`SyncObserver`].
#[uniffi::export(with_foreign)]
pub trait NodeCheckObserver: Send + Sync {
    fn on_progress(&self, report: NodeReport) -> Result<()>;
}


/// Confirm when a payment needs more than is ready: its card's rows, the
/// alert above the swipe, and the swipe's words. Settling (money on its way
/// covers it), the swipe is a bar filling until enough is ready; otherwise
/// it's locked.
#[derive(uniffi::Record)]
pub struct Shortfall {
    /// "about 0.0218 XMR", and what it's worth: the amount with the fee.
    pub needed_text: String,
    pub needed_fiat: Option<String>,
    pub ready_text: String,
    pub ready_fiat: Option<String>,
    /// Money not ready yet; none when there's none.
    pub on_way_text: Option<String>,
    pub on_way_fiat: Option<String>,
    /// "Short by", or "Short by, for now" while settling.
    pub short_label: String,
    pub short_text: String,
    pub short_fiat: Option<String>,
    pub settling: bool,
    /// The alert: "Not enough to send this", "Not enough ready yet".
    pub title: String,
    pub line: String,
    /// The swipe's words: "Not enough to send", or "Ready in about 14 min".
    pub swipe: String,
    /// How far along the bar is while settling, 0 to 1.
    pub progress: f32,
}

#[uniffi::export(async_runtime = "tokio")]
impl FobWallet {
    /// Confirm's shortfall for a payment needing `needed` (from
    /// [`FobError::NotEnoughFunds`]) at `now`, against what's ready and
    /// what's on its way.
    pub async fn shortfall(&self, needed: u64, now: u64, utc_offset: i32, hours_24: bool) -> Shortfall {
        let clock = clock::Clock { now, utc_offset, hours_24 };
        let waiting = self.inner.not_ready(clock).await;
        let ready = match &waiting {
            Some(n) => n.ready,
            None => self.inner.balance().await.unlocked,
        };
        let s = fob_wallet::not_ready::shortfall(needed, ready, waiting.as_ref());
        let worth = |atomic: u64| async move { self.inner.worth_now(atomic).await };
        let settling = s.ready_after.is_some();
        let (seconds, progress) = s.ready_after.unwrap_or((0, 0.0));
        Shortfall {
            needed_text: format!("about {} XMR", xmr(s.needed)),
            needed_fiat: worth(s.needed).await,
            ready_text: format!("{} XMR", xmr(s.ready)),
            ready_fiat: worth(s.ready).await,
            on_way_text: (s.on_way > 0).then(|| format!("{} XMR", xmr(s.on_way))),
            on_way_fiat: if s.on_way > 0 { worth(s.on_way).await } else { None },
            short_label: if settling { "Short by, for now" } else { "Short by" }.into(),
            short_text: format!("{} XMR", xmr(s.short)),
            short_fiat: worth(s.short).await,
            settling,
            title: if settling { "Not enough ready yet" } else { "Not enough to send this" }.into(),
            line: if settling {
                "Money you received is still settling. Send wakes up when enough is ready."
            } else {
                "This wallet doesn't have enough ready to cover the payment and its fee."
            }
            .into(),
            swipe: if settling { format!("Ready in about {}", fob_wallet::words::catch_up::short_time(seconds.max(60))) } else { "Not enough to send".into() },
            progress: progress.clamp(0.0, 1.0),
        }
    }
}
