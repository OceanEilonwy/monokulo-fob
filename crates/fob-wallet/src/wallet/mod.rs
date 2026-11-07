//! A wallet: its keys, what it has found on the chain, syncing and sending.

mod addresses;
mod connection;
mod funds;
mod lifecycle;
mod manual_sync;
mod people;
mod prices;
mod send;
mod state;
mod sync;
#[cfg(test)]
mod tests;

pub use crate::activity::{Activity, Direction};
pub use crate::catch_up::{ScanProgress, SyncStatus};
#[cfg(test)]
use manual_sync::{SentLookup, key_images_spent, sent_lookup};
use state::{ChainSpend, Owned, Sent, State, confirmations, default_incoming_scan_blocks, default_settled_confirmations};
use sync::{Unpublished, publish_failure};
#[cfg(test)]
use sync::{ReorgReach, pool_left, pool_spends, reorg_reach};

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::db::Db;
use crate::decoys::Decoys;
use crate::send_context::SendContext;
use crate::fetch::{Adaptive, Batch, Pipeline, Tuning};

/// A node, as a raw transport, a daemon, a decoy source and what's been learned about it.
type Connection = (Transport, Daemon, Decoys, Arc<Adaptive>);
use crate::transport::{self, Daemon, Transport};
use monero_wallet::{
    OutputWithDecoys, Scanner, WalletOutput,
    address::{MoneroAddress, SubaddressIndex},
    interface::prelude::*,
    ringct::RctType,
    send::{Change, SendError, SignableTransaction},
    transaction::Timelock,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use crate::{
    activity,
    address::short_title,
    clock::{self, Clock},
    fiat,
    catch_up,
    amount,
    contacts::{self, Contact},
    error::{CodeError, EndpointProblem, FobError, MoneroError, PriceError, Result, ScreenError, VaultError},
    backup,
    keys::{Keys, SeedKind, StoredKeys},
    manual::{self, ManualSettings, PullMode, PullOutcome, ScanOption, ScanPlan, ScanRequest, ScanTitle},
    network::Network,
    node,
    not_ready::{self, Coins, NotReady, Piece, Source},
    price::{self, Source as _},
    receipt::ReceiptView,
    profile, requests, scan, split, store, swap,
    uri::{self, WalletCode},
    network::{UNLOCK, ZmqStatus},
    zmq,
};


/// The most blocks scanned per sync step: whatever the pipeline has ready,
/// up to this, so progress shows often.
const BATCH: u64 = 500;
/// Subaddresses watched past the last one handed out or paid: a restore
/// finds payments up to this many addresses past the highest it has found
/// so far (wallet2's gap limit). Requests and shares each take one, often
/// never paid.
const LOOKAHEAD: u32 = 200;
/// How far past the highest address a restore found its first new one
/// goes, so another phone's handed out but unpaid addresses (requests,
/// shares) aren't given out again.
const RESTORE_SKIP: u32 = 50;
/// Ring size for transactions since the 2022 hard fork.
const RING_LEN: u8 = 16;
/// The most Fob pays per unit of weight, whatever a node asks. With the tail
/// emission, Monero's base fee is at most about 20,000 (with the block weight
/// median at its smallest; larger blocks make it less), and the normal fee
/// four times that: this is five times the most an honest node asks, so a
/// node can't have a payment pay a hundred times the fee.
const MAX_FEE_PER_WEIGHT: u64 = 400_000;
/// How often an up-to-date wallet looks for new blocks.
const POLL: Duration = Duration::from_secs(20);
/// A sent payment the node has never heard of for this long, in seconds, is marked not confirmed.
const NOT_CONFIRMED_AFTER: u64 = 24 * 3600;
/// How often syncing asks about sent payments gone quiet.
const QUIET_CHECK_EVERY: Duration = Duration::from_secs(10 * 60);
/// How often it polls while the node's ZMQ publisher is heard, which
/// announces each block: only in case an announcement goes missing.
const POLL_LISTENING: Duration = Duration::from_secs(120);
/// How long a pool announcement waits before the pool is asked, so a burst
/// of transactions is one look, not one each.
const POOL_GAP: Duration = Duration::from_secs(2);
/// The longest [`Wallet::sync_now`] waits for its round.
const SYNC_NOW_WAIT: Duration = Duration::from_secs(20);
/// How often the file is saved while catching up. A crash loses this much of
/// the scanning, found again from the chain; Cake's autosave is 30 s too.
const SAVE_EVERY: Duration = Duration::from_secs(30);
/// How often the app hears of progress while catching up: each time, it reads
/// its screens again. Cake's listener ticks about this often.
const REPORT_EVERY: Duration = Duration::from_secs(1);
/// How often the app hears how long the node has kept it waiting for blocks.
const WAIT_TICK: Duration = Duration::from_secs(5);
const COINBASE_UNLOCK: u64 = monero_wallet::COINBASE_LOCK_WINDOW as u64;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Where a wallet stands with the GPU it would scan on.
#[derive(Debug, PartialEq)]
enum GpuCheck {
    Passed,
    Failed,
    Due,
}

/// From what the wallet saved and whether it's checked since opening. Another
/// GPU or driver isn't what was checked, and a failure gets one more try each
/// time the wallet opens, so a passing moment can't be missed for good.
fn gpu_check_due(checked_on: Option<&str>, passed: bool, device: &str, checked_since_opening: bool) -> GpuCheck {
    match (checked_on == Some(device), passed, checked_since_opening) {
        (false, _, _) => GpuCheck::Due,
        (true, true, _) => GpuCheck::Passed,
        (true, false, true) => GpuCheck::Failed,
        (true, false, false) => GpuCheck::Due,
    }
}

/// What the sealed file before the database held, as a test writes one.
#[cfg(test)]
#[derive(Serialize)]
struct FileOut<'a> {
    keys: StoredKeys,
    state: &'a State,
}

/// The sealed file wallets were kept in before the database.
#[derive(Deserialize)]
struct FileIn {
    keys: StoredKeys,
    state: State,
}

/// Money in the wallet, in atomic units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Balance {
    pub total: u64,
    /// What can be sent now.
    pub unlocked: u64,
}

/// What the screens list, read together by [`Wallet::lists`].
pub struct Lists {
    /// Home's Activity.
    pub days: Vec<activity::Day>,
    /// Every contact by name.
    pub contacts: Vec<contacts::Row>,
    /// Addresses paid that are no contact's.
    pub unsaved: Vec<activity::PaidBefore>,
    /// The contacts Send's sheet offers.
    pub on_send: Vec<contacts::Row>,
    /// Who this wallet paid before, for Send's sheet while there's no contact.
    pub paid_before: Vec<activity::PaidBefore>,
}

/// Receive's payment code and the words around it.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceiveCode {
    /// `monero:<address>?tx_amount=…&tx_description=…`, always in XMR.
    pub uri: String,
    /// The big line on the expanded code: "$20.00" for an amount typed in
    /// money, else "0.5 XMR".
    pub headline: Option<String>,
    /// The line under it: "0.1204 XMR · Coffee beans", or the note.
    pub detail: Option<String>,
}

/// Told about every change while the wallet syncs.
pub trait SyncListener: Send + Sync {
    fn on_status(&self, status: SyncStatus);
}

/// What the price toast's Retry came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Retried {
    /// It worked: "USD price updated", "1 XMR = $166.10", shown for 2 s.
    Updated { title: String, line: String },
    /// It failed again: the toast with its new words.
    Failed(crate::price_alert::Toast),
}

/// A payment ready to sign: shown on the confirm screen, sent by its `id`.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub id: u64,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub receipt: Option<ReceiptView>,
    /// A split's pieces as the confirm screen lists them, largest first:
    /// ("$20 × 4", "0.2667 XMR"). Empty for a payment.
    pub split: Vec<(String, String)>,
    /// How many pieces, for a split.
    pub pieces: Option<u32>,
}

/// What the confirm screen shows of a payment while its fee is being
/// worked out: what the form and the code say, the node not asked.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub to: String,
    pub amount: u64,
    pub receipt: Option<ReceiptView>,
}

struct PreparedTx {
    signable: SignableTransaction,
    out: Outgoing,
}

/// What a payment records once sent, beside its transaction.
struct Outgoing {
    to: String,
    amount: u64,
    /// The coins it spends, each as [`Owned::id`] names it.
    inputs: Vec<(String, String)>,
    input_total: u64,
    receipt: Option<String>,
    note: Option<String>,
    recipient_name: Option<String>,
    /// How many pieces, for a split.
    pieces: Option<u32>,
    /// A split's pieces, paid back to this wallet; empty for a payment.
    back: Vec<u64>,
}

struct Inner {
    /// The wallet's database.
    path: PathBuf,
    keys: Keys,
    state: Mutex<State>,
    rpc: Mutex<Option<(node::Setting, Connection)>>,
    /// The transport the connection above was made with, and its node: its
    /// byte count and certificate, read by each sync report without
    /// waiting on `rpc`, which a connection being made holds.
    link: std::sync::Mutex<Option<(node::Setting, Transport)>>,
    /// Blocks being fetched ahead of the scanner.
    pipeline: Mutex<Option<Pipeline>>,
    /// The scanner, with how far it watches: 200 addresses are slow to work
    /// out again for every batch.
    scanner: std::sync::Mutex<Option<(u32, Scanner, Arc<fob_scanner::Prefilter>)>>,
    /// This wallet's GPU check has run since it was opened.
    gpu_checked: AtomicBool,
    /// When syncing last looked for sent payments gone quiet.
    quiet_checked: std::sync::Mutex<Option<Instant>>,
    /// With Manual Sync on, the sync loop scans only while this is set.
    scan_asked: AtomicBool,
    /// A scan of chosen blocks is to stop.
    scan_stop: AtomicBool,
    /// The scan someone asked for, while it runs.
    scan_job: std::sync::Mutex<Option<ScanProgress>>,
    /// Held by a pull or a scan of chosen blocks: one at a time.
    scan_running: Mutex<()>,
    /// Scanning since the last save.
    unsaved: AtomicBool,
    /// When the file was last saved.
    saved_at: std::sync::Mutex<Instant>,
    /// The database, held for each write; none once the wallet has been taken
    /// off the phone, which no late save may bring back.
    db: std::sync::Mutex<Option<Db>>,
    prepared: Mutex<Pending<PreparedTx>>,
    next_id: AtomicU64,
    status: std::sync::Mutex<SyncStatus>,
    syncing: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Wakes the sync loop from its wait for the next poll.
    wake: tokio::sync::Notify,
    /// Has the waiting sync loop look at the pool (a ZMQ announcement).
    pool_wake: tokio::sync::Notify,
    /// Who hears the sync loop, kept to restart the ZMQ subscriber.
    listener: std::sync::Mutex<Option<Arc<dyn SyncListener>>>,
    /// The ZMQ subscriber and where it listens, while syncing with instant
    /// updates on.
    zmq_task: std::sync::Mutex<Option<(String, tokio::task::JoinHandle<()>)>>,
    /// Counts the sync loop's rounds that ended caught up or failed, so
    /// [`Wallet::sync_now`] knows when its round is over.
    rounds: tokio::sync::watch::Sender<u64>,
    /// Payments to this wallet still in the node's pool: amount, when seen
    /// and the subaddress paid, by transaction. Not saved: the pool is
    /// asked again after every block.
    pool: std::sync::Mutex<HashMap<String, Pooled>>,
    /// Pool transactions already looked at, so each is fetched once.
    pool_seen: std::sync::Mutex<HashSet<String>>,
    /// The last block scanned, to scan pool transactions against.
    last_block: std::sync::Mutex<Option<monero_wallet::block::Block>>,
    /// Asks for prices every 5 minutes while Fob is on screen.
    prices_task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Has the price task ask at once (a setting changed, a money amount
    /// was typed), for whom it says here.
    prices_wake: tokio::sync::Notify,
    prices_asked: std::sync::Mutex<Option<crate::price_alert::Asked>>,
    /// When a money amount typed last had the price task ask.
    typed_asked: std::sync::Mutex<u64>,
    /// Price lookups started, and the newest of them come back: an older
    /// one coming back later is dropped.
    price_lookups: AtomicU64,
    price_looked_up: AtomicU64,
    /// The run of failed price lookups, for the toast.
    price_alerts: std::sync::Mutex<crate::price_alert::Alerts>,
    /// The node's fee per unit of weight and its rounding mask, as last
    /// asked: Split up's fee "about" before a payment is built.
    fee_rate: std::sync::Mutex<Option<(u64, u64)>>,
    /// What a payment asks the node before it can be built, kept briefly.
    send_context: SendContext,
    /// The share the last "Share my profile" made, while its screen may
    /// still swap by touching phones: a share reopened later never does.
    fresh_share: std::sync::Mutex<Option<u64>>,
    /// The last full check of the node in use, while it stays in use:
    /// Settings' "Working · node.example:38081".
    node_verdict: std::sync::Mutex<Option<(crate::node::Setting, node::Verdict)>>,
    /// The swap running on it, ended at once if its link goes out.
    live_swap: std::sync::Mutex<std::sync::Weak<swap::Session>>,
}

/// How long a prepared payment waits to be sent: one left behind (the
/// confirm screen gone without a word) is forgotten after this.
const PREPARED_FOR: Duration = Duration::from_secs(15 * 60);

/// Payments prepared for the confirm screen, by id, each forgotten
/// [`PREPARED_FOR`] after it was made.
struct Pending<T> {
    items: HashMap<u64, (tokio::time::Instant, T)>,
}

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Pending { items: HashMap::new() }
    }
}

impl<T> Pending<T> {
    fn prune(&mut self) {
        let now = tokio::time::Instant::now();
        self.items.retain(|_, (made, _)| now.duration_since(*made) < PREPARED_FOR);
    }

    fn insert(&mut self, id: u64, item: T) {
        self.prune();
        self.items.insert(id, (tokio::time::Instant::now(), item));
    }

    /// The one with `id`, taken out; `None` once it's sent, cancelled or
    /// too old.
    fn take(&mut self, id: u64) -> Option<T> {
        self.prune();
        self.items.remove(&id).map(|(_, item)| item)
    }
}

/// A payment to this wallet still in the node's pool.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pooled {
    /// When it was first seen.
    seen: u64,
    /// What came to each address it paid, by index (0 is the main one).
    parts: Vec<(u32, u64)>,
}

impl Pooled {
    fn amount(&self) -> u64 {
        self.parts.iter().map(|p| p.1).sum()
    }
}

/// Adds `amount` paid to `subaddress` to `parts`, one entry per address.
fn add_part(parts: &mut Vec<(u32, u64)>, subaddress: u32, amount: u64) {
    match parts.iter_mut().find(|p| p.0 == subaddress) {
        Some(part) => part.1 += amount,
        None => parts.push((subaddress, amount)),
    }
}

/// What the ZMQ subscriber hears, passed to the sync loop and the app.
struct ZmqEvents {
    wallet: Wallet,
    endpoint: String,
    listener: Arc<dyn SyncListener>,
}

impl zmq::Events for ZmqEvents {
    fn status(&self, status: ZmqStatus) {
        let status = self.wallet.set_zmq_status(status, Some(&self.endpoint));
        self.listener.on_status(status);
    }

    fn chain(&self) {
        self.wallet.inner.wake.notify_one();
    }

    fn pool(&self) {
        self.wallet.inner.pool_wake.notify_one();
    }
}

/// A Fob wallet. Cheap to clone; clones share everything.
#[derive(Clone)]
pub struct Wallet {
    inner: Arc<Inner>,
}

/// Where to start scanning, from what someone typed: a block height or a
/// date (YYYY-MM-DD).
pub fn restore_point(network: Network, text: &str) -> Result<u64> {
    let text = text.trim();
    if let Ok(height) = text.parse::<u64>() {
        return Ok(height);
    }
    let parts: Vec<i64> = text.split('-').map(|p| p.parse().map_err(|_| FobError::Code(CodeError::BadCode))).collect::<Result<_>>()?;
    let [y, m, d] = parts[..] else { return Err(FobError::Code(CodeError::BadCode)) };
    if !(2014..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(FobError::Code(CodeError::BadCode));
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Ok(network.restore_height_for(u64::try_from(days * 86_400).unwrap_or(0)))
}

/// The day a restore height was mined, as Keys shows it beside the height
/// ("2 Sep 2026"); none when it would be a guess.
fn restore_date(network: Network, height: u64) -> Option<String> {
    network.dated(height).map(|time| clock::full_date((time / 86_400) as i64))
}

/// What a wallet code holds, shown before restoring from it.
#[derive(Clone, Debug, PartialEq)]
pub struct CodePreview {
    pub address: String,
    pub can_spend: bool,
    /// Where scanning will start, if the code says.
    pub restore_height: Option<u64>,
}

/// Reads a wallet code without restoring anything.
pub fn preview_wallet_code(network: Network, code: &str) -> Result<CodePreview> {
    match uri::parse_wallet_code(network, code)? {
        WalletCode::Words { words, height } => {
            let restored = Keys::from_words(network, &words)?;
            Ok(CodePreview {
                address: restored.keys.primary_address(),
                can_spend: true,
                restore_height: height.or(restored.birthday.map(|b| network.height_at(b))),
            })
        }
        WalletCode::SpendKey { address, spend, view, height } => {
            let keys = Keys::from_spend_key(network, &spend, view.as_deref().map(|v| v.as_str()))?;
            if address.is_some_and(|a| a != keys.primary_address()) {
                return Err(FobError::Vault(VaultError::BadKey));
            }
            Ok(CodePreview { address: keys.primary_address(), can_spend: true, restore_height: height })
        }
        WalletCode::ViewOnly { address, view, height } => {
            let keys = Keys::view_only(network, &address, &view)?;
            Ok(CodePreview { address: keys.primary_address(), can_spend: false, restore_height: height })
        }
    }
}

/// What a sync failure says, and whether it's this device's own fault: the
/// node's words for a node's failure, otherwise the error's own (saving the
/// wallet, say, which is nothing to do with the node).
fn sync_failure(e: &FobError, certificate: Option<std::result::Result<(), transport::CertProblem>>, self_signed: bool) -> (String, bool) {
    match e {
        FobError::Node(text) => (fob_words::node::sync_failure(&node::sync_failure(text, certificate, self_signed)), false),
        e => (fob_words::capitalised(&e.to_string()), e.is_storage()),
    }
}
