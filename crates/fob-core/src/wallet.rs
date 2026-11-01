//! A wallet: its keys, what it has found on the chain, syncing and sending.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::decoys::Decoys;
use crate::fetch::{Adaptive, Pipeline, Tuning};

/// A node, as a raw transport, a daemon, a decoy source and what's been learned about it.
type Connection = (Transport, Daemon, Decoys, Arc<Adaptive>);
use crate::transport::{Daemon, Transport};
use monero_wallet::{
    OutputWithDecoys, Scanner, WalletOutput,
    address::{MoneroAddress, SubaddressIndex},
    interface::prelude::*,
    ringct::RctType,
    send::{Change, SendError, SignableTransaction},
    transaction::{Input, Timelock, Transaction},
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use crate::{
    activity::{self, Clock, short_address},
    catch_up,
    amount,
    contacts::{self, Contact},
    error::{FobError, Result},
    keys::{self, Keys, SeedKind, StoredKeys},
    network::Network,
    node,
    not_ready::{self, Coins, NotReady, Piece, Source},
    price::{self, Source as _},
    receipt::ReceiptView,
    profile, requests, split, store, swap,
    uri::{self, WalletCode},
    zmq::{self, ZmqStatus},
};


/// The most blocks scanned per sync step: whatever the pipeline has ready,
/// up to this, so progress shows often.
const BATCH: u64 = 1_000;
/// Subaddresses watched past the last one handed out or paid: a restore
/// finds payments up to this many addresses past the highest it has found
/// so far (wallet2's gap limit). Requests and shares each take one, often
/// never paid.
const LOOKAHEAD: u32 = 200;
/// How far past the highest address a restore found its first new one
/// goes, so another phone's handed out but unpaid addresses (requests,
/// shares) aren't given out again.
const RESTORE_SKIP: u32 = 50;
/// Recent block hashes kept to notice a reorganisation.
const KEEP_HASHES: usize = 30;
/// Ring size for transactions since the 2022 hard fork.
const RING_LEN: u8 = 16;
/// The most Fob pays per unit of weight, whatever a node asks: about 100× the
/// usual normal fee.
const MAX_FEE_PER_WEIGHT: u64 = 2_000_000;
/// How often an up-to-date wallet looks for new blocks.
const POLL: Duration = Duration::from_secs(20);
/// How often it polls while the node's ZMQ publisher is heard, which
/// announces each block: only in case an announcement goes missing.
const POLL_LISTENING: Duration = Duration::from_secs(120);
/// How long a pool announcement waits before the pool is asked, so a burst
/// of transactions is one look, not one each.
const POOL_GAP: Duration = Duration::from_secs(2);
/// The longest [`Wallet::sync_now`] waits for its round.
const SYNC_NOW_WAIT: Duration = Duration::from_secs(20);
/// Confirmations before money can be spent.
pub(crate) const UNLOCK: u64 = monero_wallet::DEFAULT_LOCK_WINDOW as u64;
const COINBASE_UNLOCK: u64 = monero_wallet::COINBASE_LOCK_WINDOW as u64;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// An output the wallet owns.
#[derive(Clone, Serialize, Deserialize)]
struct Owned {
    /// The `WalletOutput`, serialized, in hex.
    output: String,
    tx: String,
    height: u64,
    time: u64,
    amount: u64,
    subaddress: u32,
    coinbase: bool,
    /// The block its transaction's unlock time names, if any (a mined
    /// reward's is 60 blocks on): it can't be spent before the chain has
    /// that many blocks. 0 for none.
    #[serde(default)]
    unlock_block: u64,
    /// The output's one-time key, in hex: a second output with the same key
    /// is the burning bug and isn't counted.
    #[serde(default)]
    key: String,
    /// `None` in a view-only wallet, which can't tell when it's spent.
    key_image: Option<String>,
    spent_in: Option<String>,
}

/// A transaction on the chain that spent the wallet's outputs.
#[derive(Clone, Serialize, Deserialize)]
struct ChainSpend {
    tx: String,
    height: u64,
    time: u64,
    fee: u64,
}

/// A payment this wallet sent, with what only the sender knows.
#[derive(Clone, Serialize, Deserialize)]
struct Sent {
    tx: String,
    to: String,
    amount: u64,
    fee: u64,
    change: u64,
    time: u64,
    height: Option<u64>,
    /// The ORF receipt JSON from the payment code.
    receipt: Option<String>,
    /// The payment code's description (`tx_description`), if it had one.
    #[serde(default)]
    note: Option<String>,
    /// How many pieces, for a split: money paid back to this wallet's
    /// split address. `amount` is then the pieces' total, and `change`
    /// everything that comes back, pieces and change.
    #[serde(default)]
    pieces: Option<u32>,
    /// Each output coming back to this wallet (a split's pieces, then the
    /// change), as sent, so they show one by one before the payment is
    /// mined, as they will after. Empty in older files: `change` as one.
    #[serde(default)]
    back: Vec<u64>,
    /// The name in the payment code (`recipient_name`), cleaned: who the
    /// code said it was, not checked by anyone.
    #[serde(default)]
    recipient_name: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct State {
    node: String,
    /// The node's certificate is accepted even if self-signed (not
    /// verified, still encrypted). Off in older files.
    #[serde(default)]
    node_self_signed: bool,
    /// The node's ZMQ publisher (`tcp://host:port`, on the node's host), as
    /// last set; kept while turned off, so turning it on again uses it.
    #[serde(default)]
    zmq_endpoint: Option<String>,
    /// Instant updates on. Off by default.
    #[serde(default)]
    zmq_on: bool,
    /// First block to scan. `None` until the first sync of a new wallet,
    /// which starts at the chain's tip.
    restore_height: Option<u64>,
    next_block: u64,
    /// Blocks on the chain at the last look.
    tip: u64,
    last_block_time: u64,
    recent: Vec<(u64, String)>,
    /// The highest subaddress handed out (to Receive, a request or splits).
    receive_index: u32,
    /// The subaddress Receive shows, by index; `None` in files from before
    /// requests, where it was `receive_index`.
    #[serde(default)]
    receive_own: Option<u32>,
    outputs: Vec<Owned>,
    spends: Vec<ChainSpend>,
    sent: Vec<Sent>,
    /// How prices are shown; in dollars, from Kraken over the internet for
    /// a file from before prices.
    #[serde(default)]
    prices: price::Settings,
    /// The last current price and the daily histories asked for.
    #[serde(default)]
    price_cache: price::Cache,
    /// What XMR was worth when each payment moved, by transaction: one
    /// record per currency.
    #[serde(default)]
    price_records: HashMap<String, Vec<price::Record>>,
    /// The note last put in a payment code for each address, by its index:
    /// what payments to that address are for.
    #[serde(default)]
    notes: HashMap<u32, String>,
    /// The address splits pay their pieces to, by index: taken from the
    /// next unused one the first time, never handed out on Receive.
    #[serde(default)]
    split_index: Option<u32>,
    /// The people this wallet pays.
    #[serde(default)]
    contacts: contacts::Book,
    /// Requests to be paid, each on its own subaddress.
    #[serde(default)]
    requests: requests::Book,
    /// Your name and photo, and every share of them.
    #[serde(default)]
    profile: profile::Book,
    /// Restored from words or a code: another phone may have handed out
    /// addresses this file doesn't know.
    #[serde(default)]
    restored: bool,
    /// The first address this restored wallet handed out: payments found
    /// below it went to the other phone's.
    #[serde(default)]
    fresh_from: Option<u32>,
    /// Made here, and its seed phrase not yet said to be written down: Home
    /// reminds until it is, even if Fob was closed during the backup step.
    #[serde(default)]
    backup_pending: bool,
}

#[derive(Serialize)]
struct FileOut<'a> {
    keys: StoredKeys,
    state: &'a State,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

/// One row of Activity: a payment in or out, with its receipt if it had one.
#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    pub tx: String,
    pub direction: Direction,
    /// What moved, without the fee.
    pub amount: u64,
    pub fee: Option<u64>,
    /// Who was paid, when this wallet sent it.
    pub to: Option<String>,
    pub height: Option<u64>,
    pub time: u64,
    pub confirmations: u64,
    /// Confirmations before it's settled: 10, or for a payment in, as many
    /// as its slowest output needs to be spent (60 for a mined reward, more
    /// for one locked to a later block), the not-ready card's rule.
    pub needed: u64,
    pub receipt: Option<ReceiptView>,
    pub receipt_json: Option<String>,
    /// What one XMR was worth when it moved, in the currency prices are
    /// shown in; `None` with XMR only or no price to be had.
    pub price: Option<price::Record>,
    /// The address (by index) a payment in came to; 0 is the main address.
    pub subaddress: Option<u32>,
    /// Its pieces, in atomic units: the outputs received for a payment in,
    /// the outputs spent for one out.
    pub pieces: Vec<u64>,
    /// What it was for: the note in the payment code (Fob's own for its
    /// address, or the payee's description when paying).
    pub note: Option<String>,
    /// A split, with its number of pieces: money paid back to this wallet,
    /// so `amount` is only the fee, what left.
    pub split: Option<u32>,
    /// The contact paid, by name, when this wallet sent it to one of a
    /// contact's addresses; for a payment in, the contact whose request it
    /// paid (unless the request was deleted before it came).
    pub contact: Option<String>,
    /// That contact, by id.
    pub contact_id: Option<u64>,
    /// For a payment in to a share of your profile's address: when that
    /// share was made ("Via your profile", "Shared 09:15").
    pub shared: Option<u64>,
    /// For a payment in: what came to each address it paid, by index, so
    /// one transaction paying two requests credits each its own part.
    pub parts: Vec<(u32, u64)>,
}

impl Activity {
    /// This payment in as far as it came to the address at `subaddress`:
    /// its part there (all of it when it paid that address alone), or
    /// `None` when it paid that address nothing.
    pub fn part(&self, subaddress: u32) -> Option<Activity> {
        if self.direction != Direction::In {
            return None;
        }
        let amount = match self.parts.as_slice() {
            [] => (self.subaddress == Some(subaddress)).then_some(self.amount)?,
            parts => parts.iter().find(|p| p.0 == subaddress)?.1,
        };
        Some(Activity { amount, subaddress: Some(subaddress), parts: vec![(subaddress, amount)], ..self.clone() })
    }
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

/// How far syncing has got.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncStatus {
    pub scanned: u64,
    pub tip: u64,
    pub start: u64,
    pub synced: bool,
    /// 0 to 1, from the restore height to the tip.
    pub progress: f32,
    /// The time left as the chip shows it: none until the speed settles,
    /// counting down, and never up (see [`catch_up`]).
    pub seconds_left: Option<u64>,
    /// Blocks scanned per second over the last few seconds, once settled.
    pub blocks_per_second: Option<f64>,
    /// Megabytes a second the fetcher took from the node over the last few
    /// seconds, once settled.
    pub link_mb_per_second: Option<f64>,
    /// When the last scanned block was mined.
    pub up_to: u64,
    /// Why the last round failed, in words.
    pub error: Option<String>,
    /// The failure is the phone's own (the wallet couldn't be saved), not
    /// the node's.
    pub error_local: bool,
    /// Instant updates from the node's ZMQ publisher.
    pub zmq: ZmqStatus,
    /// [`zmq`](Self::zmq) as Settings shows it: "node.example:18083 · listening".
    pub zmq_text: String,
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
    Failed(price::alert::Toast),
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

struct PreparedTx {
    signable: SignableTransaction,
    out: Outgoing,
}

/// What a payment records once sent, beside its transaction.
struct Outgoing {
    to: String,
    amount: u64,
    inputs: Vec<usize>,
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
    path: PathBuf,
    file_key: Zeroizing<[u8; 32]>,
    keys: Keys,
    state: Mutex<State>,
    rpc: Mutex<Option<(node::Setting, Connection)>>,
    /// The transport the connection above was made with, and its node: its
    /// byte count and certificate, read by each sync report without
    /// waiting on `rpc`, which a connection being made holds.
    link: std::sync::Mutex<Option<(node::Setting, Transport)>>,
    /// Blocks being fetched ahead of the scanner.
    pipeline: Mutex<Option<Pipeline>>,
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
    prices_asked: std::sync::Mutex<Option<price::alert::Asked>>,
    /// When a money amount typed last had the price task ask.
    typed_asked: std::sync::Mutex<u64>,
    /// Price lookups started, and the newest of them come back: an older
    /// one coming back later is dropped.
    price_lookups: AtomicU64,
    price_looked_up: AtomicU64,
    /// The run of failed price lookups, for the toast.
    price_alerts: std::sync::Mutex<price::alert::Alerts>,
    /// The node's fee per unit of weight and its rounding mask, as last
    /// asked: Split up's fee "about" before a payment is built.
    fee_rate: std::sync::Mutex<Option<(u64, u64)>>,
    /// The share the last "Share my profile" made, while its screen may
    /// still swap by touching phones: a share reopened later never does.
    fresh_share: std::sync::Mutex<Option<u64>>,
    /// The last full check of the node in use, while it stays in use:
    /// Settings' "Working · node.example:38081".
    node_verdict: std::sync::Mutex<Option<(crate::node::Setting, &'static str)>>,
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
    let parts: Vec<i64> = text.split('-').map(|p| p.parse().map_err(|_| FobError::BadCode)).collect::<Result<_>>()?;
    let [y, m, d] = parts[..] else { return Err(FobError::BadCode) };
    if !(2014..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(FobError::BadCode);
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
    network.dated(height).map(|time| activity::full_date((time / 86_400) as i64))
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
                return Err(FobError::BadKey);
            }
            Ok(CodePreview { address: keys.primary_address(), can_spend: true, restore_height: height })
        }
        WalletCode::ViewOnly { address, view, height } => {
            let keys = Keys::view_only(network, &address, &view)?;
            Ok(CodePreview { address: keys.primary_address(), can_spend: false, restore_height: height })
        }
    }
}

impl Wallet {
    fn new(path: &Path, file_key: [u8; 32], keys: Keys, restore_height: Option<u64>) -> Result<Wallet> {
        let state = State {
            node: keys.network.default_nodes()[0].to_owned(),
            node_self_signed: false,
            zmq_endpoint: None,
            zmq_on: false,
            restore_height,
            next_block: restore_height.unwrap_or(0),
            tip: 0,
            last_block_time: 0,
            recent: vec![],
            receive_index: 0,
            receive_own: None,
            outputs: vec![],
            spends: vec![],
            sent: vec![],
            prices: price::Settings::default(),
            price_cache: price::Cache::default(),
            price_records: HashMap::new(),
            notes: HashMap::new(),
            split_index: None,
            contacts: contacts::Book::default(),
            requests: requests::Book::default(),
            profile: profile::Book::default(),
            restored: restore_height.is_some(),
            fresh_from: None,
            backup_pending: restore_height.is_none(),
        };
        let wallet = Wallet::from_parts(path, file_key, keys, state);
        wallet.save_state(&wallet.inner.state.try_lock().expect("a new wallet is not shared yet"))?;
        Ok(wallet)
    }

    fn from_parts(path: &Path, file_key: [u8; 32], keys: Keys, state: State) -> Wallet {
        let zmq = if state.zmq_on { ZmqStatus::Connecting } else { ZmqStatus::Off };
        let status = SyncStatus {
            scanned: state.next_block,
            tip: state.tip,
            start: state.restore_height.unwrap_or(state.next_block),
            up_to: state.last_block_time,
            zmq_text: zmq.text(state.zmq_endpoint.as_deref()),
            zmq,
            ..SyncStatus::default()
        };
        Wallet {
            inner: Arc::new(Inner {
                path: path.to_owned(),
                file_key: Zeroizing::new(file_key),
                keys,
                state: Mutex::new(state),
                rpc: Mutex::new(None),
                link: std::sync::Mutex::new(None),
                pipeline: Mutex::new(None),
                prepared: Mutex::new(Pending::default()),
                next_id: AtomicU64::new(1),
                status: std::sync::Mutex::new(status),
                syncing: std::sync::Mutex::new(None),
                wake: tokio::sync::Notify::new(),
                pool_wake: tokio::sync::Notify::new(),
                listener: std::sync::Mutex::new(None),
                zmq_task: std::sync::Mutex::new(None),
                rounds: tokio::sync::watch::Sender::new(0),
                pool: std::sync::Mutex::new(HashMap::new()),
                pool_seen: std::sync::Mutex::new(HashSet::new()),
                last_block: std::sync::Mutex::new(None),
                prices_task: std::sync::Mutex::new(None),
                prices_wake: tokio::sync::Notify::new(),
                prices_asked: std::sync::Mutex::new(None),
                typed_asked: std::sync::Mutex::new(0),
                price_lookups: AtomicU64::new(0),
                price_looked_up: AtomicU64::new(0),
                price_alerts: std::sync::Mutex::new(price::alert::Alerts::default()),
                fee_rate: std::sync::Mutex::new(None),
                fresh_share: std::sync::Mutex::new(None),
                node_verdict: std::sync::Mutex::new(None),
                live_swap: std::sync::Mutex::new(std::sync::Weak::new()),
            }),
        }
    }

    /// A brand-new wallet with a fresh Polyseed, saved at `path`.
    pub fn create(path: &Path, file_key: [u8; 32], network: Network) -> Result<Wallet> {
        Wallet::new(path, file_key, Keys::generate(network).keys, None)
    }

    /// A wallet from seed words. A Polyseed knows its own start; for 25 words
    /// `restore_height` says where to start (block 0 if `None`).
    pub fn restore_words(path: &Path, file_key: [u8; 32], network: Network, words: &str, restore_height: Option<u64>) -> Result<Wallet> {
        let restored = Keys::from_words(network, words)?;
        let height = restore_height.or(restored.birthday.map(|b| network.height_at(b))).unwrap_or(0);
        Wallet::new(path, file_key, restored.keys, Some(height))
    }

    /// A wallet from a scanned wallet code. `restore_height` is used when the
    /// code doesn't say.
    pub fn restore_code(path: &Path, file_key: [u8; 32], network: Network, code: &str, restore_height: Option<u64>) -> Result<Wallet> {
        match uri::parse_wallet_code(network, code)? {
            WalletCode::Words { words, height } => Wallet::restore_words(path, file_key, network, &words, height.or(restore_height)),
            WalletCode::SpendKey { address, spend, view, height } => {
                let keys = Keys::from_spend_key(network, &spend, view.as_deref().map(|v| v.as_str()))?;
                if address.is_some_and(|a| a != keys.primary_address()) {
                    return Err(FobError::BadKey);
                }
                Wallet::new(path, file_key, keys, Some(height.or(restore_height).unwrap_or(0)))
            }
            WalletCode::ViewOnly { address, view, height } => {
                Wallet::restore_view_only(path, file_key, network, &address, &view, height.or(restore_height))
            }
        }
    }

    pub fn restore_view_only(
        path: &Path,
        file_key: [u8; 32],
        network: Network,
        address: &str,
        view_hex: &str,
        restore_height: Option<u64>,
    ) -> Result<Wallet> {
        Wallet::new(path, file_key, Keys::view_only(network, address, view_hex)?, Some(restore_height.unwrap_or(0)))
    }

    /// The wallet saved at `path`. A photo saved before Fob read photos'
    /// headers that it wouldn't take now is dropped (the contact or profile
    /// stays): it could claim a picture too big to draw.
    pub fn open(path: &Path, file_key: [u8; 32]) -> Result<Wallet> {
        let mut file: FileIn = store::load(path, &file_key)?;
        let dropped = file.state.contacts.drop_bad_photos() | file.state.profile.drop_bad_photo();
        let wallet = Wallet::from_parts(path, file_key, file.keys.into_keys()?, file.state);
        if dropped {
            wallet.save_state(&wallet.inner.state.try_lock().expect("an opened wallet is not shared yet"))?;
        }
        Ok(wallet)
    }

    fn save_state(&self, state: &State) -> Result<()> {
        store::save(&self.inner.path, &self.inner.file_key, &FileOut { keys: StoredKeys::from(&self.inner.keys), state })
    }

    pub fn network(&self) -> Network {
        self.inner.keys.network
    }

    pub fn can_spend(&self) -> bool {
        self.inner.keys.can_spend()
    }

    pub fn seed_words(&self) -> Option<(SeedKind, Zeroizing<String>)> {
        self.inner.keys.seed_words()
    }

    /// Home's reminder while a wallet made here hasn't had its seed phrase
    /// written down, with the warning over the words meanwhile; `None` once
    /// it has (or for a restored or view-only wallet).
    pub async fn backup_reminder(&self) -> Option<keys::Reminder> {
        let pending = self.inner.state.lock().await.backup_pending;
        self.inner.keys.seed_word_count().filter(|_| pending).map(keys::reminder)
    }

    /// "I've written them down": no more reminders.
    pub async fn backup_done(&self) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        if !state.backup_pending {
            return Ok(());
        }
        state.backup_pending = false;
        if let Err(e) = self.save_state(&state) {
            state.backup_pending = true;
            return Err(e);
        }
        Ok(())
    }

    /// Where a restore elsewhere should start: the wallet code's and Keys'
    /// restore height.
    async fn restore_from(&self) -> u64 {
        let state = self.inner.state.lock().await;
        state.restore_height.unwrap_or(state.next_block)
    }

    /// A wallet code for restoring this wallet elsewhere.
    pub async fn wallet_code(&self) -> Zeroizing<String> {
        let keys = &self.inner.keys;
        let height = self.restore_from().await;
        uri::wallet_code(&keys.primary_address(), keys.spend_key_hex().as_deref().map(|s| s.as_str()), &keys.view_key_hex(), height)
    }

    /// Backup and keys' words, and its line in Settings: the seed phrase's
    /// word count and whether it was written down (said only of a wallet
    /// made here); `phone_lock`: the phone has a screen lock set, which
    /// opening it asks for.
    pub async fn backup_hub(&self, phone_lock: bool) -> keys::Hub {
        let state = self.inner.state.lock().await;
        let backed = match () {
            _ if state.restored => keys::Backed::Restored,
            _ if state.backup_pending => keys::Backed::Pending,
            _ => keys::Backed::Up,
        };
        keys::hub(self.inner.keys.seed_word_count(), backed, phone_lock)
    }

    /// Keys' page: every key in full, the restore height with its date
    /// when it can be named.
    pub async fn keys_page(&self) -> Vec<keys::KeyGroup> {
        let height = self.restore_from().await;
        self.inner.keys.page(height, restore_date(self.network(), height))
    }

    pub fn primary_address(&self) -> String {
        self.inner.keys.primary_address()
    }

    /// The next subaddress no one has had: past every one handed out to
    /// Receive or a request, and never the address splits pay to. In a
    /// restored wallet, also [`RESTORE_SKIP`] past the highest the other
    /// phone was paid at, as found so far: the addresses it handed out
    /// after that aren't known here, and aren't given out again.
    fn hand_out(state: &mut State) -> u32 {
        // Receive keeps the one it shows (a file from before requests
        // showed the last one handed out).
        state.receive_own.get_or_insert(state.receive_index);
        if state.restored {
            let theirs = state.outputs.iter().map(|o| o.subaddress).filter(|s| state.fresh_from.is_none_or(|f| *s < f) && Some(*s) != state.split_index);
            if let Some(highest) = theirs.max().filter(|h| *h > 0) {
                state.receive_index = state.receive_index.max(highest + RESTORE_SKIP);
            } else if state.fresh_from.is_none() {
                state.receive_index = state.receive_index.max(RESTORE_SKIP);
            }
        }
        state.receive_index += 1;
        if Some(state.receive_index) == state.split_index {
            state.receive_index += 1;
        }
        if state.restored {
            state.fresh_from.get_or_insert(state.receive_index);
        }
        state.receive_index
    }

    /// The highest subaddress scanning watches: [`LOOKAHEAD`] past the
    /// highest handed out or paid.
    fn watched(receive_index: u32) -> u32 {
        receive_index.saturating_add(LOOKAHEAD)
    }

    /// Has `scanner` watch subaddresses `from..=to` too.
    fn watch(scanner: &mut Scanner, from: u32, to: u32) {
        for i in from..=to {
            if let Some(sub) = SubaddressIndex::new(0, i) {
                scanner.register_subaddress(sub);
            }
        }
    }

    /// Receive's address, by index: the last one it showed.
    fn own_index(state: &State) -> u32 {
        state.receive_own.unwrap_or(state.receive_index)
    }

    /// Whether Receive's address must be a new one before it's shown: none
    /// yet (in a restored wallet, the highest the scan found is the other
    /// phone's), or the last went to a request, a split or a share.
    fn must_hand_out(state: &State) -> bool {
        let own = Self::own_index(state);
        own == 0 || state.restored && state.receive_own.is_none() || Some(own) == state.split_index || state.requests.at(own).is_some() || state.profile.at(own).is_some()
    }

    /// Receive's address and its index as it would open now, handing
    /// nothing out and saving nothing: `None` while
    /// [`receive_address`](Wallet::receive_address) would hand out a new
    /// one. A screen reads this before Receive opens; in a restored wallet,
    /// handing one out before the scan finds the other phone's payments
    /// would give out addresses it already did.
    pub async fn shown_receive_address(&self) -> Option<(String, u32)> {
        let state = self.inner.state.lock().await;
        (!Self::must_hand_out(&state)).then(|| {
            let own = Self::own_index(&state);
            (self.inner.keys.address(own), own)
        })
    }

    /// The address to give out now, as Receive opens: the last one it
    /// showed, or a first one; never one a request or splits have.
    pub async fn receive_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        if Self::must_hand_out(&state) {
            let index = Self::hand_out(&mut state);
            state.receive_own = Some(index);
            self.save_state(&state)?;
        }
        Ok(self.inner.keys.address(Self::own_index(&state)))
    }

    /// A fresh address nobody has seen.
    pub async fn new_receive_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        let index = Self::hand_out(&mut state);
        state.receive_own = Some(index);
        self.save_state(&state)?;
        Ok(self.inner.keys.address(index))
    }

    /// Keeps the note put in a payment code for the address at `index`, so
    /// payments to it say what they were for; a blank one forgets it.
    pub async fn remember_receive_note(&self, index: u32, note: &str) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let note = note.trim();
        let changed = if note.is_empty() { state.notes.remove(&index).is_some() } else { state.notes.insert(index, note.to_owned()).as_deref() != Some(note) };
        if changed { self.save_state(&state) } else { Ok(()) }
    }

    /// Receive's address, by index.
    pub async fn receive_index(&self) -> u32 {
        Self::own_index(&*self.inner.state.lock().await)
    }

    pub async fn node(&self) -> node::Setting {
        let state = self.inner.state.lock().await;
        node::Setting { url: state.node.clone(), self_signed: state.node_self_signed }
    }

    /// Settings' Node line: the last check of it ("Working · node.example:38081")
    /// when it's newer than syncing's last failure, else "Can't reach node
    /// · …" while syncing fails (as Home's chip), else what syncing says,
    /// else just the address. A failure saving the wallet isn't the node's.
    pub async fn node_line(&self) -> String {
        let setting = self.node().await;
        let address = node::address(&setting.url);
        let checked = self.inner.node_verdict.lock().expect("node lock").clone();
        let status = self.status();
        let verdict = match checked {
            // A failed round drops the check before it: what's here is newer.
            Some((checked, verdict)) if checked == setting => Some(verdict),
            _ if status.error.is_some() && !status.error_local => Some("Can't reach node"),
            _ if status.synced || status.tip > 0 => Some("Working"),
            _ => None,
        };
        verdict.map_or(address.clone(), |v| format!("{v} · {address}"))
    }

    /// Checks `setting`'s node, telling `progress` as each check finishes.
    pub async fn check_node(&self, setting: &node::Setting, progress: &(dyn Fn(&node::Report) + Sync)) -> node::Report {
        let tip = self.inner.state.lock().await.tip;
        let report = node::check(self.network(), setting, tip, now(), progress).await;
        let current = self.node().await;
        if node::normalize(&setting.url) == current.url && setting.self_signed == current.self_signed {
            *self.inner.node_verdict.lock().expect("node lock") = Some((current, report.verdict()));
        }
        report
    }

    /// The nodes to offer besides the one in use: the network's own, then
    /// `added` (ones checked on the Node screen), each once.
    pub async fn other_nodes(&self, added: &[node::Setting]) -> Vec<node::Setting> {
        let current = self.node().await.url;
        let defaults = self.network().default_nodes().iter().map(|url| node::Setting { url: (*url).to_owned(), self_signed: false });
        let mut out: Vec<node::Setting> = vec![];
        for n in defaults.chain(added.iter().cloned()) {
            let url = node::normalize(&n.url);
            if url != current && !out.iter().any(|o| o.url == url) {
                out.push(node::Setting { url, ..n });
            }
        }
        out
    }

    /// Checks the other nodes at once, each with its line for the list.
    pub async fn check_nodes(&self, nodes: &[node::Setting]) -> Vec<(node::Report, String)> {
        let tip = self.inner.state.lock().await.tip;
        node::check_all(self.network(), nodes, tip, now()).await
    }

    /// Talks to another node from now on; `self_signed` accepts its
    /// certificate unverified.
    pub async fn set_node(&self, url: &str, self_signed: bool) -> Result<()> {
        let url = crate::node::normalize(url);
        // An onion node can't be reached without Tor, and isn't looked up.
        crate::node::reachable(&url).map_err(FobError::BadEndpoint)?;
        // A node on the other network would never find this wallet's money.
        let setting = node::Setting { url: url.clone(), self_signed };
        crate::node::on_network(self.network(), &setting).await.map_err(FobError::Node)?;
        let zmq_dropped = {
            let mut state = self.inner.state.lock().await;
            // Instant updates belong to a node's host: on another host they
            // go off, and the new host's suggestion is offered instead.
            let dropped = state.zmq_endpoint.as_ref().is_some_and(|e| zmq::endpoint(&url, e).is_err());
            if dropped {
                state.zmq_endpoint = None;
                state.zmq_on = false;
            }
            state.node = url;
            state.node_self_signed = self_signed;
            self.save_state(&state)?;
            dropped
        };
        if zmq_dropped {
            self.restart_zmq().await;
        }
        *self.inner.rpc.lock().await = None;
        *self.inner.link.lock().expect("link lock") = None;
        *self.inner.pipeline.lock().await = None;
        *self.inner.node_verdict.lock().expect("node lock") = None;
        // The old node's failure isn't this one's: the chip and Node line
        // stop saying it now, and the sync loop tries the new node at once
        // rather than at the end of its backoff.
        let status = self.set_status(|s| {
            s.error = None;
            s.error_local = false;
        });
        if let Some(listener) = self.inner.listener.lock().expect("listener lock").clone() {
            listener.on_status(status);
        }
        self.inner.wake.notify_one();
        Ok(())
    }

    /// The node's ZMQ publisher: the one set, else the usual port on the
    /// node's host, offered for Settings to show and edit.
    pub async fn zmq_endpoint(&self) -> String {
        let state = self.inner.state.lock().await;
        state.zmq_endpoint.clone().or_else(|| zmq::suggest(self.network(), &state.node)).unwrap_or_default()
    }

    /// Turns instant updates on or off. `endpoint` (checked: TCP, on the
    /// node's own host) replaces the one set; without it, the one set or
    /// the suggestion is used. Takes effect at once while syncing.
    pub async fn set_zmq(&self, on: bool, endpoint: Option<&str>) -> Result<()> {
        {
            let mut state = self.inner.state.lock().await;
            let typed = match endpoint {
                Some(typed) => Some(typed.to_owned()),
                None if on => state.zmq_endpoint.clone().or_else(|| zmq::suggest(self.network(), &state.node)),
                // Off, nothing typed: the one set stays for next time.
                None => None,
            };
            if let Some(typed) = typed {
                state.zmq_endpoint = Some(zmq::endpoint(&state.node, &typed).map_err(FobError::BadEndpoint)?);
            } else if on {
                return Err(FobError::BadEndpoint("set a node first".into()));
            }
            state.zmq_on = on;
            self.save_state(&state)?;
        }
        self.restart_zmq().await;
        Ok(())
    }

    /// Starts the ZMQ subscriber afresh from the settings, if the sync loop
    /// runs and instant updates are on; otherwise just stops it.
    async fn restart_zmq(&self) {
        if let Some((_, task)) = self.inner.zmq_task.lock().expect("zmq lock").take() {
            task.abort();
        }
        let (on, endpoint) = {
            let state = self.inner.state.lock().await;
            (state.zmq_on, state.zmq_endpoint.clone())
        };
        self.set_zmq_status(if on { ZmqStatus::Connecting } else { ZmqStatus::Off }, endpoint.as_deref());
        let listener = self.inner.listener.lock().expect("listener lock").clone();
        let syncing = self.inner.syncing.lock().expect("sync lock").as_ref().is_some_and(|h| !h.is_finished());
        if let (true, Some(endpoint), Some(listener), true) = (on, endpoint, listener, syncing) {
            let events = ZmqEvents { wallet: self.clone(), endpoint: endpoint.clone(), listener };
            let task = crate::runtime().spawn(zmq::listen(endpoint.clone(), events));
            if let Some((_, old)) = self.inner.zmq_task.lock().expect("zmq lock").replace((endpoint, task)) {
                old.abort();
            }
            // Stopped meanwhile: stop this one too.
            if self.inner.syncing.lock().expect("sync lock").is_none() {
                self.stop_sync();
            }
        }
    }

    fn set_zmq_status(&self, zmq: ZmqStatus, endpoint: Option<&str>) -> SyncStatus {
        self.set_status(|s| {
            s.zmq_text = zmq.text(endpoint);
            s.zmq = zmq;
        })
    }

    async fn rpc(&self) -> Result<Daemon> {
        Ok(self.connection().await?.1)
    }

    /// The node: as a raw transport (for [`fetch`](crate::fetch)), as a
    /// daemon, and as a source of decoys with the output distribution cached.
    async fn connection(&self) -> Result<Connection> {
        let node = self.node().await;
        let mut rpc = self.inner.rpc.lock().await;
        // Kept for the node and its mark together: a mark taken off while a
        // connection was being made never leaves one accepting any certificate.
        if let Some((setting, connection)) = rpc.as_ref()
            && *setting == node
        {
            return Ok(connection.clone());
        }
        let err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let transport = Transport::new(&node.url, node.self_signed).map_err(err)?;
        *self.inner.link.lock().expect("link lock") = Some((node.clone(), transport.clone()));
        let daemon = monero_daemon_rpc::MoneroDaemon::new(transport.clone()).await.map_err(err)?;
        let decoys = Decoys::new(daemon.clone(), transport.clone(), self.inner.path.with_extension("decoys"));
        let adaptive = Arc::new(Adaptive::new(Tuning::DEFAULT));
        *rpc = Some((node, (transport.clone(), daemon.clone(), decoys.clone(), adaptive.clone())));
        Ok((transport, daemon, decoys, adaptive))
    }

    pub async fn balance(&self) -> Balance {
        let state = self.inner.state.lock().await;
        let mut balance = Balance::default();
        for output in state.outputs.iter().filter(|o| o.spent_in.is_none()) {
            balance.total += output.amount;
            if confirmations(&state, output.height) >= output.needed() {
                balance.unlocked += output.amount;
            }
        }
        for sent in state.sent.iter().filter(|s| s.height.is_none()) {
            balance.total += sent.change;
        }
        for pooled in self.inner.pool.lock().expect("pool lock").values() {
            balance.total += pooled.amount();
        }
        balance
    }

    /// Each coin the wallet owns, the same money [`balance`](Wallet::balance)
    /// counts: those ready to spend (its unlocked outputs), then those not
    /// ready yet (outputs short of their confirmations, change from payments
    /// not yet mined, and payments to this wallet still in the pool, one
    /// piece per transaction there), and the last block's time.
    async fn coin_pieces(&self) -> (Vec<Piece>, Vec<Piece>, u64) {
        let state = self.inner.state.lock().await;
        // Where a payment's own outputs came from: a split, or change named
        // as Activity titles the payment.
        let source = |sent: &Sent| match sent.receipt.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok()) {
            _ if sent.pieces.is_some() => Source::Split,
            Some(receipt) => Source::Change(Some(ReceiptView::from(&receipt).merchant)),
            None => Source::Change(Some(short_address(&sent.to))),
        };
        let (mut ready, mut waiting) = (vec![], vec![]);
        for output in state.outputs.iter().filter(|o| o.spent_in.is_none()) {
            let needed = output.needed();
            let confirmations = confirmations(&state, output.height);
            let source = match state.sent.iter().find(|s| s.tx == output.tx) {
                Some(sent) => source(sent),
                None if state.spends.iter().any(|s| s.tx == output.tx) => Source::Change(None),
                None => Source::Received,
            };
            let piece = Piece { amount: output.amount, confirmations, needed, source, time: output.time };
            if confirmations >= needed { ready.push(piece) } else { waiting.push(piece) }
        }
        for sent in state.sent.iter().filter(|s| s.height.is_none() && s.change > 0) {
            let back = if sent.back.is_empty() { std::slice::from_ref(&sent.change) } else { sent.back.as_slice() };
            for &amount in back {
                waiting.push(Piece { amount, confirmations: 0, needed: UNLOCK, source: source(sent), time: sent.time });
            }
        }
        for pooled in self.inner.pool.lock().expect("pool lock").values() {
            waiting.push(Piece { amount: pooled.amount(), confirmations: 0, needed: UNLOCK, source: Source::Received, time: pooled.seen });
        }
        (ready, waiting, state.last_block_time)
    }

    /// Every coin, ready or not, as the Coins screen shows it.
    pub async fn coins(&self, clock: Clock) -> Coins {
        let (ready, waiting, last_block_time) = self.coin_pieces().await;
        let price = self.price_now().await;
        let age = self.price_age(clock.now).await;
        let mut coins = not_ready::coins(ready, waiting, last_block_time, UNLOCK, price.as_ref().map(|(c, p)| (c, *p)), age.as_deref(), clock);
        // A view-only wallet can't split, whatever it holds.
        coins.can_split &= self.can_spend();
        coins
    }

    /// The money not ready yet as Home shows it, beside what's ready to
    /// spend, or `None` when all of it is.
    pub async fn not_ready(&self, clock: Clock) -> Option<NotReady> {
        // Both halves from one look at the wallet, so they always add up.
        let (ready, pieces, last_block_time) = self.coin_pieces().await;
        let ready = ready.iter().map(|p| p.amount).sum();
        let mut summary = not_ready::summarise(pieces, ready, last_block_time, clock)?;
        if let Some((currency, p)) = self.price_now().await {
            for sum in &mut summary.sums {
                sum.fiat = Some(price::value(sum.atomic, p, &currency));
            }
            summary.price_age = self.price_age(clock.now).await;
            if let Some(age) = &summary.price_age {
                summary.all_ready_line = format!("{} · price {age}", summary.all_ready_line);
            }
        }
        Some(summary)
    }

    /// Every payment in and out, newest first, unconfirmed ones on top.
    pub async fn activity(&self) -> Vec<Activity> {
        let state = self.inner.state.lock().await;
        let mut rows = vec![];
        let mut outgoing: HashSet<&str> = HashSet::new();
        for sent in &state.sent {
            outgoing.insert(&sent.tx);
            let receipt = sent.receipt.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok());
            rows.push(Activity {
                tx: sent.tx.clone(),
                direction: Direction::Out,
                // A split's pieces are still this wallet's: only the fee left.
                amount: if sent.pieces.is_some() { sent.fee } else { sent.amount },
                fee: Some(sent.fee),
                to: Some(sent.to.clone()),
                height: sent.height,
                time: sent.time,
                confirmations: sent.height.map(|h| confirmations(&state, h)).unwrap_or(0),
                needed: UNLOCK,
                receipt: receipt.as_ref().map(ReceiptView::from),
                receipt_json: sent.receipt.clone(),
                price: None,
                subaddress: None,
                pieces: vec![],
                note: None,
                split: None,
                contact: None,
                contact_id: None,
                shared: None,
                parts: vec![],
            });
        }
        // Spends found on the chain that this wallet didn't send (a restored
        // wallet): what left is the spent inputs less change and fee.
        for spend in &state.spends {
            if !outgoing.insert(&spend.tx) {
                continue;
            }
            let spent: u64 = state.outputs.iter().filter(|o| o.spent_in.as_deref() == Some(&spend.tx)).map(|o| o.amount).sum();
            let change: u64 = state.outputs.iter().filter(|o| o.tx == spend.tx).map(|o| o.amount).sum();
            rows.push(Activity {
                tx: spend.tx.clone(),
                direction: Direction::Out,
                amount: spent.saturating_sub(change).saturating_sub(spend.fee),
                fee: Some(spend.fee),
                to: None,
                height: Some(spend.height),
                time: spend.time,
                confirmations: confirmations(&state, spend.height),
                needed: UNLOCK,
                receipt: None,
                receipt_json: None,
                price: None,
                subaddress: None,
                pieces: vec![],
                note: None,
                split: None,
                contact: None,
                contact_id: None,
                shared: None,
                parts: vec![],
            });
        }
        let mut incoming: HashMap<&str, Activity> = HashMap::new();
        for output in state.outputs.iter().filter(|o| !outgoing.contains(o.tx.as_str())) {
            let row = incoming.entry(&output.tx).or_insert_with(|| Activity {
                tx: output.tx.clone(),
                direction: Direction::In,
                amount: 0,
                fee: None,
                to: None,
                height: Some(output.height),
                time: output.time,
                confirmations: confirmations(&state, output.height),
                needed: UNLOCK,
                receipt: None,
                receipt_json: None,
                price: None,
                subaddress: None,
                pieces: vec![],
                note: None,
                split: None,
                contact: None,
                contact_id: None,
                shared: None,
                parts: vec![],
            });
            row.amount += output.amount;
            add_part(&mut row.parts, output.subaddress, output.amount);
        }
        rows.extend(incoming.into_values());
        for (tx, pooled) in self.inner.pool.lock().expect("pool lock").iter() {
            if !rows.iter().any(|r| r.tx == *tx) {
                rows.push(Activity {
                    tx: tx.clone(),
                    direction: Direction::In,
                    amount: pooled.amount(),
                    fee: None,
                    to: None,
                    height: None,
                    time: pooled.seen,
                    confirmations: 0,
                    needed: UNLOCK,
                    receipt: None,
                    receipt_json: None,
                    price: None,
                    subaddress: pooled.parts.first().map(|p| p.0),
                    pieces: vec![],
                    note: None,
                    split: None,
                    contact: None,
                    contact_id: None,
                    shared: None,
                    parts: pooled.parts.clone(),
                });
            }
        }
        for row in &mut rows {
            if let Some(code) = state.prices.code() {
                row.price = state.price_records.get(&row.tx).and_then(|r| r.iter().find(|r| r.currency == code)).cloned();
            }
            match row.direction {
                Direction::In => {
                    let outputs: Vec<&Owned> = state.outputs.iter().filter(|o| o.tx == row.tx).collect();
                    row.pieces = outputs.iter().map(|o| o.amount).collect();
                    row.needed = outputs.iter().map(|o| o.needed()).max().unwrap_or(UNLOCK);
                    row.subaddress = row.subaddress.or(outputs.first().map(|o| o.subaddress));
                    // A request's address: its note, and who it was for.
                    match row.subaddress.and_then(|s| state.requests.at(s)) {
                        Some(request) => {
                            row.note = request.note.clone();
                            row.contact_id = state.requests.payer(request.subaddress, &row.tx, row.time);
                            row.contact = row.contact_id.and_then(|id| state.contacts.get(id)).map(|c| c.name.clone());
                            row.contact_id = row.contact.as_ref().and(row.contact_id);
                        }
                        None => row.note = row.subaddress.and_then(|s| state.notes.get(&s)).cloned(),
                    }
                    // A share's address: "Via your profile".
                    row.shared = row.subaddress.and_then(|s| state.profile.at(s)).map(|s| s.created);
                }
                Direction::Out => {
                    row.pieces = state.outputs.iter().filter(|o| o.spent_in.as_deref() == Some(row.tx.as_str())).map(|o| o.amount).collect();
                    let sent = state.sent.iter().find(|s| s.tx == row.tx);
                    row.note = sent.and_then(|s| s.note.clone());
                    row.split = sent.and_then(|s| s.pieces);
                    if let Some(owner) = row.to.as_deref().and_then(|to| state.contacts.owner(to)).filter(|_| row.split.is_none()) {
                        row.contact = Some(owner.name.clone());
                        row.contact_id = Some(owner.id);
                    }
                }
            }
        }
        rows.sort_by_key(|r| std::cmp::Reverse((r.height.is_none(), r.height, r.time)));
        rows
    }

    pub fn status(&self) -> SyncStatus {
        self.inner.status.lock().expect("status lock").clone()
    }

    fn set_status(&self, f: impl FnOnce(&mut SyncStatus)) -> SyncStatus {
        let mut status = self.inner.status.lock().expect("status lock");
        f(&mut status);
        status.clone()
    }

    /// Scans the next batch of blocks. Returns whether more are waiting.
    pub async fn sync_step(&self) -> Result<bool> {
        let (transport, rpc, decoys, adaptive) = self.connection().await?;
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));

        let (mut start, mut tip, recent) = {
            let state = self.inner.state.lock().await;
            (state.next_block, state.tip, state.recent.last().cloned())
        };
        // Ask for the tip only near the end of what's known: while catching
        // up, every request goes to fetching blocks.
        if tip == 0 || start + BATCH > tip {
            tip = rpc.latest_block_number().await.map_err(node_err)? as u64 + 1;
            let mut state = self.inner.state.lock().await;
            state.tip = tip;
            if state.restore_height.is_none() {
                // A new wallet can't have money in blocks before it existed.
                let first = tip.saturating_sub(UNLOCK);
                state.restore_height = Some(first);
                state.next_block = first;
            }
            start = state.next_block;
            self.save_state(&state)?;
        }
        self.refresh_status().await;
        if start >= tip {
            // Caught up: show payments on their way before they're mined. The
            // pool is a nicety: a node that won't share it doesn't stop syncing.
            let _ = self.scan_pool(&rpc).await;
            // And keep the decoy data current, so a payment is quick to prepare.
            if self.can_spend() {
                let _ = decoys.update(tip as usize - 1).await;
            }
            return Ok(false);
        }

        let blocks = if start == 0 {
            // get_blocks.bin can't start at the genesis block; fetch it alone.
            rpc.contiguous_scannable_blocks(0..=0).await.map_err(node_err)?
        } else {
            // Keep fetching ahead while this step scans: reuse the pipeline
            // if it's where the wallet is, else start one to the tip.
            let mut pipeline = self.inner.pipeline.lock().await;
            if pipeline.as_ref().is_none_or(|p| p.next != start || p.next > p.end) {
                *pipeline = Some(Pipeline::start(transport.clone(), adaptive.clone(), start, tip - 1));
            }
            let running = pipeline.as_mut().expect("just started");
            match running.next_batch(BATCH as usize).await {
                Ok(blocks) if !blocks.is_empty() => blocks,
                Ok(_) => {
                    // Ran out before the tip moved on: start again next step.
                    *pipeline = None;
                    return Ok(true);
                }
                Err(e) => {
                    *pipeline = None;
                    return Err(node_err(e));
                }
            }
        };
        // A reorganisation shows as a first block that doesn't follow the last
        // one scanned: forget the recent blocks and scan them again.
        if let (Some((_, hash)), Some(first)) = (&recent, blocks.first())
            && hex::encode(first.block.header.previous) != *hash
        {
            self.rewind().await?;
            return Ok(true);
        }

        let mut state = self.inner.state.lock().await;
        if state.next_block != start {
            // Rewound or reset while fetching: this batch is stale.
            return Ok(true);
        }
        let keys = &self.inner.keys;
        let mut scanner = Scanner::new(keys.view_pair());
        let mut watching = Self::watched(state.receive_index);
        Self::watch(&mut scanner, 1, watching);
        for scannable in blocks {
            // A payment near the edge moves it on, from the next block.
            if Self::watched(state.receive_index) > watching {
                Self::watch(&mut scanner, watching + 1, Self::watched(state.receive_index));
                watching = Self::watched(state.receive_index);
            }
            let height = scannable.block.number() as u64;
            let time = scannable.block.header.timestamp;
            let block_hash = hex::encode(scannable.block.hash());
            let miner_tx = hex::encode(scannable.block.miner_transaction().hash());

            for (hash, tx) in scannable.block.transactions.iter().zip(&scannable.transactions) {
                let hash = hex::encode(hash);
                if let Some(sent) = state.sent.iter_mut().find(|s| s.tx == hash) {
                    sent.height = Some(height);
                }
                let fee = match tx {
                    Transaction::V2 { proofs: Some(proofs), .. } => proofs.base.fee,
                    _ => 0,
                };
                for input in &tx.prefix().inputs {
                    let Input::ToKey { key_image, .. } = input else { continue };
                    let image = hex::encode(key_image.to_bytes());
                    if let Some(output) = state.outputs.iter_mut().find(|o| o.key_image.as_deref() == Some(image.as_str())) {
                        output.spent_in = Some(hash.clone());
                        if !state.spends.iter().any(|s| s.tx == hash) {
                            state.spends.push(ChainSpend { tx: hash.clone(), height, time, fee });
                        }
                    }
                }
            }

            *self.inner.last_block.lock().expect("block lock") = Some(scannable.block.clone());
            let found = scanner.scan(scannable).map_err(|e| FobError::Node(format!("scanning block {height}: {e:?}")))?;
            for output in found.ignore_additional_timelock() {
                // Locked to a block (a mined reward, rarely anything else):
                // kept, and not spent before then. Locked to a time: not
                // kept, as before, since a phone's clock can't say when.
                let unlock_block = match output.additional_timelock() {
                    Timelock::None => 0,
                    Timelock::Block(block) => block as u64,
                    Timelock::Time(_) => continue,
                };
                let tx = hex::encode(output.transaction());
                let key = hex::encode(output.key().compress().to_bytes());
                // Seen before: the same output again, or the burning bug (a
                // second output with a key already received, which can't
                // both be spent). Only the first counts.
                if state.outputs.iter().any(|o| o.key == key || (o.tx == tx && o.output == hex::encode(output.serialize()))) {
                    continue;
                }
                self.inner.pool.lock().expect("pool lock").remove(&tx);
                let subaddress = output.subaddress().map(|s| s.address()).unwrap_or(0);
                // Money to the split address doesn't move Receive on to it.
                if Some(subaddress) != state.split_index {
                    state.receive_index = state.receive_index.max(subaddress);
                }
                let key_image = keys.key_image(output.key(), output.key_offset()).map(hex::encode);
                // An output arriving in a transaction already seen spending (our
                // own change) is still ours to spend later.
                state.outputs.push(Owned {
                    output: hex::encode(output.serialize()),
                    coinbase: tx == miner_tx,
                    unlock_block,
                    key,
                    tx,
                    height,
                    time,
                    amount: output.commitment().amount,
                    subaddress,
                    key_image,
                    spent_in: None,
                });
            }

            state.recent.push((height, block_hash));
            if state.recent.len() > KEEP_HASHES {
                state.recent.remove(0);
            }
            state.next_block = height + 1;
            state.last_block_time = time;
        }
        self.record_prices(&mut state);
        self.save_state(&state)?;
        let more = state.next_block < tip;
        drop(state);
        self.refresh_status().await;
        Ok(more)
    }

    /// Looks for payments to this wallet in the node's pool, so they show
    /// as on their way about two minutes before they're mined.
    ///
    /// monero-oxide scans blocks, so the pool's transactions are scanned in a
    /// stand-in: the last block scanned, with the pool's transactions as its
    /// own. Only their amounts are kept, in memory; the real block scans them
    /// again once mined, with their true output indexes.
    async fn scan_pool(&self, rpc: &Daemon) -> Result<()> {
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let response = rpc.rpc_call("get_transaction_pool_hashes", None, 4 * 1024 * 1024).await.map_err(node_err)?;
        let hashes: Vec<String> = serde_json::from_str::<serde_json::Value>(&response)
            .ok()
            .and_then(|v| v.get("tx_hashes").and_then(|h| serde_json::from_value(h.clone()).ok()))
            .unwrap_or_default();
        let in_pool: HashSet<String> = hashes.iter().cloned().collect();
        self.inner.pool.lock().expect("pool lock").retain(|tx, _| in_pool.contains(tx));
        self.inner.pool_seen.lock().expect("pool lock").retain(|tx| in_pool.contains(tx));

        let ours: HashSet<String> = self.inner.state.lock().await.sent.iter().map(|s| s.tx.clone()).collect();
        let new: Vec<[u8; 32]> = {
            let seen = self.inner.pool_seen.lock().expect("pool lock");
            hashes
                .iter()
                .filter(|h| !seen.contains(*h) && !ours.contains(*h))
                .filter_map(|h| hex::decode(h).ok()?.try_into().ok())
                .collect()
        };
        if new.is_empty() {
            return Ok(());
        }
        let last = self.inner.last_block.lock().expect("block lock").clone();
        let mut block = match last {
            Some(block) => block,
            // Opened already synced: nothing scanned yet this run.
            None => {
                let latest = rpc.latest_block_number().await.map_err(node_err)?;
                let block = rpc.block_by_number(latest).await.map_err(node_err)?;
                *self.inner.last_block.lock().expect("block lock") = Some(block.clone());
                block
            }
        };
        let txs = rpc.transactions(&new).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        let miner = block.miner_transaction().hash();
        block.transactions = new.clone();
        let stand_in = monero_wallet::interface::ScannableBlock {
            block,
            transactions: txs.into_iter().map(Into::into).collect(),
            output_index_for_first_ringct_output: Some(0),
        };
        let receive_index = self.inner.state.lock().await.receive_index;
        let mut scanner = Scanner::new(self.inner.keys.view_pair());
        Self::watch(&mut scanner, 1, Self::watched(receive_index));
        let found = scanner.scan(stand_in).map_err(|e| FobError::Node(format!("scanning the pool: {e:?}")))?;
        let outputs = found.ignore_additional_timelock().into_iter().filter(|o| o.transaction() != miner);
        self.pool_found(outputs.map(|o| (hex::encode(o.transaction()), o.subaddress().map(|s| s.address()), o.commitment().amount)), now());
        self.inner.pool_seen.lock().expect("pool lock").extend(new.iter().map(hex::encode));
        // A payment first seen in the pool is priced as it's seen.
        let mut state = self.inner.state.lock().await;
        if self.record_prices(&mut state) {
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// Keeps what a pool scan found, `seen` now: each output's transaction,
    /// the subaddress it paid (`None` for the main address, index 0) and
    /// its amount, each address's own part of a transaction kept apart.
    fn pool_found(&self, outputs: impl IntoIterator<Item = (String, Option<u32>, u64)>, seen: u64) {
        let mut pool = self.inner.pool.lock().expect("pool lock");
        for (tx, subaddress, amount) in outputs {
            let pooled = pool.entry(tx).or_insert(Pooled { seen, parts: vec![] });
            add_part(&mut pooled.parts, subaddress.unwrap_or(0), amount);
        }
    }

    /// Brings [`status`](Wallet::status) up to date with the wallet's state.
    async fn refresh_status(&self) {
        let state = self.inner.state.lock().await;
        let (next, tip, start, up_to) = (state.next_block, state.tip, state.restore_height.unwrap_or(state.next_block), state.last_block_time);
        drop(state);
        self.set_status(|s| {
            s.scanned = next;
            s.tip = tip;
            s.start = start;
            s.synced = tip > 0 && next >= tip;
            s.progress = if tip <= start { 1.0 } else { (next.saturating_sub(start)) as f32 / (tip - start) as f32 };
            s.up_to = up_to;
        });
    }

    /// Forgets the last blocks after a reorganisation, to scan them again.
    async fn rewind(&self) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let Some(&(from, _)) = state.recent.first() else { return Ok(()) };
        state.outputs.retain(|o| o.height < from);
        state.spends.retain(|s| s.height < from);
        let spends: HashSet<String> = state.spends.iter().map(|s| s.tx.clone()).collect();
        let pending: HashSet<String> = state.sent.iter().filter(|s| s.height.is_none() || s.height >= Some(from)).map(|s| s.tx.clone()).collect();
        for output in &mut state.outputs {
            if output.spent_in.as_ref().is_some_and(|t| !spends.contains(t) && !pending.contains(t)) {
                output.spent_in = None;
            }
        }
        for sent in &mut state.sent {
            if sent.height.is_some_and(|h| h >= from) {
                sent.height = None;
            }
        }
        state.recent.clear();
        state.next_block = from;
        *self.inner.pipeline.lock().await = None;
        self.save_state(&state)
    }

    /// Tells `listener` where syncing is, measuring this catch-up's `pace`
    /// as it goes (a failure starts it again, measuring nothing).
    async fn report(&self, listener: &dyn SyncListener, pace: &mut catch_up::Pace, failed: Option<&FobError>) {
        self.refresh_status().await;
        let (next, tip) = {
            let status = self.status();
            (status.scanned, status.tip)
        };
        let link = self.inner.link.lock().expect("link lock").clone();
        let received = link.as_ref().map_or(0, |(_, transport)| transport.received());
        let (seconds_left, speeds) = pace.report(Instant::now(), received, next, tip.saturating_sub(next), failed.is_some());
        // As the Node checks would say it, the certificate included.
        let error = failed.map(|e| match &link {
            Some((setting, transport)) => node::sync_failure(e, transport.certificate(), setting.self_signed),
            None => node::sync_failure(e, None, false),
        });
        if error.as_ref().is_some_and(|(_, local)| !local) {
            // A failing sync is newer than the last check of the node.
            *self.inner.node_verdict.lock().expect("node lock") = None;
        }
        let status = self.set_status(|s| {
            s.seconds_left = seconds_left;
            s.blocks_per_second = speeds.map(|s| s.blocks_per_second);
            s.link_mb_per_second = speeds.map(|s| s.bytes_per_second / 1e6);
            s.error_local = error.as_ref().is_some_and(|(_, local)| *local);
            s.error = error.map(|(words, _)| words);
        });
        listener.on_status(status);
    }

    /// The card Home's sync chip opens, for the status now.
    pub fn sync_details(&self, clock: Clock) -> catch_up::Details {
        catch_up::details(&self.status(), clock)
    }

    /// Keeps the wallet up to date in the background until [`stop_sync`]:
    /// catches up as fast as the node allows, then looks for new blocks
    /// every 20 s, or every 2 minutes while the node's ZMQ publisher is
    /// heard (a new block then wakes it at once). With instant updates on,
    /// the subscriber runs beside it. `listener` hears every step.
    ///
    /// [`stop_sync`]: Wallet::stop_sync
    pub fn start_sync(&self, listener: Arc<dyn SyncListener>) {
        self.stop_sync();
        *self.inner.listener.lock().expect("listener lock") = Some(listener.clone());
        let wallet = self.clone();
        let handle = crate::runtime().spawn(async move {
            let mut pace = catch_up::Pace::default();
            let mut backoff = Duration::from_secs(2);
            loop {
                match wallet.sync_step().await {
                    Ok(more) => {
                        backoff = Duration::from_secs(2);
                        wallet.report(listener.as_ref(), &mut pace, None).await;
                        if !more {
                            pace.reset();
                            wallet.inner.rounds.send_modify(|n| *n += 1);
                            let poll = if wallet.status().zmq == ZmqStatus::Listening { POLL_LISTENING } else { POLL };
                            wallet.wait(poll, listener.as_ref()).await;
                        }
                    }
                    Err(e) => {
                        wallet.report(listener.as_ref(), &mut pace, Some(&e)).await;
                        wallet.inner.rounds.send_modify(|n| *n += 1);
                        wallet.wait(backoff, listener.as_ref()).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                    }
                }
            }
        });
        *self.inner.syncing.lock().expect("sync lock") = Some(handle);
        // Beside the loop, while instant updates are on.
        let wallet = self.clone();
        crate::runtime().spawn(async move { wallet.restart_zmq().await });
    }

    pub fn stop_sync(&self) {
        if let Some(handle) = self.inner.syncing.lock().expect("sync lock").take() {
            handle.abort();
        }
        if let Some((endpoint, task)) = self.inner.zmq_task.lock().expect("zmq lock").take() {
            task.abort();
            // Still on: it connects again with the loop.
            self.set_zmq_status(ZmqStatus::Connecting, Some(&endpoint));
        }
        *self.inner.listener.lock().expect("listener lock") = None;
    }

    /// The sync loop's wait between rounds, cut short by [`sync_now`](Wallet::sync_now)
    /// or a new block announced. A transaction announced in the pool has
    /// the pool looked at (once caught up) and the wait go on.
    async fn wait(&self, time: Duration, listener: &dyn SyncListener) {
        let deadline = tokio::time::Instant::now() + time;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => return,
                _ = self.inner.wake.notified() => return,
                _ = self.inner.pool_wake.notified() => {
                    tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + POOL_GAP)).await;
                    if self.status().synced && self.status().error.is_none() && let Ok(rpc) = self.rpc().await {
                        let _ = self.scan_pool(&rpc).await;
                        self.report(listener, &mut catch_up::Pace::default(), None).await;
                    }
                }
            }
        }
    }

    /// Asks the node for new blocks and its pool now, rather than at the next
    /// poll (pull to refresh). Returns once that round is over: caught up,
    /// failed (the error is in [`status`](Wallet::status)), or 20 s passed.
    /// Without the sync loop running it does one step itself.
    pub async fn sync_now(&self) {
        let running = self.inner.syncing.lock().expect("sync lock").as_ref().is_some_and(|h| !h.is_finished());
        if !running {
            let _ = tokio::time::timeout(SYNC_NOW_WAIT, self.sync_step()).await;
            self.refresh_status().await;
            return;
        }
        let mut rounds = self.inner.rounds.subscribe();
        rounds.mark_unchanged();
        // A round already under way ends this wait; the stored permit then
        // starts the next one at once, which is what pulling asked for.
        self.inner.wake.notify_one();
        let _ = tokio::time::timeout(SYNC_NOW_WAIT, rounds.changed()).await;
    }

    /// Builds a payment of `amount` to `to`, choosing the money to spend
    /// and the fee. Nothing leaves the wallet until [`send`](Wallet::send).
    /// `note` (typed, or the payment code's description) is kept with the
    /// payment once sent; `code` is the payment code the form was filled
    /// from, if any, whose name and receipt go with it as
    /// [`uri::code_terms`] says.
    pub async fn prepare(&self, to: &str, amount: u64, note: Option<String>, code: Option<&str>) -> Result<Prepared> {
        self.inner.keys.spend()?;
        let network = self.network();
        let destination = MoneroAddress::from_str(network.monero(), to.trim()).map_err(|_| FobError::BadAddress(network.name()))?;
        if amount == 0 {
            return Err(FobError::BadAmount);
        }
        let terms = uri::code_terms(network, to, amount, code);
        let receipt_json = terms.receipt.as_ref().and_then(crate::receipt::to_json);
        self.prepare_payments(to.trim(), vec![(destination, amount)], receipt_json, note, terms.name, None).await
    }

    /// Asks the node its fee rate, keeping it for Split up's estimate.
    async fn fee_rate(&self, rpc: &Daemon) -> Result<FeeRate> {
        let fee_rate = rpc.fee_rate(FeePriority::Normal, MAX_FEE_PER_WEIGHT).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        // The mask has no getter; it's the second half of the serialization.
        let bytes = fee_rate.serialize();
        let mask = bytes.get(8..16).and_then(|b| b.try_into().ok()).map(u64::from_le_bytes).unwrap_or(1);
        *self.inner.fee_rate.lock().expect("fee lock") = Some((fee_rate.per_weight(), mask));
        Ok(fee_rate)
    }

    /// Asks the node its fee rate now, so Split up's fee estimate is the
    /// node's, not Monero's usual. Quietly keeps the last one on failure.
    pub async fn refresh_fee_rate(&self) {
        if let Ok((_, rpc, _, _)) = self.connection().await {
            let _ = self.fee_rate(&rpc).await;
        }
    }

    /// Builds a transaction paying `payments`, choosing the money to spend
    /// and the fee. `to` is who the confirm screen says is paid.
    async fn prepare_payments(
        &self,
        to: &str,
        payments: Vec<(MoneroAddress, u64)>,
        receipt_json: Option<String>,
        note: Option<String>,
        recipient_name: Option<String>,
        pieces: Option<u32>,
    ) -> Result<Prepared> {
        let amount: u64 = payments.iter().map(|p| p.1).sum();
        let (_, rpc, decoys, _) = self.connection().await?;
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let fee_rate = self.fee_rate(&rpc).await?;
        let latest = rpc.latest_block_number().await.map_err(node_err)?;

        let (mut candidates, available) = {
            let state = self.inner.state.lock().await;
            let mut candidates: Vec<(usize, u64, WalletOutput)> = state
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, o)| {
                    o.spent_in.is_none()
                        && confirmations(&state, o.height) >= o.needed()
                })
                .filter_map(|(i, o)| {
                    let bytes = hex::decode(&o.output).ok()?;
                    Some((i, o.amount, WalletOutput::read(&mut bytes.as_slice()).ok()?))
                })
                .collect();
            candidates.sort_by_key(|c| std::cmp::Reverse(c.1));
            let available = candidates.iter().map(|c| c.1).sum::<u64>();
            (candidates, available)
        };
        let not_enough = |needed: u64| FobError::NotEnoughFunds { available: amount::format(available), needed: amount::format(needed) };
        if available < amount {
            return Err(not_enough(amount));
        }

        // Start with the inputs that cover the amount and a generous fee guess,
        // then add one at a time while the real fee needs more.
        let outputs = payments.len() + 1;
        let guess = |inputs: usize| fee_rate.calculate_fee_from_weight((1_600 + 900 * inputs as u64).max(split::weight(inputs, outputs))) as u64;
        let mut chosen = vec![];
        let mut total = 0;
        while total < amount + guess(chosen.len().max(1)) && !candidates.is_empty() {
            let c = candidates.remove(0);
            total += c.1;
            chosen.push(c);
        }
        let mut with_decoys = vec![];
        for c in &chosen {
            with_decoys.push(OutputWithDecoys::new(&mut OsRng, &decoys, RING_LEN, latest, c.2.clone()).await.map_err(|e| FobError::Node(format!("{e:?}")))?);
        }
        let change = Change::new(self.inner.keys.view_pair(), None);
        let signable = loop {
            let mut outgoing_view = Zeroizing::new([0u8; 32]);
            OsRng.fill_bytes(outgoing_view.as_mut());
            match SignableTransaction::new(
                RctType::ClsagBulletproofPlus,
                outgoing_view,
                with_decoys.clone(),
                payments.clone(),
                change.clone(),
                vec![],
                fee_rate,
            ) {
                Ok(signable) => break signable,
                Err(SendError::NotEnoughFunds { necessary_fee, .. }) => {
                    if candidates.is_empty() {
                        return Err(not_enough(amount + necessary_fee.unwrap_or(0)));
                    }
                    let c = candidates.remove(0);
                    total += c.1;
                    with_decoys.push(OutputWithDecoys::new(&mut OsRng, &decoys, RING_LEN, latest, c.2.clone()).await.map_err(|e| FobError::Node(format!("{e:?}")))?);
                    chosen.push(c);
                }
                Err(e) => return Err(FobError::Send(format!("{e:?}"))),
            }
        };
        let fee = signable.necessary_fee();
        let receipt = receipt_json.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok());
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.inner.prepared.lock().await.insert(
            id,
            PreparedTx {
                signable,
                out: Outgoing {
                    to: to.to_owned(),
                    amount,
                    inputs: chosen.iter().map(|c| c.0).collect(),
                    input_total: total,
                    receipt: receipt.is_some().then_some(receipt_json).flatten(),
                    note: requests::clean_note(note.as_deref()),
                    recipient_name,
                    pieces,
                    back: if pieces.is_some() { payments.iter().map(|p| p.1).collect() } else { vec![] },
                },
            },
        );
        Ok(Prepared { id, to: to.to_owned(), amount, fee, receipt: receipt.as_ref().map(ReceiptView::from), split: vec![], pieces })
    }

    /// The notes Split up offers, in money at the current price when there
    /// is one, else in XMR; and the money the split can use.
    async fn split_setup(&self) -> (Option<(price::Currency, u64)>, Vec<split::Note>, split::Budget) {
        let money = self.price_now().await;
        let notes = split::notes(money.as_ref().map(|(c, p)| (c, *p)));
        let state = self.inner.state.lock().await;
        let outputs = state.outputs.iter().filter(|o| o.spent_in.is_none() && confirmations(&state, o.height) >= o.needed()).map(|o| o.amount).collect();
        (money, notes, split::Budget { outputs, fee_rate: *self.inner.fee_rate.lock().expect("fee lock") })
    }

    /// What was paid in the 30 days to `now`, splits aside: what Split
    /// up's Auto learns from. The amounts Activity shows going out, read
    /// straight from what was sent and spent (no receipts parsed), as Split
    /// up asks again with every tap.
    fn split_payments(state: &State, now: u64) -> Vec<u64> {
        let since = now.saturating_sub(split::LOOKBACK);
        let sent: HashSet<&str> = state.sent.iter().map(|s| s.tx.as_str()).collect();
        let mut spent: HashSet<&str> = HashSet::new();
        // A restored wallet's spends: the inputs less change and fee, as in Activity.
        let spends = state.spends.iter().filter(|s| !sent.contains(s.tx.as_str()) && spent.insert(&s.tx)).map(|spend| {
            let inputs: u64 = state.outputs.iter().filter(|o| o.spent_in.as_deref() == Some(&spend.tx)).map(|o| o.amount).sum();
            let change: u64 = state.outputs.iter().filter(|o| o.tx == spend.tx).map(|o| o.amount).sum();
            (spend.time, inputs.saturating_sub(change).saturating_sub(spend.fee))
        });
        state
            .sent
            .iter()
            .filter(|s| s.pieces.is_none())
            .map(|s| (s.time, s.amount))
            .chain(spends)
            .filter(|(time, amount)| *time >= since && *amount > 0)
            .map(|(_, amount)| amount)
            .collect()
    }

    /// Split up's screen for `counts` of each note, at `clock` (the
    /// phone's), which says how old the price is and which payments Auto
    /// learns from. The screen opens with no counts: empty.
    pub async fn split_view(&self, counts: &[u32], clock: Clock) -> split::View {
        let (money, notes, budget) = self.split_setup().await;
        let (payments, age) = {
            let state = self.inner.state.lock().await;
            (Self::split_payments(&state, clock.now), state.price_cache.shown(&state.prices, clock.now).and_then(|c| price::alert::old(c.at, clock.now)))
        };
        split::view(&notes, counts, &payments, &budget, money.as_ref().map(|(c, p)| (c, *p)), age.as_deref())
    }

    /// One more piece of note `index`, or why not (the 15-piece cap, or not
    /// enough ready to spend with the fee).
    pub async fn split_add(&self, counts: &[u32], index: usize) -> Result<Vec<u32>> {
        let (_, notes, budget) = self.split_setup().await;
        split::add(&notes, counts, index, &budget).map_err(FobError::Split)
    }

    /// One fewer piece of note `index`.
    pub async fn split_remove(&self, counts: &[u32], index: usize) -> Vec<u32> {
        let (_, notes, _) = self.split_setup().await;
        split::remove(&notes, counts, index)
    }

    /// The address splits pay to: one of this wallet's subaddresses, the
    /// next unused one the first time, kept from then on and skipped by
    /// Receive.
    async fn split_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        let index = match state.split_index {
            Some(index) => index,
            None => {
                let index = state.receive_index + 1;
                state.split_index = Some(index);
                self.save_state(&state)?;
                index
            }
        };
        Ok(self.inner.keys.address(index))
    }

    /// Builds a split: `counts` of each note paid to this wallet's split
    /// address, the rest back as change, for the confirm screen.
    pub async fn prepare_split(&self, counts: &[u32]) -> Result<Prepared> {
        self.inner.keys.spend()?;
        let (_, notes, budget) = self.split_setup().await;
        let amounts = split::amounts(&notes, counts);
        if amounts.is_empty() {
            return Err(FobError::Split("Add a coin to split".into()));
        }
        split::check(&notes, counts, &budget).map_err(FobError::Split)?;
        let address = self.split_address().await?;
        let network = self.network();
        let destination = MoneroAddress::from_str(network.monero(), &address).map_err(|_| FobError::BadAddress(network.name()))?;
        let n = amounts.len() as u32;
        let payments = amounts.into_iter().map(|a| (destination, a)).collect();
        let mut prepared = self.prepare_payments(&address, payments, None, None, None, Some(n)).await?;
        prepared.split = split::lines(&notes, counts);
        Ok(prepared)
    }

    /// Signs and broadcasts a prepared payment. Returns its transaction hash.
    pub async fn send(&self, id: u64) -> Result<String> {
        let prepared = self.inner.prepared.lock().await.take(id).ok_or(FobError::NoSuchPayment)?;
        let spend = self.inner.keys.spend()?;
        let fee = prepared.signable.necessary_fee();
        let tx = prepared.signable.sign(&mut OsRng, spend).map_err(|e| FobError::Send(format!("{e:?}")))?;
        let hash = hex::encode(tx.hash());
        let wallet = self.clone();
        let publish = async move {
            let rpc = wallet.rpc().await?;
            rpc.publish_transaction(&tx).await.map_err(|e| FobError::Node(format!("{e:?}")))
        };
        self.publish_and_record(hash, fee, prepared.out, publish).await
    }

    /// Publishes a signed payment with `publish`, then records it (its
    /// inputs spent, the payment kept), as one step that runs to the end on
    /// its own once begun: dropping this call (the app leaving Confirm while
    /// "Sending…") never leaves a payment the node may have unrecorded.
    async fn publish_and_record(&self, hash: String, fee: u64, out: Outgoing, publish: impl Future<Output = Result<()>> + Send + 'static) -> Result<String> {
        let wallet = self.clone();
        let step = tokio::spawn(async move {
            {
                // Spent already by a payment sent in the meantime?
                let state = wallet.inner.state.lock().await;
                if out.inputs.iter().any(|&i| state.outputs.get(i).is_none_or(|o| o.spent_in.is_some())) {
                    return Err(FobError::NoSuchPayment);
                }
            }
            publish.await?;
            wallet.record_sent(hash, fee, out).await
        });
        step.await.map_err(|e| FobError::Send(format!("{e}")))?
    }

    /// Keeps a payment just published: its inputs spent, and what only the
    /// sender knows.
    async fn record_sent(&self, hash: String, fee: u64, out: Outgoing) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        for &i in &out.inputs {
            if let Some(output) = state.outputs.get_mut(i) {
                output.spent_in = Some(hash.clone());
            }
        }
        // A split's pieces come back to this wallet with the change.
        let back = if out.pieces.is_some() { out.input_total } else { out.input_total.saturating_sub(out.amount) };
        let change = out.input_total.saturating_sub(out.amount).saturating_sub(fee);
        let outputs = out.back.iter().copied().chain((change > 0).then_some(change)).collect();
        state.sent.push(Sent {
            tx: hash.clone(),
            to: out.to,
            amount: out.amount,
            fee,
            change: back.saturating_sub(fee),
            time: now(),
            height: None,
            receipt: out.receipt,
            note: out.note,
            pieces: out.pieces,
            back: outputs,
            recipient_name: out.recipient_name,
        });
        self.record_prices(&mut state);
        self.save_state(&state)?;
        Ok(hash)
    }

    /// Forgets a prepared payment that won't be sent.
    pub async fn cancel(&self, id: u64) {
        self.inner.prepared.lock().await.take(id);
    }

    /// Every payment and when it moved: sent, spent on the chain, received,
    /// and on its way in the pool.
    fn payment_times(&self, state: &State) -> Vec<(String, u64)> {
        let mut times: HashMap<String, u64> = HashMap::new();
        let sent = state.sent.iter().map(|s| (&s.tx, s.time));
        let spent = state.spends.iter().map(|s| (&s.tx, s.time));
        let received = state.outputs.iter().map(|o| (&o.tx, o.time));
        for (tx, time) in sent.chain(spent).chain(received) {
            times.entry(tx.clone()).or_insert(time);
        }
        for (tx, Pooled { seen, .. }) in self.inner.pool.lock().expect("pool lock").iter() {
            times.entry(tx.clone()).or_insert(*seen);
        }
        times.into_iter().collect()
    }

    /// Records what XMR was worth with each payment that has no price yet
    /// in the currency shown: the current price if it was asked within an
    /// hour of the payment, else the day's from the history. Nothing with
    /// XMR only; a payment with nothing to go on stays without, and gets
    /// one if the history later has its day. Returns whether any was
    /// recorded.
    fn record_prices(&self, state: &mut State) -> bool {
        let Some(code) = state.prices.currency.clone() else { return false };
        let mut recorded = false;
        for (tx, time) in self.payment_times(state) {
            let records = state.price_records.get(&tx);
            if records.is_some_and(|r| r.iter().any(|r| r.currency == code)) {
                continue;
            }
            if let Some(record) = state.price_cache.value_at(&state.prices, time) {
                state.price_records.entry(tx).or_default().push(record);
                recorded = true;
            }
        }
        recorded
    }

    /// How prices are shown.
    pub async fn price_settings(&self) -> price::Settings {
        self.inner.state.lock().await.prices.clone()
    }

    /// The failure in the current run of failed price lookups, if one is
    /// going on.
    pub fn price_alert(&self) -> Option<price::alert::Alert> {
        self.inner.price_alerts.lock().expect("price lock").alert().cloned()
    }

    /// The price toast, while lookups are failing: what failed and what Fob
    /// uses instead. The app shows one whose `id` it hasn't shown yet.
    pub async fn price_toast(&self, clock: Clock) -> Option<price::alert::Toast> {
        let alert = self.price_alert()?;
        let state = self.inner.state.lock().await;
        Some(price::alert::toast(&alert, state.price_cache.last(&state.prices), clock))
    }

    /// Settings' Price from row: "Kraken · web", or while
    /// lookups fail the toast's line ("Kraken didn't answer · using the
    /// 14:02 price"), and whether they do (the row is red, with Try again).
    pub async fn price_from_line(&self, clock: Clock) -> (String, bool) {
        match self.price_toast(clock).await {
            Some(toast) => (toast.line, true),
            None => (self.price_settings().await.source_text(), false),
        }
    }

    /// Has the price task ask now, for `asked`.
    fn ask_prices(&self, asked: price::alert::Asked) {
        *self.inner.prices_asked.lock().expect("price lock") = Some(asked);
        self.inner.prices_wake.notify_one();
    }

    async fn change_prices<T>(&self, change: impl FnOnce(&mut price::Settings) -> Result<T>) -> Result<T> {
        let mut state = self.inner.state.lock().await;
        let mut settings = state.prices.clone();
        let said = change(&mut settings)?;
        if settings != state.prices {
            // Which leads on Home only changes how amounts are drawn: nothing to ask.
            let asks = (settings.code(), settings.provider, settings.transport) != (state.prices.code(), state.prices.provider, state.prices.transport);
            state.prices = settings;
            self.record_prices(&mut state);
            self.save_state(&state)?;
            if asks {
                // The run was about settings gone; the user's new ones are asked now.
                self.inner.price_alerts.lock().expect("price lock").reset();
                self.ask_prices(price::alert::Asked::User);
            }
        }
        Ok(said)
    }

    /// "Default currency" `code`, one of the 45, or XMR (`None`),
    /// when Fob never asks for a price. Moves to a provider that prices it
    /// when the chosen one doesn't, and returns the words saying so.
    pub async fn set_currency(&self, code: Option<&str>) -> Result<Option<String>> {
        self.change_prices(|s| s.choose_currency(code)).await
    }

    /// Prices from `provider`, one that prices the currency. Moves to the
    /// web when it doesn't answer over Tor, and returns the words saying so.
    pub async fn set_price_provider(&self, provider: price::Provider) -> Result<Option<String>> {
        self.change_prices(|s| s.choose_provider(provider)).await
    }

    /// How price requests travel. Tor isn't available yet, and is refused,
    /// as is a way the provider doesn't answer over.
    pub async fn set_price_transport(&self, transport: price::Transport) -> Result<()> {
        self.change_prices(|s| s.choose_transport(transport)).await
    }

    /// Price from, as it opens: every provider that prices the chosen
    /// currency asked for XMR's price now, all at once, over the chosen
    /// transport; `None` with XMR only.
    pub async fn check_prices(&self) -> Option<Vec<price::Check>> {
        let settings = self.price_settings().await;
        let currency = price::currency(settings.code()?);
        let providers = price::providers_for(&currency.code);
        Some(price::check(providers, &currency, settings.transport, settings.provider.name(), price::CHECK_TIMEOUT).await)
    }

    /// The "Default currency" list, the chosen one ticked: `phone` is
    /// the phone's own currency code, `query` what's typed in the search.
    pub async fn currency_list(&self, phone: Option<&str>, query: &str) -> price::CurrencyList {
        price::currency_list(self.price_settings().await.code(), phone, query)
    }

    /// Tapping Home's balance: the default currency first, or XMR. Refused
    /// with XMR as the default currency, where nothing swaps.
    pub async fn set_fiat_first(&self, fiat_first: bool) -> Result<()> {
        self.change_prices(|s| {
            if s.code().is_none() {
                return Err(FobError::Price("With XMR as the default currency there's nothing to swap".into()));
            }
            s.fiat_first = fiat_first;
            Ok(())
        })
        .await
    }

    /// Home's balance of `atomic` XMR at `now` (seconds since the epoch),
    /// from one look at the price, so the two always agree: what it's worth
    /// with the price's age once it's missed a refresh (as
    /// [`worth_aged`](Wallet::worth_aged)), and whether tapping it swaps XMR
    /// and the default currency (one besides XMR, with that worth) and
    /// which leads.
    pub async fn balance_worth(&self, atomic: u64, now: u64) -> (Option<(String, Option<String>)>, price::BalanceOrder) {
        let state = self.inner.state.lock().await;
        let shown = state.price_cache.shown(&state.prices, now);
        let worth = shown.map(|c| (price::value(atomic, c.price, &price::currency(&c.currency)), price::alert::old(c.at, now)));
        let order = price::balance_order(&state.prices, worth.is_some());
        (worth, order)
    }

    /// The current price shown now, when amounts are also shown in a
    /// currency and one was asked within the day: (currency, price).
    pub async fn price_now(&self) -> Option<(price::Currency, u64)> {
        let state = self.inner.state.lock().await;
        state.price_cache.shown(&state.prices, now()).map(|c| (price::currency(&c.currency), c.price))
    }

    /// When the price shown at `now` was asked: new with every new price,
    /// so a screen converting money knows to convert again.
    pub async fn price_at(&self, now: u64) -> Option<u64> {
        let state = self.inner.state.lock().await;
        state.price_cache.shown(&state.prices, now).map(|c| c.at)
    }

    /// How old the price shown at `now` is, once it's missed a refresh:
    /// "23 h old"; `None` while it's fresh, or without one.
    pub async fn price_age(&self, now: u64) -> Option<String> {
        let state = self.inner.state.lock().await;
        state.price_cache.shown(&state.prices, now).and_then(|c| price::alert::old(c.at, now))
    }

    /// [`price_age`](Wallet::price_age) now, by the phone's clock.
    pub async fn price_age_now(&self) -> Option<String> {
        self.price_age(now()).await
    }

    /// What `atomic` XMR is worth now: "$83.03"; `None` without a current price.
    pub async fn worth_now(&self, atomic: u64) -> Option<String> {
        self.price_now().await.map(|(currency, p)| price::value(atomic, p, &currency))
    }

    /// What `atomic` XMR is worth at `now` (seconds since the epoch), with
    /// the price's age once it's missed a refresh: ("$412.06", Some("2 h
    /// old")); `None` without a price from within the day.
    pub async fn worth_aged(&self, atomic: u64, now: u64) -> Option<(String, Option<String>)> {
        let state = self.inner.state.lock().await;
        let c = state.price_cache.shown(&state.prices, now)?;
        Some((price::value(atomic, c.price, &price::currency(&c.currency)), price::alert::old(c.at, now)))
    }

    /// The payment `tx` as its own screen shows it, or `None` if there's no
    /// such payment.
    pub async fn payment(&self, tx: &str, clock: Clock) -> Option<activity::Detail> {
        let row = self.activity().await.into_iter().find(|a| a.tx == tx)?;
        let settings = self.price_settings().await;
        let now = self.price_now().await;
        let mut detail = activity::detail(&row, now.as_ref().map(|(c, p)| (c, *p)), settings.code().is_some(), settings.provider.name(), clock);
        // "Now" at an old price says how old.
        if let Some(age) = self.price_age(clock.now).await {
            detail.rate_now = detail.rate_now.map(|rate| format!("{rate} · {age}"));
        }
        Some(detail)
    }

    /// An amount field for `text`, typed in money (`in_fiat`) or XMR: in
    /// money only with a currency besides XMR and a price from within the
    /// day, saying when it's from once it's aged (`clock`, the phone's).
    /// Money typed against a price 5 minutes old or more has the price
    /// task ask (at most once a minute), a failure shown as the user's; so
    /// does the field with no price at all (opened or typed in, the field
    /// then in XMR), while a currency is chosen.
    pub async fn amount_field(&self, text: &str, in_fiat: bool, clock: Clock) -> price::AmountField {
        let (wanted, shown) = {
            let state = self.inner.state.lock().await;
            (state.prices.code().map(price::currency), state.price_cache.shown(&state.prices, clock.now).cloned())
        };
        let stale = shown.as_ref().is_some_and(|c| clock.now.saturating_sub(c.at) >= price::REFRESH);
        if wanted.is_some() && (shown.is_none() || stale && in_fiat && !text.trim().is_empty()) {
            let mut last = self.inner.typed_asked.lock().expect("price lock");
            if clock.now.saturating_sub(*last) >= price::PULL_REFRESH {
                *last = clock.now;
                self.ask_prices(price::alert::Asked::Typing);
            }
        }
        Self::with_field_price(wanted, shown, clock, |price| price::amount_field(text, in_fiat, price))
    }

    /// The amount field as a form opens it, blank: in money while there's a
    /// price, XMR otherwise. It asks for nothing, so it can be ready before
    /// the form is drawn; the form's own [`amount_field`](Self::amount_field)
    /// then asks, as opening it does.
    pub async fn blank_amount_field(&self, clock: Clock) -> price::AmountField {
        let (wanted, shown) = {
            let state = self.inner.state.lock().await;
            (state.prices.code().map(price::currency), state.price_cache.shown(&state.prices, clock.now).cloned())
        };
        Self::with_field_price(wanted, shown, clock, price::blank_field)
    }

    /// `make` given the field's price: the currency `wanted` at the price
    /// `shown`, with its age by `clock` once it's aged.
    fn with_field_price<T>(
        wanted: Option<price::Currency>,
        shown: Option<price::Current>,
        clock: Clock,
        make: impl FnOnce(price::FieldPrice) -> T,
    ) -> T {
        let age = shown.as_ref().and_then(|c| price::alert::age(c.at, clock));
        let price = match (&wanted, &shown) {
            (Some(c), Some(now)) => price::FieldPrice::At(c, now.price, age.as_deref()),
            (Some(c), None) => price::FieldPrice::Missing(c),
            (None, _) => price::FieldPrice::None,
        };
        make(price)
    }

    /// An amount field's text as typed, the other way: money to XMR (`to_fiat`
    /// false) or XMR to money, for switching the field. Blank, or nothing to
    /// convert with, gives blank.
    pub async fn convert_amount(&self, text: &str, to_fiat: bool) -> String {
        let Some((currency, p)) = self.price_now().await else { return String::new() };
        if to_fiat {
            amount::parse(text).map(|a| price::plain(price::to_minor(a, p, &currency), &currency)).unwrap_or_default()
        } else {
            price::parse(text, &currency).ok().and_then(|m| price::to_atomic(m, p, &currency)).map(amount::format).unwrap_or_default()
        }
    }

    /// An amount field's text as XMR (atomic units): typed in XMR, or in
    /// money (`in_fiat`) converted at the current price, with the money as
    /// written ("$20.00"). Throws when it isn't an amount, or there's no
    /// price to convert with.
    pub async fn typed_amount(&self, text: &str, in_fiat: bool) -> Result<(u64, Option<String>)> {
        let (atomic, fiat) = self.typed(text, in_fiat).await?;
        Ok((atomic, fiat.map(|(text, _)| text)))
    }

    /// As [`typed_amount`](Wallet::typed_amount), the money with its
    /// currency's code.
    async fn typed(&self, text: &str, in_fiat: bool) -> Result<(u64, Option<(String, String)>)> {
        let text = text.trim();
        if !in_fiat {
            return Ok((amount::parse(text)?, None));
        }
        let (currency, p) = self.price_now().await.ok_or_else(|| FobError::Price("no price to convert with".into()))?;
        let minor = price::parse(text, &currency)?;
        Ok((price::to_atomic(minor, p, &currency).ok_or(FobError::BadAmount)?, Some((price::format(minor, &currency), currency.code))))
    }

    /// Builds a payment of an amount as Send's field took it: typed in
    /// XMR, or in money (`in_fiat`) converted at the current price, the
    /// conversion the field's line showed. Otherwise as
    /// [`prepare`](Wallet::prepare).
    pub async fn prepare_typed(&self, to: &str, amount_text: &str, in_fiat: bool, note: Option<String>, code: Option<&str>) -> Result<Prepared> {
        let (atomic, _) = self.typed_amount(amount_text, in_fiat).await?;
        self.prepare(to, atomic, note, code).await
    }

    /// Under Send's sheet at `now`: "1.8592 XMR ready to spend · $308.70",
    /// the money only with a price from within the day, and its age once
    /// it's missed a refresh ("… · $308.70 · 2 h old").
    pub async fn ready_line(&self, now: u64) -> String {
        let ready = self.balance().await.unlocked;
        let xmr = format!("{} XMR ready to spend", amount::format_short(ready));
        match self.worth_aged(ready, now).await {
            Some((worth, Some(old))) => format!("{xmr} · {worth} · {old}"),
            Some((worth, None)) => format!("{xmr} · {worth}"),
            None => xmr,
        }
    }

    /// Contacts by name, those whose name or note has `search` in it.
    pub async fn contacts(&self, search: &str, clock: Clock) -> Vec<contacts::Row> {
        let rows = self.activity().await;
        contacts::list(&self.inner.state.lock().await.contacts, &rows, search, clock)
    }

    pub async fn contact(&self, id: u64) -> Option<Contact> {
        self.inner.state.lock().await.contacts.get(id).cloned()
    }

    /// A contact's card, or `None` once it's deleted.
    pub async fn contact_card(&self, id: u64, clock: Clock) -> Option<contacts::Card> {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        state.contacts.get(id).map(|c| contacts::card(c, &state.requests, &rows, clock))
    }

    /// What the screens list, from one look at the activity (it parses
    /// every receipt): Home's Activity, by day, with every payment, request
    /// and share of your profile; Contacts and its "Paid, not saved"; Send's
    /// sheet's contacts (with a send address, the most recently paid first)
    /// and the people paid before.
    pub async fn lists(&self, clock: Clock) -> Lists {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        let mut lines = requests::activity_rows(&state.requests, &state.contacts, &rows, clock);
        lines.extend(profile::activity_rows(&state.profile, clock));
        lines.sort_by_key(|l| std::cmp::Reverse(l.time));
        let lists = Lists {
            contacts: contacts::list(&state.contacts, &rows, "", clock),
            unsaved: contacts::unsaved(&state.contacts, &rows, clock),
            on_send: contacts::on_send(&state.contacts, &rows, clock),
            paid_before: activity::paid_before(&rows, clock),
            days: vec![],
        };
        drop(state);
        Lists { days: activity::by_day(rows, lines, clock), ..lists }
    }

    /// The name a request's code carries (`recipient_name`): the profile's,
    /// while "Name in payment requests" is on.
    fn name_in_requests(state: &State) -> Option<String> {
        state.profile.name_in_requests().map(Into::into)
    }

    /// Your profile, once it's set up.
    pub async fn profile(&self) -> Option<profile::Profile> {
        self.inner.state.lock().await.profile.profile().cloned()
    }

    /// Settings' row: "Your profile" over "Henry · share as a code", or
    /// only "Add a shareable Fob contact" before there's a name.
    pub async fn profile_row(&self) -> profile::SettingsRow {
        profile::settings_row(self.inner.state.lock().await.profile.profile())
    }

    /// Changes the profile and saves it, or leaves it as it was when
    /// `change` refuses.
    async fn change_profile<T>(&self, change: impl FnOnce(&mut profile::Book) -> Result<T>) -> Result<T> {
        let mut state = self.inner.state.lock().await;
        let mut book = state.profile.clone();
        let done = change(&mut book)?;
        let before = std::mem::replace(&mut state.profile, book);
        if let Err(e) = self.save_state(&state) {
            state.profile = before;
            return Err(e);
        }
        Ok(done)
    }

    /// Sets your name and photo ("Make it yours", or Edit).
    pub async fn set_profile(&self, name: &str, photo: Option<Vec<u8>>) -> Result<profile::Profile> {
        self.change_profile(|book| book.set(name, photo)).await
    }

    /// "Name in payment requests", on or off.
    pub async fn set_name_in_requests(&self, on: bool) -> Result<()> {
        self.change_profile(|book| book.set_name_in_requests(on)).await
    }

    /// "Share my profile": a share on a subaddress no one has had, recorded
    /// now, however briefly its code is shown. Returns it with its code.
    pub async fn share_profile(&self) -> Result<(profile::Share, String)> {
        let mut state = self.inner.state.lock().await;
        let mut changed = state.clone();
        let subaddress = Self::hand_out(&mut changed);
        let share = changed.profile.share(subaddress, self.inner.keys.address(subaddress), profile::ShareKind::CodeShown, now())?;
        self.save_state(&changed)?;
        *state = changed;
        *self.inner.fresh_share.lock().expect("share lock") = Some(share.id);
        let code = profile::code(state.profile.profile().expect("a share needs a profile"), &share);
        Ok((share, code))
    }

    /// Whether share `id` can still swap by touching phones: the one the
    /// last Share my profile made, its swap not over, its code only shown
    /// (a link that went out, or a touch, already put its address with
    /// someone).
    fn can_touch(&self, share: &profile::Share) -> bool {
        *self.inner.fresh_share.lock().expect("share lock") == Some(share.id) && share.kind == profile::ShareKind::CodeShown
    }

    /// Touch phones, while the screen of the share Share my profile just
    /// made is open: a swap of the profile on that share's own address, so
    /// one opening hands out one address however it goes out. Any other
    /// share (reopened from Your shares or Activity, or already gone out
    /// as a link or a touch) is refused: a second person would get an
    /// address someone already has.
    pub async fn start_swap(&self, id: u64) -> Result<Arc<swap::Session>> {
        let state = self.inner.state.lock().await;
        let profile = state.profile.profile().cloned().ok_or_else(|| FobError::Profile("Set up your profile first.".into()))?;
        let share = state.profile.get(id).ok_or_else(|| FobError::Profile("That share was deleted.".into()))?;
        if !self.can_touch(share) {
            return Err(FobError::Profile("Touch phones works on a new Share my profile only.".into()));
        }
        let session = Arc::new(swap::Session::new(
            self.network(),
            &uri::profile_uri(&share.address, &profile.name, profile.photo.as_deref()),
            share.subaddress,
            share.address.clone(),
        )?);
        *self.inner.live_swap.lock().expect("swap lock") = Arc::downgrade(&session);
        Ok(session)
    }

    /// The swap is over (it worked, or the screen closed): nothing more is
    /// answered or read, and its share swaps no more. The share is kept as
    /// "Touched phones with Sam" when theirs came, or "Touched phones" when
    /// only ours went out whole; unchanged when neither, or when it's kept
    /// so already. Returns the share, their profile as if scanned, and
    /// whether the other phone said it took ours.
    pub async fn end_swap(&self, session: &swap::Session) -> Result<Option<(profile::Share, profile::Scanned, bool)>> {
        let ended = session.end();
        let mut state = self.inner.state.lock().await;
        let Some(id) = state.profile.at(session.subaddress()).map(|s| s.id) else { return Ok(None) };
        {
            let mut fresh = self.inner.fresh_share.lock().expect("share lock");
            if *fresh == Some(id) {
                *fresh = None;
            }
        }
        if ended.got.is_none() && !ended.gave {
            return Ok(None);
        }
        let mut changed = state.clone();
        let Some(share) = changed.profile.touched(id, ended.got.as_ref().map(|p| p.name.clone())) else { return Ok(None) };
        self.save_state(&changed)?;
        *state = changed;
        Ok(ended.got.map(|code| (share, profile::scanned(&state.contacts, code), ended.sure)))
    }

    /// A share's code again (its address, the profile as it is now), and
    /// whether it can still swap by touching phones; `None` for no such
    /// share or no profile.
    pub async fn share_code(&self, id: u64) -> Option<(profile::Share, String, bool)> {
        let state = self.inner.state.lock().await;
        let share = state.profile.get(id)?;
        Some((share.clone(), profile::code(state.profile.profile()?, share), self.can_touch(share)))
    }

    /// Copy link or Share pressed on a share's screen: kept as that kind.
    /// Its address is now with whoever gets the link, so a swap running on
    /// it ends at once.
    pub async fn share_went_out(&self, id: u64, kind: profile::ShareKind) -> Result<()> {
        let live = self.inner.live_swap.lock().expect("swap lock").upgrade();
        if let Some(session) = live
            && self.inner.state.lock().await.profile.at(session.subaddress()).is_some_and(|s| s.id == id)
        {
            session.end();
        }
        self.change_profile(|book| Ok(book.went_out(id, kind))).await.map(|_| ())
    }

    /// Your shares, newest first, each with what came to its address.
    pub async fn profile_shares(&self, clock: Clock) -> Vec<profile::Line> {
        let rows = self.activity().await;
        profile::lines(&self.inner.state.lock().await.profile, &rows, clock)
    }

    /// A scanned profile code, and the contact it would change, if any.
    pub async fn scanned_profile(&self, text: &str) -> Result<profile::Scanned> {
        let code = uri::parse_profile(self.network(), text)?;
        Ok(profile::scanned(&self.inner.state.lock().await.contacts, code))
    }

    /// "Add contact" on a scanned profile, or "Add as new contact" once
    /// asked (`as_new`): its name, photo and address as the send address.
    /// Checked again here, whatever the screen showed: a profile whose
    /// address is a contact's is never added beside it (Update is the way),
    /// and one whose name is (or looks like) a contact's only once asked.
    pub async fn add_contact_from_profile(&self, text: &str, as_new: bool) -> Result<Contact> {
        let code = uri::parse_profile(self.network(), text)?;
        self.change_contacts(|book, network, _| {
            if let Some(owner) = book.owner(&code.address) {
                return Err(FobError::Contact(format!("That address is {}'s already: update {} instead.", owner.name, owner.name)));
            }
            if let Some(named) = book.named(&code.name).filter(|_| !as_new) {
                return Err(FobError::Contact(format!("You have a contact named {}: choose Update or Add as new contact.", named.name)));
            }
            book.add(network, profile::new_contact(&code), now())
        })
        .await
    }

    /// "Update Sam" on a scanned profile, once asked: the contact takes its
    /// name, photo and address as the send address.
    pub async fn update_contact_from_profile(&self, id: u64, text: &str) -> Result<Contact> {
        let code = uri::parse_profile(self.network(), text)?;
        self.change_contacts(|book, network, paid| {
            let contact = book.get(id).ok_or_else(|| FobError::Contact("That contact was deleted.".into()))?;
            book.edit(network, id, profile::updated(contact, &code), paid)
        })
        .await
    }

    /// Makes a request, when Share or Copy is pressed on Receive (never by
    /// opening it): on a subaddress no one has had, for `contact` (or no
    /// one), asking `amount_text` (in money when `in_fiat`, converted at the
    /// current price as the field's line showed) with `note`, its code
    /// naming the profile while requests carry it. The same ask again, while
    /// nothing has come to the last one, gives the last one again
    /// ([`requests::Book::same_again`]). Returns it with its code.
    pub async fn make_request(&self, contact: Option<u64>, amount_text: Option<&str>, in_fiat: bool, note: Option<&str>) -> Result<(requests::Request, ReceiveCode)> {
        let rows = self.activity().await;
        let typed = amount_text.map(str::trim).filter(|a| !a.is_empty());
        let asked = match typed {
            None => None,
            Some(t) => {
                let (xmr, fiat) = self.typed(t, in_fiat).await?;
                Some(requests::Asked {
                    typed: t.to_owned(),
                    unit: fiat.as_ref().map(|(_, code)| code.clone()).unwrap_or_else(|| "XMR".into()),
                    xmr,
                    fiat: fiat.map(|(text, _)| text),
                })
            }
        };
        let mut state = self.inner.state.lock().await;
        if contact.is_some_and(|id| state.contacts.get(id).is_none()) {
            return Err(FobError::Contact("That contact was deleted.".into()));
        }
        let name = Self::name_in_requests(&state);
        let ask = requests::Draft { subaddress: 0, address: String::new(), contact, note: note.map(Into::into), asked, name };
        if let Some(again) = state.requests.same_again(&ask, &rows) {
            return Ok((again.clone(), Self::request_code_of(again)));
        }
        let mut changed = state.clone();
        let subaddress = Self::hand_out(&mut changed);
        let draft = requests::Draft { subaddress, address: self.inner.keys.address(subaddress), ..ask };
        let request = changed.requests.add(draft, now())?;
        self.save_state(&changed)?;
        *state = changed;
        let code = Self::request_code_of(&request);
        Ok((request, code))
    }

    fn request_code_of(request: &requests::Request) -> ReceiveCode {
        let note = request.note.clone();
        let (headline, detail) = match &request.asked {
            Some(asked @ requests::Asked { fiat: Some(fiat), .. }) => {
                (Some(fiat.clone()), Some([Some(format!("{} XMR", amount::format(asked.xmr))), note].into_iter().flatten().collect::<Vec<_>>().join(" · ")))
            }
            Some(asked) => (Some(format!("{} XMR", amount::format(asked.xmr))), note),
            None => (None, note),
        };
        ReceiveCode { uri: requests::code_uri(request), headline, detail }
    }

    /// A request's code again, as it was made ("Show the code again"),
    /// whatever the profile's name is now.
    pub async fn request_code(&self, id: u64) -> Option<ReceiveCode> {
        let state = self.inner.state.lock().await;
        Some(Self::request_code_of(state.requests.get(id)?))
    }

    /// A request's own screen, or `None` once it's deleted.
    pub async fn request(&self, id: u64, clock: Clock) -> Option<requests::Detail> {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        state.requests.get(id).map(|r| requests::detail(r, &state.contacts, &rows, clock))
    }

    /// Deletes a request: it leaves Activity and its contact's card; its
    /// address stays this wallet's, and what's paid there later shows
    /// without a name.
    pub async fn delete_request(&self, id: u64) -> Result<()> {
        // What's in already, the pool's too, stays named when it's mined.
        let rows = self.activity().await;
        let mut state = self.inner.state.lock().await;
        let mut changed = state.requests.clone();
        if changed.delete(id, now(), &rows) {
            let before = std::mem::replace(&mut state.requests, changed);
            if let Err(e) = self.save_state(&state) {
                state.requests = before;
                return Err(e);
            }
        }
        Ok(())
    }

    /// Share's sheet ("Who is this for?"): every contact, each with its
    /// requests in a line.
    pub async fn request_contacts(&self) -> Vec<contacts::Row> {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        requests::sheet(&state.contacts, &state.requests, &rows)
    }

    /// Changes the contacts and saves them, or leaves them as they were when
    /// `change` refuses. `change` is told which addresses were paid.
    async fn change_contacts<T>(&self, change: impl FnOnce(&mut contacts::Book, Network, &dyn Fn(&str) -> bool) -> Result<T>) -> Result<T> {
        let mut state = self.inner.state.lock().await;
        let mut book = state.contacts.clone();
        let paid: HashSet<&str> = state.sent.iter().filter(|s| s.pieces.is_none()).map(|s| s.to.as_str()).collect();
        let done = change(&mut book, self.network(), &|a| paid.contains(a))?;
        state.contacts = book;
        self.save_state(&state)?;
        Ok(done)
    }

    pub async fn add_contact(&self, draft: contacts::Draft) -> Result<Contact> {
        self.change_contacts(|book, network, _| book.add(network, draft, now())).await
    }

    pub async fn edit_contact(&self, id: u64, draft: contacts::Draft) -> Result<Contact> {
        self.change_contacts(|book, network, paid| book.edit(network, id, draft, paid)).await
    }

    /// A contact's send address, scanned or pasted (a payment code's
    /// address is taken).
    pub async fn set_contact_send_address(&self, id: u64, text: &str) -> Result<Contact> {
        self.change_contacts(|book, network, paid| book.set_send_address(network, id, text, paid)).await
    }

    /// Keeps an address paid (from a code named for the contact) with it.
    pub async fn add_contact_paid_at(&self, id: u64, address: &str) -> Result<Contact> {
        self.change_contacts(|book, network, _| book.add_paid_at(network, id, address)).await
    }

    /// The payment to `address` a contact made from it would be named by:
    /// the newest one from a code with a name (once paid as a named code's,
    /// an address is never offered as a send address), else the newest.
    fn naming_payment<'a>(state: &'a State, address: &str) -> Option<&'a Sent> {
        let mut paid: Vec<&Sent> = state.sent.iter().filter(|s| s.to == address && s.pieces.is_none()).collect();
        paid.sort_by_key(|s| std::cmp::Reverse(s.time));
        paid.iter().find(|s| s.recipient_name.is_some()).or(paid.first()).copied()
    }

    /// What the Paid screen offers for the payment `tx` (Add to Sam, or
    /// Save as a contact), or `None`: a split, or an address that's already
    /// a contact's. Its name is this payment's code's, else the one any
    /// earlier payment to the address went under.
    pub async fn save_offer(&self, tx: &str) -> Option<contacts::Offer> {
        let state = self.inner.state.lock().await;
        let sent = state.sent.iter().find(|s| s.tx == tx && s.pieces.is_none())?;
        let name = sent.recipient_name.as_deref().or_else(|| Self::naming_payment(&state, &sent.to)?.recipient_name.as_deref());
        contacts::offer(&state.contacts, &sent.to, name, sent.note.as_deref())
    }

    /// New contact for an address paid that's no contact's ("Paid, not
    /// saved"): as the Paid screen would have offered after its last
    /// payment, a code's name making it an address they were paid at.
    pub async fn new_contact_for(&self, address: &str) -> contacts::Draft {
        let state = self.inner.state.lock().await;
        let named = Self::naming_payment(&state, address);
        contacts::new_contact(address, named.and_then(|s| s.recipient_name.as_deref()), named.and_then(|s| s.note.as_deref()))
    }

    /// Deletes a contact; payments to its addresses stay.
    pub async fn delete_contact(&self, id: u64) -> Result<()> {
        self.change_contacts(|book, _, _| {
            book.delete(id);
            Ok(())
        })
        .await
    }

    /// Receive's payment code for `address`, with an amount typed in XMR or,
    /// when `in_fiat`, in money (the code still asks in XMR, converted at
    /// the current price), and a note for the payer.
    pub async fn receive_code(&self, address: &str, amount_text: Option<&str>, in_fiat: bool, note: Option<&str>) -> Result<ReceiveCode> {
        let typed = amount_text.map(str::trim).filter(|a| !a.is_empty());
        let (xmr, fiat) = match typed {
            None => (None, None),
            Some(t) => {
                let (xmr, fiat) = self.typed_amount(t, in_fiat).await?;
                (Some(xmr), fiat)
            }
        };
        let xmr = xmr.filter(|a| *a > 0);
        let xmr_text = xmr.map(amount::format);
        let code = uri::receive(address, xmr_text.as_deref(), note)?;
        Ok(match (fiat.filter(|_| xmr.is_some()), xmr_text) {
            (Some(fiat), Some(xmr)) => ReceiveCode {
                uri: code.uri,
                headline: Some(fiat),
                detail: Some([Some(format!("{xmr} XMR")), code.detail].into_iter().flatten().collect::<Vec<_>>().join(" · ")),
            },
            _ => ReceiveCode { uri: code.uri, headline: code.headline, detail: code.detail },
        })
    }

    /// Asks the provider for what's due, unless XMR only: the current price
    /// every 5 minutes (a minute when the user asked), the daily history
    /// once a day while a payment lacks a price. Returns whether anything
    /// changed. A failure goes to the run of failures as `asked` says.
    pub async fn refresh_prices(&self, asked: price::alert::Asked) -> Result<bool> {
        let provider = self.inner.state.lock().await.prices.provider;
        self.refresh_prices_from(&provider, asked, now()).await
    }

    async fn refresh_prices_from(&self, source: &impl price::Source, asked: price::alert::Asked, now: u64) -> Result<bool> {
        let pulled = asked != price::alert::Asked::Background;
        let (settings, code, current_due, history_due, lacking_day) = {
            let state = self.inner.state.lock().await;
            let settings = state.prices.clone();
            let Some(code) = settings.currency.clone() else { return Ok(false) };
            let window = price::window(source, now);
            // The newest day a payment lacks a price for, to name it.
            let lacking_day = self
                .payment_times(&state)
                .into_iter()
                .filter(|(tx, time)| {
                    window.contains(&price::day(*time))
                        && !state.price_records.get(tx).is_some_and(|r| r.iter().any(|r| r.currency == code))
                })
                .map(|(_, time)| price::day(time))
                .max();
            let current_due = price::current_due(&state.price_cache, &settings, now, pulled);
            let history_due = lacking_day.is_some() && price::history_due(&state.price_cache, &settings, now);
            (settings, code, current_due, history_due, lacking_day)
        };
        if !current_due && !history_due {
            return Ok(false);
        }
        let failed = |failure, day| price::alert::Failed { failure, provider: source.name().into(), currency: code.clone(), day };
        // Kept over a transport the provider doesn't answer over (an older
        // wallet file): never asked another way without the user's pick.
        let refused = match settings.transport {
            t if !source.answers_over(t) => Some(price::Failure::NotOverTor),
            t if !t.available() => Some(price::Failure::TorNotYet),
            _ => None,
        };
        if let Some(failure) = refused {
            self.inner.price_alerts.lock().expect("price lock").looked_up(Some(failed(failure, None)), asked);
            return Err(FobError::PriceFailed(failure));
        }
        let lookup = self.inner.price_lookups.fetch_add(1, Ordering::Relaxed) + 1;
        let current = if current_due { Some(source.current(&code, settings.transport).await) } else { None };
        let history = if history_due { Some(source.daily(&code, price::window(source, now), settings.transport).await) } else { None };

        let mut state = self.inner.state.lock().await;
        // Which leads on Home (fiat_first) only changes how amounts are drawn.
        let asked_for = |s: &price::Settings| (s.code().map(str::to_owned), s.provider, s.transport);
        if asked_for(&state.prices) != asked_for(&settings) {
            // Changed while asking: these answers are for settings gone.
            return Ok(false);
        }
        // A lookup started after this one has already come back (Retry
        // over a background one hanging): this one's answer, price or
        // failure, is older news and is dropped.
        let done = self.inner.price_looked_up.fetch_max(lookup, Ordering::Relaxed);
        if done > lookup {
            return Ok(false);
        }
        // A failure in a word: the current price's first, else the day's.
        let kind = |e: &FobError| match e {
            FobError::PriceFailed(f) => *f,
            _ => price::Failure::NotAnswering,
        };
        let failure = match (&current, &history) {
            (Some(Err(e)), _) => Some(failed(kind(e), None)),
            (_, Some(Err(e))) => Some(failed(kind(e), lacking_day)),
            _ => None,
        };
        self.inner.price_alerts.lock().expect("price lock").looked_up(failure, asked);
        let mut changed = false;
        let mut error = None;
        match current {
            Some(Ok(p)) => {
                state.price_cache.current = Some(price::Current { currency: code.clone(), provider: source.name().into(), price: p, at: now });
                changed = true;
            }
            Some(Err(e)) => error = Some(e),
            None => {}
        }
        match history {
            Some(Ok(days)) => {
                price::store_history(&mut state.price_cache, &settings, days, now);
                changed = true;
            }
            Some(Err(e)) => error = error.or(Some(e)),
            None => {}
        }
        if changed {
            self.record_prices(&mut state);
            self.save_state(&state)?;
        }
        match error {
            Some(e) => Err(e),
            None => Ok(changed),
        }
    }

    /// Asks for prices while Fob is on screen, until [`stop_prices`]: at
    /// once, then every 5 minutes, and at once again when a price setting
    /// changes. `listener` hears of each new price, so the app redraws.
    ///
    /// [`stop_prices`]: Wallet::stop_prices
    pub fn start_prices(&self, listener: Arc<dyn SyncListener>) {
        self.stop_prices();
        let wallet = self.clone();
        let handle = crate::runtime().spawn(async move {
            loop {
                let asked = wallet.inner.prices_asked.lock().expect("price lock").take().unwrap_or(price::alert::Asked::Background);
                let before = wallet.price_alert();
                let changed = wallet.refresh_prices(asked).await.unwrap_or(false);
                if changed || wallet.price_alert() != before {
                    listener.on_status(wallet.status());
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(price::REFRESH)) => {}
                    _ = wallet.inner.prices_wake.notified() => {}
                }
            }
        });
        *self.inner.prices_task.lock().expect("price lock") = Some(handle);
    }

    pub fn stop_prices(&self) {
        if let Some(handle) = self.inner.prices_task.lock().expect("price lock").take() {
            handle.abort();
        }
    }

    /// Pull to refresh: new blocks and the pool as [`sync_now`](Wallet::sync_now),
    /// and the price if a minute has passed. Returns when both are done.
    pub async fn pull(&self) {
        let _ = tokio::join!(self.sync_now(), self.refresh_prices(price::alert::Asked::User));
    }

    /// The toast's Retry: asks again as the user, and says what came of it.
    pub async fn retry_prices(&self, clock: Clock) -> Retried {
        let provider = self.inner.state.lock().await.prices.provider;
        self.retry_prices_from(&provider, clock).await
    }

    async fn retry_prices_from(&self, source: &impl price::Source, clock: Clock) -> Retried {
        // A run about one day's price says that day when it's found.
        let day = self.price_alert().and_then(|a| a.failed.day);
        let _ = self.refresh_prices_from(source, price::alert::Asked::User, clock.now).await;
        if let Some(toast) = self.price_toast(clock).await {
            return Retried::Failed(toast);
        }
        let state = self.inner.state.lock().await;
        let (title, line) = price::alert::updated(&state.price_cache, &state.prices, day, clock);
        Retried::Updated { title, line }
    }
}

impl Owned {
    /// Confirmations before it can be spent: 10 (60 for a mined reward), or
    /// more if its unlock time names a later block. Monero lets an output
    /// with unlock time `b` be spent once the chain has `b` blocks.
    fn needed(&self) -> u64 {
        let window = if self.coinbase { COINBASE_UNLOCK } else { UNLOCK };
        window.max(self.unlock_block.saturating_sub(self.height))
    }
}

fn confirmations(state: &State, height: u64) -> u64 {
    state.tip.saturating_sub(height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::price::alert::Asked;

    /// The phone's clock now, at UTC.
    fn clock_now() -> Clock {
        Clock { now: now(), utc_offset: 0, hours_24: true }
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fob-wallet-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("wallet.fob")
    }

    #[test]
    fn restore_points_are_heights_or_dates() {
        assert_eq!(restore_point(Network::Stagenet, " 123456 ").unwrap(), 123_456);
        let march = restore_point(Network::Mainnet, "2026-03-12").unwrap();
        let april = restore_point(Network::Mainnet, "2026-04-12").unwrap();
        assert_eq!(april - march, 31 * 720);
        assert!(restore_point(Network::Mainnet, "12/03/2026").is_err());
        assert!(restore_point(Network::Mainnet, "2026-13-01").is_err());
    }

    #[tokio::test]
    async fn coins_are_found_ready_or_piece_by_piece_not_yet() {
        let wallet = Wallet::create(&dir("not-ready"), [1; 32], Network::Stagenet).unwrap();
        let owned = |tx: &str, height: u64, amount: u64, coinbase: bool| Owned {
            output: String::new(),
            tx: tx.into(),
            height,
            time: 1_000 + height,
            amount,
            subaddress: 0,
            coinbase,
            unlock_block: if coinbase { height + 60 } else { 0 },
            key: tx.into(),
            key_image: None,
            spent_in: None,
        };
        let sent = |tx: &str, height: Option<u64>, change: u64| Sent {
            tx: tx.into(),
            to: "86Ux3m9Pqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq7kQa".into(),
            amount: 1,
            fee: 1,
            change,
            time: 5_000,
            height,
            receipt: None,
            note: None,
            pieces: None,
            back: vec![],
            recipient_name: None,
        };
        {
            let mut state = wallet.inner.state.lock().await;
            state.tip = 100;
            state.last_block_time = 4_000;
            state.outputs = vec![
                owned("in", 97, 10, false),
                owned("old", 50, 20, false),
                owned("mined", 80, 30, true),
                owned("change", 99, 40, false),
                owned("restored", 98, 50, false),
                Owned { unlock_block: 200, ..owned("locked", 40, 80, false) },
                owned("split", 60, 25, false),
            ];
            state.sent = vec![
                sent("change", Some(99), 40),
                sent("pending", None, 60),
                sent("nothing-back", None, 0),
                Sent { pieces: Some(1), ..sent("split", Some(60), 25) },
            ];
            state.spends = vec![ChainSpend { tx: "restored".into(), height: 98, time: 1_098, fee: 1 }];
        }
        wallet.pool_found([("pool".into(), None, 70)], 6_000);

        let (ready, pieces, last) = wallet.coin_pieces().await;
        let shown: Vec<_> = pieces.iter().map(|p| (p.amount, p.confirmations, p.needed, p.source.clone())).collect();
        let to = Some("86Ux … 7kQa".to_owned());
        assert_eq!(
            shown,
            [
                (10, 3, 10, Source::Received),
                (30, 20, 60, Source::Received),
                (40, 1, 10, Source::Change(to.clone())),
                (50, 2, 10, Source::Change(None)),
                (80, 60, 160, Source::Received),
                (60, 0, 10, Source::Change(to)),
                (70, 0, 10, Source::Received),
            ]
        );
        assert_eq!(last, 4_000);
        // Activity holds each payment in to the same rule: "Not ready"
        // until its outputs can be spent, not at 10 for every one.
        let lines: HashMap<String, String> = wallet.lists(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await.days
            .into_iter()
            .flat_map(|d| d.rows)
            .map(|r| (r.activity.unwrap().tx, r.line.split(" · ").take(2).collect::<Vec<_>>().join(" · ")))
            .collect();
        assert_eq!(lines["in"], "Not ready · 3 of 10");
        assert_eq!(lines["mined"], "Not ready · 20 of 60");
        assert_eq!(lines["locked"], "Not ready · 60 of 160");
        assert!(!lines["old"].starts_with("Not ready"), "{}", lines["old"]);
        // Ready: what the balance counts as unlocked, a split's coin named as one.
        let shown: Vec<_> = ready.iter().map(|p| (p.amount, p.source.clone())).collect();
        assert_eq!(shown, [(20, Source::Received), (25, Source::Split)]);
        // The same money the balance counts as not unlocked.
        let balance = wallet.balance().await;
        assert_eq!(balance.unlocked, ready.iter().map(|p| p.amount).sum::<u64>());
        assert_eq!(balance.total - balance.unlocked, pieces.iter().map(|p| p.amount).sum::<u64>());
        // Coins ready to spend: Split up can be pressed.
        assert!(wallet.coins(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await.can_split);
        // The Coins screen counts them all.
        let coins = wallet.coins(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await;
        assert_eq!((coins.ready_coins.len(), coins.not_ready_coins.len()), (2, 7));
        assert_eq!((coins.ready, coins.not_ready), (balance.unlocked, balance.total - balance.unlocked));
        assert!(coins.intro.contains("is held as 9 coins"), "{}", coins.intro);
        // Home's sums: ready, not ready, and all of it, the balance.
        let summary = wallet.not_ready(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await.unwrap();
        let sums: Vec<_> = summary.sums.iter().map(|s| (s.label.as_str(), s.atomic)).collect();
        assert_eq!(sums, [("Ready to spend", balance.unlocked), ("Not ready · 7 coins", balance.total - balance.unlocked), ("All of it", balance.total)]);
    }

    #[tokio::test]
    async fn a_wallet_opens_again_with_the_same_addresses() {
        let path = dir("reopen");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let first = wallet.receive_address().await.unwrap();
        let second = wallet.new_receive_address().await.unwrap();
        assert_ne!(first, second);
        let again = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(again.primary_address(), wallet.primary_address());
        assert_eq!(again.receive_address().await.unwrap(), second);
        assert!(matches!(Wallet::open(&path, [2; 32]), Err(FobError::WrongKey)));
    }

    #[tokio::test]
    async fn a_wallet_code_restores_the_wallet_it_came_from() {
        let wallet = Wallet::create(&dir("code-a"), [1; 32], Network::Stagenet).unwrap();
        let code = wallet.wallet_code().await;
        let preview = preview_wallet_code(Network::Stagenet, &code).unwrap();
        assert_eq!((preview.address.as_str(), preview.can_spend), (wallet.primary_address().as_str(), true));
        let restored = Wallet::restore_code(&dir("code-b"), [1; 32], Network::Stagenet, &code, None).unwrap();
        assert_eq!(restored.primary_address(), wallet.primary_address());
        assert!(restored.can_spend());
    }

    #[tokio::test]
    async fn a_view_only_wallet_refuses_to_prepare_payments() {
        let full = Keys::generate(Network::Stagenet).keys;
        let watch =
            Wallet::restore_view_only(&dir("watch"), [1; 32], Network::Stagenet, &full.primary_address(), &full.view_key_hex(), Some(5)).unwrap();
        assert!(!watch.can_spend());
        // Even holding a ready coin, a view-only wallet can't split.
        watch.inner.state.lock().await.outputs = vec![Owned {
            output: String::new(),
            tx: "in".into(),
            height: 0,
            time: 0,
            amount: 10,
            subaddress: 0,
            coinbase: false,
            unlock_block: 0,
            key: "in".into(),
            key_image: None,
            spent_in: None,
        }];
        watch.inner.state.lock().await.tip = 100;
        let coins = watch.coins(Clock { now: 6_000, utc_offset: 0, hours_24: true }).await;
        assert!(coins.ready > 0 && !coins.can_split);
        assert!(matches!(watch.prepare(&full.primary_address(), 1, None, None).await, Err(FobError::ViewOnly)));
    }

    #[tokio::test]
    async fn an_empty_wallet_has_nothing_to_show() {
        let wallet = Wallet::create(&dir("empty"), [1; 32], Network::Stagenet).unwrap();
        assert_eq!(wallet.balance().await, Balance::default());
        assert!(wallet.activity().await.is_empty());
    }

    struct Quiet;
    impl SyncListener for Quiet {
        fn on_status(&self, _: SyncStatus) {}
    }

    /// A wallet whose node refuses connections, so every round fails at once.
    async fn unreachable(name: &str) -> Wallet {
        let wallet = Wallet::create(&dir(name), [1; 32], Network::Stagenet).unwrap();
        wallet.inner.state.lock().await.node = "http://127.0.0.1:1".into();
        wallet
    }

    #[tokio::test]
    async fn sync_now_cuts_the_wait_short_and_returns_when_the_round_ends() {
        let wallet = unreachable("sync-now").await;
        wallet.start_sync(Arc::new(Quiet));
        // Let the first round fail; the loop then waits out its backoff.
        let mut rounds = wallet.inner.rounds.subscribe();
        tokio::time::timeout(Duration::from_secs(5), rounds.changed()).await.unwrap().unwrap();
        let started = Instant::now();
        wallet.sync_now().await;
        // Sooner than the 2 s backoff it cut short.
        assert!(started.elapsed() < Duration::from_secs(1), "waited {:?}", started.elapsed());
        assert!(*wallet.inner.rounds.borrow() >= 2);
        assert!(wallet.status().error.is_some());
        wallet.stop_sync();
    }

    /// A new block announced by the node's publisher ends the loop's wait
    /// at once: here a 4 s backoff, cut short well inside a second.
    #[tokio::test]
    async fn a_block_announced_over_zmq_wakes_the_sync_loop() {
        let wallet = unreachable("zmq-wake").await;
        let publisher = crate::zmq::test_publisher::Publisher::bind("tcp://127.0.0.1:0").await;
        wallet.set_zmq(true, Some(publisher.endpoint())).await.unwrap();
        assert_eq!(wallet.status().zmq, ZmqStatus::Connecting, "on, not syncing yet");
        let mut rounds = wallet.inner.rounds.subscribe();
        wallet.start_sync(Arc::new(Quiet));
        let listening = async {
            while wallet.status().zmq != ZmqStatus::Listening {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), listening).await.expect("never listening");
        assert!(wallet.status().zmq_text.ends_with(" · listening"), "{}", wallet.status().zmq_text);
        // The first round, and the one connecting wakes for what was missed.
        while *rounds.borrow_and_update() < 2 {
            tokio::time::timeout(Duration::from_secs(5), rounds.changed()).await.unwrap().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let settled = *rounds.borrow_and_update();
        let started = Instant::now();
        // Sent until the subscription has reached the publisher.
        let woke = async {
            loop {
                publisher.send(&format!("{}:{{}}", crate::zmq::CHAIN_TOPIC));
                if tokio::time::timeout(Duration::from_millis(50), rounds.changed()).await.is_ok() {
                    return;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(3), woke).await.expect("the announcement never woke the loop");
        assert!(started.elapsed() < Duration::from_secs(1), "woke after {:?}", started.elapsed());
        assert!(*wallet.inner.rounds.borrow() > settled);
        // Pool announcements don't start a round of their own.
        let pooled = *wallet.inner.rounds.borrow();
        publisher.send(&format!("{}:[]", crate::zmq::POOL_TOPIC));
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(*wallet.inner.rounds.borrow(), pooled);
        wallet.stop_sync();
        assert_eq!(wallet.status().zmq, ZmqStatus::Connecting, "still on, for when syncing starts again");
        wallet.set_zmq(false, None).await.unwrap();
        assert_eq!(wallet.status().zmq, ZmqStatus::Off);
        assert_eq!(wallet.zmq_endpoint().await, publisher.endpoint(), "kept for turning on again");
    }

    #[tokio::test]
    async fn zmq_is_off_until_turned_on_and_follows_the_node_s_host() {
        let wallet = Wallet::create(&dir("zmq-settings"), [1; 32], Network::Stagenet).unwrap();
        assert_eq!(wallet.status().zmq, ZmqStatus::Off);
        assert_eq!(wallet.status().zmq_text, "Off");
        // Offered, not used: the node's host, stagenet's usual port.
        assert_eq!(wallet.zmq_endpoint().await, "tcp://stagenet.xmr-tw.org:38083");
        assert!(matches!(wallet.set_zmq(true, Some("tcp://elsewhere.example:38083")).await, Err(FobError::BadEndpoint(_))));
        assert_eq!(wallet.status().zmq, ZmqStatus::Off, "refused, so still off");
        wallet.set_zmq(true, None).await.unwrap();
        assert_eq!(wallet.status().zmq_text, "stagenet.xmr-tw.org:38083 · connecting");
        let path = wallet.inner.path.clone();
        drop(wallet);
        let again = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(again.status().zmq, ZmqStatus::Connecting, "saved with the wallet");
        // Another host (set without asking the node, as set_node would):
        // instant updates go off rather than follow somewhere unchosen.
        again.inner.state.lock().await.node = "http://127.0.0.1:1".into();
        assert!(matches!(again.set_zmq(true, Some("tcp://stagenet.xmr-tw.org:38083")).await, Err(FobError::BadEndpoint(_))));
    }

    /// A provider standing in for Kraken: a fixed price now, and a history.
    struct FakePrices {
        now: u64,
        days: Vec<(i64, u64)>,
        fail: bool,
        asked_now: AtomicU64,
        asked_days: AtomicU64,
        /// Answers the current price only once this is notified.
        hold: Option<Arc<tokio::sync::Notify>>,
    }

    impl FakePrices {
        fn new(now: u64, days: Vec<(i64, u64)>) -> FakePrices {
            FakePrices { now, days, fail: false, asked_now: AtomicU64::new(0), asked_days: AtomicU64::new(0), hold: None }
        }

        fn asked(&self) -> (u64, u64) {
            (self.asked_now.load(Ordering::Relaxed), self.asked_days.load(Ordering::Relaxed))
        }
    }

    impl price::Source for FakePrices {
        fn name(&self) -> &'static str {
            "Kraken"
        }

        fn currencies(&self) -> &'static [&'static str] {
            &["USD", "EUR"]
        }

        fn answers_over(&self, transport: price::Transport) -> bool {
            transport == price::Transport::Web
        }

        async fn current(&self, _: &str, _: price::Transport) -> Result<u64> {
            self.asked_now.fetch_add(1, Ordering::Relaxed);
            if let Some(hold) = &self.hold {
                hold.notified().await;
            }
            if self.fail { Err(FobError::Price("no answer".into())) } else { Ok(self.now) }
        }

        async fn daily(&self, _: &str, days: std::ops::Range<i64>, _: price::Transport) -> Result<Vec<(i64, u64)>> {
            self.asked_days.fetch_add(1, Ordering::Relaxed);
            if self.fail { Err(FobError::Price("no answer".into())) } else { Ok(self.days.iter().copied().filter(|d| days.contains(&d.0)).collect()) }
        }

        fn history_days(&self) -> i64 {
            720
        }
    }

    const XMR: u64 = amount::ATOMIC_PER_XMR;
    const DAY: u64 = 86_400;

    fn received(tx: &str, time: u64, amount: u64) -> Owned {
        Owned {
            output: String::new(),
            tx: tx.into(),
            height: 10,
            time,
            amount,
            subaddress: 1,
            coinbase: false,
            unlock_block: 0,
            key: tx.into(),
            key_image: None,
            spent_in: None,
        }
    }

    fn price_of(activity: &[Activity], tx: &str) -> Option<(u64, bool)> {
        activity.iter().find(|a| a.tx == tx).and_then(|a| a.price.as_ref()).map(|p| (p.price, p.daily))
    }

    #[tokio::test]
    async fn a_payment_seen_now_keeps_the_price_of_the_moment() {
        let wallet = Wallet::create(&dir("price-now"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let fake = FakePrices::new(166_060_000, vec![]);
        // Nothing to price yet: the current price only, no history.
        assert!(wallet.refresh_prices_from(&fake, Asked::Background, t).await.unwrap());
        assert_eq!(fake.asked(), (1, 0));
        assert_eq!(wallet.worth_now(XMR / 2).await.as_deref(), Some("$83.03"));
        {
            let mut state = wallet.inner.state.lock().await;
            state.outputs.push(received("in", t - 120, XMR / 2));
            state.sent.push(Sent { tx: "out".into(), to: "86Ux".into(), amount: XMR / 10, fee: 1, change: 0, time: t, height: None, receipt: None, note: None, pieces: None, back: vec![], recipient_name: None });
            assert!(wallet.record_prices(&mut state));
            wallet.save_state(&state).unwrap();
        }
        let activity = wallet.activity().await;
        assert_eq!(price_of(&activity, "in"), Some((166_060_000, false)));
        assert_eq!(price_of(&activity, "out"), Some((166_060_000, false)));
        // Kept in the wallet file, with who priced it.
        let again = Wallet::open(&wallet.inner.path, [1; 32]).unwrap();
        let record = again.activity().await.into_iter().find(|a| a.tx == "in").unwrap().price.unwrap();
        assert_eq!((record.currency.as_str(), record.provider.as_str(), record.at), ("USD", "Kraken", t));
        // Asked again only after 5 minutes (a minute when pulled).
        assert!(!wallet.refresh_prices_from(&fake, Asked::Background, t + 299).await.unwrap());
        assert!(wallet.refresh_prices_from(&fake, Asked::User, t + 60).await.unwrap());
        assert_eq!(fake.asked(), (2, 0));
    }

    #[tokio::test]
    async fn older_payments_are_priced_from_the_daily_history() {
        let wallet = Wallet::create(&dir("price-history"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let today = price::day(t);
        let fake = FakePrices::new(170_000_000, vec![(today - 10, 150_000_000), (today - 1, 160_000_000), (today, 165_000_000)]);
        {
            let mut state = wallet.inner.state.lock().await;
            state.outputs.push(received("ten-days", t - 10 * DAY, XMR));
            state.outputs.push(received("too-old", t - 900 * DAY, XMR));
            // No price on hand: nothing recorded yet.
            assert!(!wallet.record_prices(&mut state));
        }
        assert!(wallet.refresh_prices_from(&fake, Asked::Background, t).await.unwrap());
        assert_eq!(fake.asked(), (1, 1), "the whole window, once");
        let activity = wallet.activity().await;
        assert_eq!(price_of(&activity, "ten-days"), Some((150_000_000, true)));
        // Older than Kraken's window: not available, and not asked about.
        assert_eq!(price_of(&activity, "too-old"), None);
        // A payment found later is priced from what's cached, as a sync
        // step finds it, and the history isn't asked for again that day.
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("yesterday", t - DAY, XMR));
        assert!(wallet.record_prices(&mut state));
        drop(state);
        assert_eq!(price_of(&wallet.activity().await, "yesterday"), Some((160_000_000, true)));
        state = wallet.inner.state.lock().await;
        state.outputs.push(received("last-week", t - 7 * DAY, XMR));
        assert!(!wallet.record_prices(&mut state), "not in the history fetched");
        drop(state);
        wallet.refresh_prices_from(&fake, Asked::Background, t + 600).await.unwrap();
        assert_eq!(fake.asked(), (2, 1));
        assert_eq!(price_of(&wallet.activity().await, "last-week"), None);
        // The next day it is.
        wallet.refresh_prices_from(&fake, Asked::Background, t + DAY).await.unwrap();
        assert_eq!(fake.asked(), (3, 2));
    }

    #[tokio::test]
    async fn with_xmr_only_nothing_is_asked_or_shown() {
        let wallet = Wallet::create(&dir("price-off"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let fake = FakePrices::new(166_000_000, vec![(price::day(t), 165_000_000)]);
        wallet.refresh_prices_from(&fake, Asked::Background, t).await.unwrap();
        assert_eq!(wallet.set_currency(None).await.unwrap(), None);
        wallet.inner.state.lock().await.outputs.push(received("while-off", t, XMR));
        assert!(!wallet.refresh_prices_from(&fake, Asked::User, t + DAY).await.unwrap());
        assert_eq!(fake.asked(), (1, 0), "no request with XMR only");
        assert_eq!(wallet.worth_now(XMR).await, None);
        assert!(!wallet.record_prices(&mut *wallet.inner.state.lock().await));
        assert!(wallet.activity().await.iter().all(|a| a.price.is_none()));
        // Dollars again: the payment seen meanwhile is priced from what's cached.
        wallet.set_currency(Some("USD")).await.unwrap();
        assert_eq!(price_of(&wallet.activity().await, "while-off"), Some((166_000_000, false)));
    }

    #[tokio::test]
    async fn a_failed_request_leaves_the_payment_not_available() {
        let wallet = Wallet::create(&dir("price-fail"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        wallet.inner.state.lock().await.outputs.push(received("in", t - 3 * DAY, XMR));
        let fake = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
        assert!(wallet.refresh_prices_from(&fake, Asked::Background, t).await.is_err());
        assert_eq!(fake.asked(), (1, 1));
        assert_eq!(price_of(&wallet.activity().await, "in"), None);
        assert_eq!(wallet.worth_now(XMR).await, None);
    }

    #[tokio::test]
    async fn failed_lookups_show_once_per_run_in_the_background_and_always_for_the_user() {
        let wallet = Wallet::create(&dir("price-alerts"), [1; 32], Network::Stagenet).unwrap();
        // Yesterday's noon, UTC: the steps below never cross midnight, so
        // the kept price is "the hh:mm price", not "yesterday's".
        let t = now() / DAY * DAY - DAY / 2;
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
        let working = FakePrices::new(166_100_000, vec![]);
        assert_eq!(wallet.price_from_line(clock).await, ("Kraken · web".into(), false));
        // The first background failure shows.
        assert!(wallet.refresh_prices_from(&failing, Asked::Background, t).await.is_err());
        let first = wallet.price_toast(clock).await.unwrap();
        assert_eq!((first.title.as_str(), first.line.as_str()), ("Couldn't get the USD price", "Kraken didn't answer · no USD price yet"));
        // Settings' row says the same, red.
        assert_eq!(wallet.price_from_line(clock).await, ("Kraken didn't answer · no USD price yet".into(), true));
        // Five minutes on, failing still: not again.
        wallet.refresh_prices_from(&failing, Asked::Background, t + 300).await.unwrap_err();
        assert_eq!(wallet.price_toast(clock).await.unwrap().id, first.id);
        // Pulled: shown again.
        wallet.refresh_prices_from(&failing, Asked::User, t + 400).await.unwrap_err();
        let pulled = wallet.price_toast(clock).await.unwrap();
        assert!(pulled.id > first.id);
        // A lookup that works ends it.
        wallet.refresh_prices_from(&working, Asked::Background, t + 600).await.unwrap();
        assert_eq!(wallet.price_toast(clock).await, None);
        assert!(!wallet.price_from_line(clock).await.1);
        // The next run's first background failure shows, with the price kept.
        wallet.refresh_prices_from(&failing, Asked::Background, t + 900).await.unwrap_err();
        let later = wallet.price_toast(Clock { now: t + 900, ..clock }).await.unwrap();
        assert!(later.id > pulled.id);
        assert!(later.line.starts_with("Kraken didn't answer · using the "), "{}", later.line);
        // Changing currency forgets the run (and asks at once, as the user).
        wallet.set_currency(Some("EUR")).await.unwrap();
        assert_eq!(wallet.price_toast(clock).await, None);
        assert_eq!(*wallet.inner.prices_asked.lock().unwrap(), Some(Asked::User));
    }

    /// A lookup that comes back after a newer one has changes nothing: a
    /// background failure landing after Retry worked doesn't bring the
    /// toast back, and an older price doesn't overwrite a newer one.
    #[tokio::test]
    async fn an_older_lookup_coming_back_late_changes_nothing() {
        let wallet = Wallet::create(&dir("price-late"), [1; 32], Network::Stagenet).unwrap();
        let clock = clock_now();
        let late = |fail, price| {
            let hold = Arc::new(tokio::sync::Notify::new());
            let fake = Arc::new(FakePrices { fail, hold: Some(hold.clone()), ..FakePrices::new(price, vec![]) });
            let (w, f) = (wallet.clone(), fake.clone());
            let task = tokio::spawn(async move { w.refresh_prices_from(&*f, Asked::Background, clock.now).await });
            (fake, hold, task)
        };
        let asked = |fake: &FakePrices| fake.asked().0 == 1;
        // A background lookup hangs; Retry works meanwhile.
        let (fake, hold, background) = late(true, 1);
        while !asked(&fake) {
            tokio::task::yield_now().await;
        }
        let worked = wallet.retry_prices_from(&FakePrices::new(166_100_000, vec![]), clock).await;
        assert!(matches!(worked, Retried::Updated { .. }), "{worked:?}");
        hold.notify_one();
        assert!(!background.await.unwrap().unwrap(), "dropped");
        assert_eq!(wallet.price_toast(clock).await, None, "no toast after Retry worked");
        // An older lookup's price landing after a newer one's is dropped too.
        wallet.inner.state.lock().await.price_cache.current = None;
        let (fake, hold, background) = late(false, 150_000_000);
        while !asked(&fake) {
            tokio::task::yield_now().await;
        }
        wallet.refresh_prices_from(&FakePrices::new(170_000_000, vec![]), Asked::User, clock.now).await.unwrap();
        hold.notify_one();
        background.await.unwrap().unwrap();
        assert_eq!(wallet.price_now().await.map(|(_, p)| p), Some(170_000_000));
    }

    /// Swapping Home's balance while a lookup is out keeps what the lookup
    /// brings: the order is only how amounts are drawn, not what's asked.
    #[tokio::test]
    async fn a_price_asked_while_the_balance_swaps_is_kept() {
        let wallet = Wallet::create(&dir("price-swap-while"), [1; 32], Network::Stagenet).unwrap();
        wallet.set_currency(Some("USD")).await.unwrap();
        let t = now();
        let hold = Arc::new(tokio::sync::Notify::new());
        let fake = Arc::new(FakePrices { hold: Some(hold.clone()), ..FakePrices::new(166_100_000, vec![]) });
        let (w, f) = (wallet.clone(), fake.clone());
        let lookup = tokio::spawn(async move { w.refresh_prices_from(&*f, Asked::User, t).await });
        while fake.asked().0 == 0 {
            tokio::task::yield_now().await;
        }
        wallet.set_fiat_first(true).await.unwrap();
        hold.notify_one();
        assert!(lookup.await.unwrap().unwrap(), "kept");
        assert_eq!(wallet.balance_worth(XMR, t).await.0, Some(("$166.10".into(), None)));
        // Another currency meanwhile is another question: that answer goes.
        let (w, f) = (wallet.clone(), fake.clone());
        wallet.inner.state.lock().await.price_cache.current = None;
        let lookup = tokio::spawn(async move { w.refresh_prices_from(&*f, Asked::User, t).await });
        while fake.asked().0 == 1 {
            tokio::task::yield_now().await;
        }
        wallet.set_currency(Some("EUR")).await.unwrap();
        hold.notify_one();
        assert!(!lookup.await.unwrap().unwrap(), "dropped");
    }

    /// An older file's Tor stays Tor: the lookups fail saying why rather
    /// than going over the web unannounced.
    #[tokio::test]
    async fn a_wallet_file_s_tor_is_never_moved_to_the_web_on_opening() {
        let path = dir("price-tor-file");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        file["state"]["prices"] = serde_json::json!({"currency": "USD", "provider": "kraken", "transport": "tor", "fiat_first": false});
        store::save(&path, &[1; 32], &file).unwrap();
        let old = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(old.price_settings().await.transport, price::Transport::Tor);
        let fake = FakePrices::new(166_100_000, vec![]);
        let failed = old.refresh_prices_from(&fake, Asked::Background, now()).await.unwrap_err();
        assert!(matches!(failed, FobError::PriceFailed(price::Failure::NotOverTor)));
        assert_eq!(fake.asked(), (0, 0), "nothing asked");
        assert_eq!(old.price_toast(clock_now()).await.unwrap().line, "Kraken doesn't answer over Tor · no USD price yet");
    }

    /// Swapping Home's balance only changes how amounts are drawn: no
    /// lookup, and a run of failures goes on.
    #[tokio::test]
    async fn swapping_the_balance_asks_for_nothing() {
        let wallet = Wallet::create(&dir("price-show-first"), [1; 32], Network::Stagenet).unwrap();
        let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
        wallet.refresh_prices_from(&failing, Asked::Background, now()).await.unwrap_err();
        let shown = wallet.price_alert().unwrap().id;
        wallet.set_fiat_first(true).await.unwrap();
        assert!(wallet.price_settings().await.fiat_first);
        assert_eq!(*wallet.inner.prices_asked.lock().unwrap(), None);
        assert_eq!(wallet.price_alert().map(|a| a.id), Some(shown));
        wallet.set_currency(Some("EUR")).await.unwrap();
        assert_eq!(*wallet.inner.prices_asked.lock().unwrap(), Some(Asked::User));
    }

    #[tokio::test]
    async fn a_day_s_history_that_fails_is_named() {
        let wallet = Wallet::create(&dir("price-alert-day"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        wallet.inner.state.lock().await.outputs.push(received("in", t - 2 * DAY, XMR));
        wallet.inner.state.lock().await.outputs.push(received("older", t - 5 * DAY, XMR));
        // The current price came; the history didn't.
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
        wallet.inner.state.lock().await.price_cache.history.clear();
        let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
        wallet.refresh_prices_from(&failing, Asked::Background, t + 60).await.unwrap_err();
        assert_eq!(failing.asked(), (0, 1), "only the history was due");
        let clock = Clock { now: t + 60, utc_offset: 0, hours_24: true };
        let toast = wallet.price_toast(clock).await.unwrap();
        let (today, _) = activity::local(t + 60, clock);
        let date = activity::date(price::day(t - 2 * DAY), today);
        assert_eq!(toast.title, format!("Couldn't get {date}'s USD price"));
        assert_eq!(toast.line, "Kraken didn't answer · shown as not available");
        // Retry finds it: the day's price is named, not the current one.
        let days = vec![(price::day(t - 2 * DAY), 160_120_000), (price::day(t - 5 * DAY), 150_000_000)];
        let worked = wallet.retry_prices_from(&FakePrices::new(166_100_000, days), clock).await;
        assert_eq!(worked, Retried::Updated { title: format!("{date}'s USD price found"), line: "1 XMR = $160.12".into() });
    }

    #[tokio::test]
    async fn retry_says_what_came_of_it() {
        let wallet = Wallet::create(&dir("price-retry"), [1; 32], Network::Stagenet).unwrap();
        let clock = clock_now();
        let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
        wallet.refresh_prices_from(&failing, Asked::Background, clock.now).await.unwrap_err();
        let shown = wallet.price_toast(clock).await.unwrap();
        // Failing again: the toast again, as a new showing.
        let Retried::Failed(again) = wallet.retry_prices_from(&failing, clock).await else { panic!("it failed") };
        assert!(again.id > shown.id);
        assert_eq!((again.retrying_title.as_str(), again.retrying_line.as_str()), ("Trying again…", "Asking Kraken for the USD price"));
        assert_eq!(failing.asked().0, 2);
        // Working: the words for 2 s, and the run is over.
        let worked = wallet.retry_prices_from(&FakePrices::new(166_100_000, vec![]), clock).await;
        assert_eq!(worked, Retried::Updated { title: "USD price updated".into(), line: "1 XMR = $166.10".into() });
        assert_eq!(wallet.price_toast(clock).await, None);
    }

    #[tokio::test]
    async fn an_old_price_keeps_amounts_with_its_age_and_one_over_a_day_turns_money_off() {
        let wallet = Wallet::create(&dir("price-age"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let at = |now| Clock { now, utc_offset: 0, hours_24: true };
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
        // Just asked: the rate.
        assert_eq!(wallet.amount_field("20", true, at(t)).await.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));
        // A refresh missed: when it's from, money still typed.
        let f = wallet.amount_field("20", true, at(t + price::AGED)).await;
        assert!(f.in_fiat && f.can_switch);
        let from = price::alert::age(t, at(t + price::AGED)).unwrap();
        assert_eq!(f.line, Some(format!("≈ 0.1204 XMR · {from}")));
        // Over a day: XMR only, saying why.
        let f = wallet.amount_field("20", true, at(t + price::SHOWN)).await;
        assert!(!f.in_fiat && !f.can_switch);
        assert_eq!(f.line.as_deref(), Some("No USD price now · type the amount in XMR"));
    }

    /// Everything converted at the shown price says how old it is once it's
    /// missed a refresh, up to the day it's shown for: Coins, Home's
    /// not-ready sums, a payment's "Now", Split up's footer and Confirm.
    #[tokio::test]
    async fn every_amount_at_an_old_price_says_how_old() {
        let wallet = Wallet::create(&dir("price-age-everywhere"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let at = |now| Clock { now, utc_offset: 0, hours_24: true };
        {
            let mut state = wallet.inner.state.lock().await;
            state.tip = 100;
            state.outputs.push(received("in", t - 3600, XMR));
            let mut waiting = received("new", t - 60, XMR / 2);
            waiting.height = 95;
            state.outputs.push(waiting);
        }
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
        // Fresh: no age anywhere.
        assert_eq!(wallet.price_age(t).await, None);
        assert_eq!(wallet.coins(at(t)).await.ready_sum, "1 · $166.10");
        assert_eq!(wallet.not_ready(at(t)).await.unwrap().price_age, None);
        assert_eq!(wallet.payment("in", at(t)).await.unwrap().rate_now.as_deref(), Some("1 XMR = $166.10"));
        let fresh = wallet.split_view(&[1], at(t)).await.footer;
        assert!(fresh.starts_with("Network fee about ") && !fresh.contains('·'), "{fresh}");
        // A second short of a day: still shown, and how old it is said.
        let late = t + price::SHOWN - 1;
        assert_eq!(wallet.price_age(late).await.as_deref(), Some("23 h old"));
        assert_eq!(wallet.coins(at(late)).await.ready_sum, "1 · $166.10 · 23 h old");
        let n = wallet.not_ready(at(late)).await.unwrap();
        assert_eq!(n.price_age.as_deref(), Some("23 h old"));
        assert!(n.all_ready_line.ends_with(" · price 23 h old"), "{}", n.all_ready_line);
        assert_eq!(wallet.payment("in", at(late)).await.unwrap().rate_now.as_deref(), Some("1 XMR = $166.10 · 23 h old"));
        let footer = wallet.split_view(&[1], at(late)).await.footer;
        assert!(footer.starts_with("Network fee about ") && footer.ends_with(" · 23 h old price"), "{footer}");
        // A day: no price, so no age.
        assert_eq!(wallet.price_age(t + price::SHOWN).await, None);
    }

    /// Receive's code and its field convert at the same price: a new price
    /// changes both, and says it's new.
    #[tokio::test]
    async fn receive_s_code_and_field_agree_after_a_new_price() {
        let wallet = Wallet::create(&dir("receive-new-price"), [1; 32], Network::Stagenet).unwrap();
        let address = wallet.receive_address().await.unwrap();
        let t = now();
        wallet.refresh_prices_from(&FakePrices::new(100_000_000, vec![]), Asked::Background, t - 20 * 3600).await.unwrap();
        let old = wallet.price_at(t).await;
        assert_eq!(wallet.receive_code(&address, Some("20"), true, None).await.unwrap().detail.as_deref(), Some("0.2 XMR"));
        wallet.refresh_prices_from(&FakePrices::new(200_000_000, vec![]), Asked::Typing, t).await.unwrap();
        assert_ne!(wallet.price_at(t).await, old, "a new price is told apart");
        let field = wallet.amount_field("20", true, clock_now()).await;
        let code = wallet.receive_code(&address, Some("20"), true, None).await.unwrap();
        assert_eq!(field.line.as_deref(), Some("≈ 0.1 XMR · 1 XMR = $200.00"));
        assert_eq!(code.detail.as_deref(), Some("0.1 XMR"));
        assert_eq!(code.uri, format!("monero:{address}?tx_amount=0.1"));
    }

    /// With no price at all the field is in XMR, and opening or typing in it
    /// asks for one (once a minute), as money typed against an old one does.
    #[tokio::test]
    async fn with_no_price_the_field_asks_for_one() {
        let wallet = Wallet::create(&dir("price-none-asks"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let at = |now| Clock { now, utc_offset: 0, hours_24: true };
        let asked = || wallet.inner.prices_asked.lock().unwrap().take();
        let f = wallet.amount_field("", false, at(t)).await;
        assert!(!f.can_switch);
        assert_eq!(asked(), Some(Asked::Typing), "opening asks");
        wallet.amount_field("0.5", false, at(t + 30)).await;
        assert_eq!(asked(), None, "not again within the minute");
        wallet.amount_field("0.55", false, at(t + 60)).await;
        assert_eq!(asked(), Some(Asked::Typing));
        // XMR only: never.
        wallet.set_currency(None).await.unwrap();
        asked();
        wallet.amount_field("1", false, at(t + 600)).await;
        assert_eq!(asked(), None);
    }

    /// The blank field a form opens with, ready before it's drawn: in money
    /// at the shown price, XMR without one, and never a lookup of its own.
    #[tokio::test]
    async fn the_blank_field_follows_the_price_and_asks_nothing() {
        let wallet = Wallet::create(&dir("price-blank"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let at = |now| Clock { now, utc_offset: 0, hours_24: true };
        let asked = || wallet.inner.prices_asked.lock().unwrap().take();
        let f = wallet.blank_amount_field(at(t)).await;
        assert!(!f.in_fiat && !f.can_switch);
        assert_eq!(asked(), None, "no price, still no lookup");
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
        let f = wallet.blank_amount_field(at(t)).await;
        assert!(f.in_fiat && f.can_switch);
        assert_eq!(f, wallet.amount_field("", true, at(t)).await, "the same as the form's own");
        // Over a day old: XMR again.
        assert!(!wallet.blank_amount_field(at(t + price::SHOWN)).await.in_fiat);
        assert_eq!(asked(), None);
    }

    #[tokio::test]
    async fn money_typed_against_an_old_price_asks_again_at_most_once_a_minute() {
        let wallet = Wallet::create(&dir("price-typed"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let at = |now| Clock { now, utc_offset: 0, hours_24: true };
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
        let asked = || wallet.inner.prices_asked.lock().unwrap().take();
        // A fresh price, XMR typed, or nothing typed: no lookup.
        wallet.amount_field("20", true, at(t + 60)).await;
        wallet.amount_field("0.5", false, at(t + price::REFRESH)).await;
        wallet.amount_field(" ", true, at(t + price::REFRESH)).await;
        assert_eq!(asked(), None);
        // Money typed against a price 5 minutes old: asked, as typing.
        wallet.amount_field("20", true, at(t + price::REFRESH)).await;
        assert_eq!(asked(), Some(Asked::Typing));
        // The next key within the minute: not again.
        wallet.amount_field("205", true, at(t + price::REFRESH + 59)).await;
        assert_eq!(asked(), None);
        wallet.amount_field("2050", true, at(t + price::REFRESH + 60)).await;
        assert_eq!(asked(), Some(Asked::Typing));
    }

    #[tokio::test]
    async fn receive_takes_an_amount_in_money_and_asks_in_xmr() {
        let wallet = Wallet::create(&dir("receive-fiat"), [1; 32], Network::Stagenet).unwrap();
        let address = wallet.receive_address().await.unwrap();
        // No price yet: XMR only.
        assert!(!wallet.amount_field("", true, clock_now()).await.can_switch);
        assert!(matches!(wallet.receive_code(&address, Some("20"), true, None).await, Err(FobError::Price(_))));
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, now()).await.unwrap();
        let field = wallet.amount_field("20", true, clock_now()).await;
        assert_eq!((field.sign.as_deref(), field.unit.as_str(), field.in_fiat), (Some("$"), "USD", true));
        // What the field says the code asks for is what it asks for.
        assert_eq!(field.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));

        let code = wallet.receive_code(&address, Some("20"), true, Some("Coffee beans")).await.unwrap();
        assert_eq!(code.uri, format!("monero:{address}?tx_amount=0.1204&tx_description=Coffee%20beans"));
        assert_eq!(code.headline.as_deref(), Some("$20.00"));
        assert_eq!(code.detail.as_deref(), Some("0.1204 XMR · Coffee beans"));
        // In XMR, as before.
        let code = wallet.receive_code(&address, Some("0.5"), false, None).await.unwrap();
        assert_eq!((code.headline.as_deref(), code.detail), (Some("0.5 XMR"), None));
        assert!(wallet.receive_code(&address, Some("20.123"), true, None).await.is_err());
        // Switching the field converts what's typed.
        assert_eq!(wallet.convert_amount("20", false).await, "0.1204");
        assert_eq!(wallet.convert_amount("0.1204", true).await, "20.00");
        assert_eq!(wallet.convert_amount("", true).await, "");
        // XMR only: no money at all.
        wallet.set_currency(None).await.unwrap();
        assert_eq!(wallet.amount_field("20", true, clock_now()).await.unit, "XMR");
        assert_eq!(wallet.convert_amount("20", false).await, "");
    }

    #[tokio::test]
    async fn send_takes_an_amount_in_money_at_the_current_price() {
        let wallet = Wallet::create(&dir("send-fiat"), [1; 32], Network::Stagenet).unwrap();
        let address = wallet.receive_address().await.unwrap();
        // No price: XMR only.
        assert_eq!(wallet.typed_amount(" 0.5 ", false).await.unwrap(), (XMR / 2, None));
        assert!(matches!(wallet.typed_amount("20", true).await, Err(FobError::Price(_))));
        assert!(matches!(wallet.prepare_typed(&address, "20", true, None, None).await, Err(FobError::Price(_))));
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, now()).await.unwrap();
        // The XMR the field's line showed for it.
        assert_eq!(wallet.amount_field("20", true, clock_now()).await.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));
        assert_eq!(wallet.typed_amount("20", true).await.unwrap(), (120_400_000_000, Some("$20.00".into())));
        assert!(matches!(wallet.typed_amount("20.123", true).await, Err(FobError::BadAmount)));
        // Converted before the address and amount are checked, as prepare checks them.
        assert!(matches!(wallet.prepare_typed("86Ux", "20", true, None, None).await, Err(FobError::BadAddress(_))));
        assert!(matches!(wallet.prepare_typed(&address, "0", true, None, None).await, Err(FobError::BadAmount)));
        assert!(matches!(wallet.prepare_typed(&address, "abc", false, None, None).await, Err(FobError::BadAmount)));
    }

    #[tokio::test]
    async fn send_s_sheet_says_what_s_ready_and_who_was_paid() {
        let wallet = Wallet::create(&dir("send-sheet"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        assert_eq!(wallet.ready_line(t).await, "0 XMR ready to spend");
        assert!(wallet.lists(clock).await.paid_before.is_empty());
        let to = "5".repeat(95);
        {
            let mut state = wallet.inner.state.lock().await;
            state.tip = 100;
            state.outputs.push(Owned { height: 20, ..received("in", t, 2 * XMR) });
            // Not ready: not counted.
            state.outputs.push(Owned { height: 95, key: "late".into(), ..received("late", t, XMR) });
            state.sent.push(Sent { tx: "out".into(), to: to.clone(), amount: XMR / 10, fee: 1, change: 0, time: t, height: None, receipt: None, note: Some("Lunch money".into()), pieces: None, back: vec![], recipient_name: None });
        }
        assert_eq!(wallet.ready_line(t).await, "2 XMR ready to spend");
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
        assert_eq!(wallet.ready_line(t).await, "2 XMR ready to spend · $332.20");
        // A price that's missed refreshes says how old it is; one over a
        // day old isn't used.
        assert_eq!(wallet.ready_line(t + 2 * 3_600).await, "2 XMR ready to spend · $332.20 · 2 h old");
        assert_eq!(wallet.worth_aged(XMR, t + price::AGED).await, Some(("$166.10".into(), Some("10 min old".into()))));
        assert_eq!(wallet.worth_aged(XMR, t + 60).await, Some(("$166.10".into(), None)));
        assert_eq!(wallet.ready_line(t + price::SHOWN).await, "2 XMR ready to spend");
        let paid = wallet.lists(clock).await.paid_before;
        assert_eq!(paid.len(), 1);
        assert_eq!((paid[0].address.as_str(), paid[0].name.as_str(), paid[0].line.as_str()), (to.as_str(), "5555 … 5555", "Lunch money · Today"));
    }

    #[tokio::test]
    async fn notes_go_with_payments_in_and_out() {
        let wallet = Wallet::create(&dir("notes"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        wallet.remember_receive_note(1, " Coffee beans ").await.unwrap();
        {
            let mut state = wallet.inner.state.lock().await;
            state.outputs.push(received("in", t, XMR / 2));
            state.outputs.push(Owned { key: "in-2".into(), output: "2".into(), ..received("in", t, XMR / 4) });
            state.sent.push(Sent { tx: "out".into(), to: "86Ux".into(), amount: 1, fee: 1, change: 0, time: t, height: None, receipt: None, note: Some("Rent".into()), pieces: None, back: vec![], recipient_name: None });
        }
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        let shown = wallet.payment("in", clock).await.unwrap();
        assert_eq!((shown.party.as_deref(), shown.pieces.as_deref()), (Some("Address 1 · Coffee beans"), Some("2 · 0.5 + 0.25")));
        assert!(shown.when.ends_with(" · Coffee beans"));
        assert!(wallet.payment("out", clock).await.unwrap().when.ends_with(" · Rent"));
        assert!(wallet.payment("nothing", clock).await.is_none());
        // A blank note forgets it.
        wallet.remember_receive_note(1, "").await.unwrap();
        assert_eq!(wallet.payment("in", clock).await.unwrap().party.as_deref(), Some("Address 1"));
    }

    #[tokio::test]
    async fn split_up_learns_from_payments_and_pays_its_own_address() {
        let wallet = Wallet::create(&dir("split"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let sent = |tx: &str, amount: u64, time: u64, pieces: Option<u32>| Sent {
            tx: tx.into(),
            to: "86Ux".into(),
            amount,
            fee: 1,
            change: 0,
            time,
            height: Some(20),
            receipt: None,
            note: None,
            pieces,
            back: vec![],
            recipient_name: None,
        };
        {
            let mut state = wallet.inner.state.lock().await;
            state.tip = 100;
            state.outputs.push(received("in", t, 2 * XMR));
            // Not ready yet: not the split's to use.
            state.outputs.push(Owned { height: 95, key: "late".into(), ..received("late", t, 5 * XMR) });
            for i in 0..4 {
                state.sent.push(sent(&format!("coffee-{i}"), XMR / 10, t - 86_400 * i, None));
            }
            // Older than 30 days, and a split: neither teaches it.
            state.sent.push(sent("old", XMR, t - 40 * 86_400, None));
            state.sent.push(sent("split", XMR, t, Some(12)));
        }
        // What Auto learns from, read straight from the wallet, is what
        // Activity shows going out in those 30 days, splits aside.
        {
            let mut state = wallet.inner.state.lock().await;
            state.outputs.push(Owned { key: "spent".into(), spent_in: Some("restored-spend".into()), ..received("old-in", t - 40 * 86_400, XMR) });
            state.outputs.push(Owned { key: "change".into(), ..received("restored-spend", t, XMR / 2) });
            state.spends.push(ChainSpend { tx: "restored-spend".into(), height: 99, time: t, fee: XMR / 100 });
        }
        let since = clock_now().now - split::LOOKBACK;
        let mut learned = Wallet::split_payments(&*wallet.inner.state.lock().await, clock_now().now);
        let mut shown: Vec<u64> = wallet
            .activity()
            .await
            .into_iter()
            .filter(|a| a.direction == Direction::Out && a.split.is_none() && a.time >= since && a.amount > 0)
            .map(|a| a.amount)
            .collect();
        learned.sort();
        shown.sort();
        assert_eq!(learned, shown);
        assert_eq!(learned, [XMR / 10, XMR / 10, XMR / 10, XMR / 10, XMR / 2 - XMR / 100]);
        {
            let mut state = wallet.inner.state.lock().await;
            state.outputs.retain(|o| o.tx != "old-in" && o.tx != "restored-spend");
            state.spends.clear();
        }
        // No price: notes in XMR. Split up opens empty, with Auto learned
        // from the four coffees.
        let opening = wallet.split_view(&[], clock_now()).await;
        assert_eq!((opening.pieces, opening.is_auto, opening.button.as_str(), opening.footer.as_str()), (0, false, "Add a coin to split", ""));
        let auto = opening.auto.expect("four payments to learn from").counts;
        assert_eq!(auto, [0, 0, 15, 0, 0, 0]);
        let view = wallet.split_view(&auto, clock_now()).await;
        assert!(view.is_auto);
        assert_eq!((view.summary.as_deref(), view.button.as_str()), (Some("15 coins · 1.5 XMR"), "Split into 15 coins"));
        assert!(view.footer.starts_with("Network fee about 0.0001"), "{}", view.footer);
        let refused = wallet.split_add(&auto, 0).await.unwrap_err();
        assert_eq!(refused.to_string(), "15 coins is the most one split can make");
        // Only the 2 XMR ready counts: a 1 XMR piece fits once, not twice.
        let one = wallet.split_add(&[], 5).await.unwrap();
        assert!(matches!(wallet.split_add(&one, 5).await, Err(FobError::Split(_))));
        assert_eq!(wallet.split_remove(&one, 5).await, [0; 6]);

        // The split's address: the next unused, kept, and never given out on Receive.
        assert_eq!(wallet.receive_index().await, 0);
        let split_address = wallet.split_address().await.unwrap();
        assert_eq!(split_address, wallet.inner.keys.address(1));
        assert_eq!(wallet.split_address().await.unwrap(), split_address);
        assert_eq!(wallet.receive_address().await.unwrap(), wallet.inner.keys.address(2));
        assert_eq!(wallet.new_receive_address().await.unwrap(), wallet.inner.keys.address(3));
        assert_eq!(Wallet::open(&wallet.inner.path, [1; 32]).unwrap().split_address().await.unwrap(), split_address);

        // Sent and not yet mined: all of it comes back, and reads as a split.
        {
            let mut state = wallet.inner.state.lock().await;
            state.sent.retain(|s| s.tx != "split");
            state.sent.push(Sent { change: 2 * XMR - 1, height: None, ..sent("split", 15 * XMR / 10, t, Some(15)) });
        }
        assert_eq!(wallet.balance().await.total, 2 * XMR + 5 * XMR + 2 * XMR - 1);
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        let pieces = wallet.not_ready(clock).await.unwrap().pieces;
        assert!(pieces.iter().any(|p| p.source.starts_with("Split up · ")));

        // Sent with its outputs known: each one shows before it's mined,
        // the same count as after, not one lump.
        {
            let mut state = wallet.inner.state.lock().await;
            state.sent.retain(|s| s.tx != "split");
            let back = vec![XMR / 10; 15].into_iter().chain([XMR / 2 - 1]).collect::<Vec<_>>();
            state.sent.push(Sent { change: 2 * XMR - 1, height: None, back, ..sent("split", 15 * XMR / 10, t, Some(15)) });
        }
        let summary = wallet.not_ready(clock).await.unwrap();
        let split: Vec<_> = summary.pieces.iter().filter(|p| p.source.starts_with("Split up · ")).collect();
        assert_eq!(split.len(), 16);
        assert_eq!(split.iter().map(|p| p.atomic).sum::<u64>(), 2 * XMR - 1);
        let row = wallet.lists(clock).await.days.into_iter().flat_map(|d| d.rows).find(|r| r.activity.as_ref().is_some_and(|a| a.tx == "split")).unwrap();
        assert_eq!((row.title.as_str(), row.amount.as_str()), ("Split up", "−0.000000000001"));
        assert!(row.line.starts_with("Sending · 15 coins · "), "{}", row.line);
    }

    #[tokio::test]
    async fn price_settings_refuse_what_fob_cant_do() {
        let wallet = Wallet::create(&dir("price-settings"), [1; 32], Network::Stagenet).unwrap();
        assert!(matches!(wallet.set_currency(Some("XYZ")).await, Err(FobError::Price(_))));
        assert_eq!(
            wallet.set_price_transport(price::Transport::Tor).await.unwrap_err().to_string(),
            "prices over Tor aren't available yet"
        );
        assert_eq!(wallet.set_currency(Some("EUR")).await.unwrap(), None);
        wallet.set_fiat_first(true).await.unwrap();
        let again = Wallet::open(&wallet.inner.path, [1; 32]).unwrap();
        let settings = again.price_settings().await;
        assert_eq!((settings.code(), settings.fiat_first, settings.transport), (Some("EUR"), true, price::Transport::Web));
    }

    #[tokio::test]
    async fn a_currency_kraken_lacks_moves_to_coingecko_and_old_records_stay() {
        let wallet = Wallet::create(&dir("price-move"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        wallet.inner.state.lock().await.outputs.push(received("in", t, XMR));
        wallet.refresh_prices_from(&FakePrices::new(166_000_000, vec![]), Asked::Background, t).await.unwrap();
        assert_eq!(price_of(&wallet.activity().await, "in"), Some((166_000_000, false)));
        let said = wallet.set_currency(Some("NZD")).await.unwrap();
        assert_eq!(said.as_deref(), Some("Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko"));
        // Kept, and the dollar price no longer shows in New Zealand dollars.
        let again = Wallet::open(&wallet.inner.path, [1; 32]).unwrap();
        let settings = again.price_settings().await;
        assert_eq!((settings.code(), settings.provider), (Some("NZD"), price::Provider::CoinGecko));
        assert_eq!(price_of(&again.activity().await, "in"), None);
        assert_eq!(again.worth_now(XMR).await, None);
        // Kraken can't be picked for it; back in dollars, the record is there.
        assert!(again.set_price_provider(price::Provider::Kraken).await.is_err());
        again.set_currency(Some("USD")).await.unwrap();
        assert_eq!(again.price_settings().await.provider, price::Provider::CoinGecko, "CoinGecko prices dollars too");
        assert_eq!(price_of(&again.activity().await, "in"), Some((166_000_000, false)));
        assert_eq!(again.set_price_provider(price::Provider::Kraken).await.unwrap(), None);
        // The list ticks what's chosen; with XMR only there's nothing to check.
        let list = again.currency_list(Some("NZD"), "").await;
        assert!(list.groups[2].rows.iter().any(|r| r.chosen && r.code.as_deref() == Some("USD")));
        again.set_currency(None).await.unwrap();
        assert!(again.check_prices().await.is_none());
        assert!(again.currency_list(None, "").await.groups[0].rows[0].chosen);
    }

    #[tokio::test]
    async fn a_wallet_file_from_before_prices_opens_with_them_on() {
        let path = dir("price-old-file");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        let state = file["state"].as_object_mut().unwrap();
        for new in ["prices", "price_cache", "price_records"] {
            assert!(state.remove(new).is_some(), "{new} is saved");
        }
        store::save(&path, &[1; 32], &file).unwrap();
        let old = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(old.price_settings().await, price::Settings::default());
    }

    #[tokio::test]
    async fn a_wallet_file_with_prices_off_opens_with_xmr_only() {
        let path = dir("price-off-file");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        // As files had them before currencies: a switch and a currency.
        file["state"]["prices"] = serde_json::json!({"on": false, "currency": "EUR", "provider": "kraken", "transport": "web", "fiat_first": true});
        store::save(&path, &[1; 32], &file).unwrap();
        let old = Wallet::open(&path, [1; 32]).unwrap();
        let settings = old.price_settings().await;
        assert_eq!((settings.code(), settings.fiat_first), (None, true));
        file["state"]["prices"] = serde_json::json!({"on": true, "currency": "EUR", "provider": "kraken", "transport": "web", "fiat_first": false});
        store::save(&path, &[1; 32], &file).unwrap();
        drop(old);
        let old = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(old.price_settings().await.code(), Some("EUR"));
    }

    /// A wallet saved with "Show first" on its currency (before the row
    /// went) keeps that order; with XMR as the default currency nothing
    /// swaps, and the order waits for a currency again.
    #[tokio::test]
    async fn an_old_file_s_currency_first_stays_and_xmr_never_swaps() {
        let path = dir("price-order-file");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        // Without fiat_first at all: XMR first.
        file["state"]["prices"] = serde_json::json!({"currency": "EUR", "provider": "kraken", "transport": "web"});
        store::save(&path, &[1; 32], &file).unwrap();
        assert!(!Wallet::open(&path, [1; 32]).unwrap().price_settings().await.fiat_first);
        file["state"]["prices"] = serde_json::json!({"currency": "EUR", "provider": "kraken", "transport": "web", "fiat_first": true});
        store::save(&path, &[1; 32], &file).unwrap();
        let old = Wallet::open(&path, [1; 32]).unwrap();
        let t = now();
        let order = |can_swap, fiat_first| price::BalanceOrder { can_swap, fiat_first };
        let balance = async |w: &Wallet| w.balance_worth(XMR, t).await;
        assert_eq!(balance(&old).await, (None, order(false, false)), "no price yet: nothing to swap to");
        old.refresh_prices_from(&FakePrices::new(166_000_000, vec![]), Asked::Background, t).await.unwrap();
        let worth = Some(("€166.00".to_string(), None));
        assert_eq!(balance(&old).await, (worth.clone(), order(true, true)), "a worth whenever it swaps");
        old.set_fiat_first(false).await.unwrap();
        assert_eq!(balance(&old).await, (worth, order(true, false)));
        old.set_fiat_first(true).await.unwrap();
        // XMR: no swap, and tapping is refused.
        old.set_currency(None).await.unwrap();
        assert_eq!(balance(&old).await, (None, order(false, false)));
        assert!(matches!(old.set_fiat_first(false).await, Err(FobError::Price(_))));
        // A currency again: its order as it was.
        old.set_currency(Some("EUR")).await.unwrap();
        assert!(Wallet::open(&path, [1; 32]).unwrap().price_settings().await.fiat_first);
    }

    #[tokio::test]
    async fn contacts_are_kept_in_the_wallet_file_and_name_payments() {
        let path = dir("contacts");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        let (sam_address, other) = (wallet.inner.keys.address(7), wallet.inner.keys.address(8));
        let photo = contacts::test_webp(400, 96);
        let sam = wallet
            .add_contact(contacts::Draft { name: "Sam".into(), note: Some("Lunch money".into()), photo: Some(photo.clone()), send_address: Some(sam_address.clone()), ..Default::default() })
            .await
            .unwrap();
        // A refused change leaves the file as it was.
        assert!(wallet.edit_contact(sam.id, contacts::Draft { name: String::new(), ..Default::default() }).await.is_err());
        {
            let mut state = wallet.inner.state.lock().await;
            state.sent.push(Sent { tx: "lunch".into(), to: sam_address.clone(), amount: XMR / 10, fee: 1, change: 0, time: t, height: None, receipt: None, note: Some("Lunch".into()), pieces: None, back: vec![], recipient_name: None });
            state.sent.push(Sent { tx: "else".into(), to: other.clone(), amount: XMR / 10, fee: 1, change: 0, time: t - 10, height: None, receipt: None, note: None, pieces: None, back: vec![], recipient_name: None });
            wallet.save_state(&state).unwrap();
        }
        let reopened = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(reopened.contact(sam.id).await, Some(sam.clone()));
        assert_eq!(reopened.contact(sam.id).await.unwrap().photo, Some(photo));
        // Activity calls the payment to Sam's address "Sam".
        let lists = reopened.lists(clock).await;
        let days = lists.days;
        let titles: Vec<_> = days[0].rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Sam", &activity::short_address(&other)]);
        assert_eq!(reopened.payment("lunch", clock).await.unwrap().party.as_deref(), Some("Sam"));
        assert_eq!(reopened.contacts("lunch", clock).await[0].line, "Lunch money · paid today");
        assert_eq!(lists.on_send[0].line, "Lunch · Today");
        assert_eq!(lists.unsaved.iter().map(|p| p.address.as_str()).collect::<Vec<_>>(), [other.as_str()]);
        assert_eq!(reopened.contact_card(sam.id, clock).await.unwrap().paid.len(), 1);
        // Deleted, the payment stays, shown by its address again.
        reopened.delete_contact(sam.id).await.unwrap();
        assert!(reopened.contact_card(sam.id, clock).await.is_none());
        let rows = Wallet::open(&path, [1; 32]).unwrap().activity().await;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.contact.is_none()));
    }

    #[tokio::test]
    async fn the_paid_screen_offers_to_save_whoever_a_payment_went_to() {
        let wallet = Wallet::create(&dir("save-offer"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let (sam_address, code, henry, plain) = (wallet.inner.keys.address(7), wallet.inner.keys.address(8), wallet.inner.keys.address(9), wallet.inner.keys.address(10));
        let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), send_address: Some(sam_address.clone()), ..Default::default() }).await.unwrap();
        let sent = |tx: &str, to: &str, time: u64, name: Option<&str>| Sent {
            tx: tx.into(),
            to: to.into(),
            amount: 1,
            fee: 1,
            change: 0,
            time,
            height: None,
            receipt: None,
            note: Some("Pizza".into()),
            pieces: None,
            back: vec![],
            recipient_name: name.map(Into::into),
        };
        {
            let mut state = wallet.inner.state.lock().await;
            state.sent.push(sent("to-sam", &sam_address, t, None));
            state.sent.push(sent("named-sam", &code, t, Some("Sam")));
            state.sent.push(sent("named-henry", &henry, t - 20, Some("Henry")));
            state.sent.push(sent("henry-again", &henry, t, None));
            state.sent.push(sent("plain", &plain, t, None));
            state.sent.push(Sent { pieces: Some(3), ..sent("split", &plain, t, None) });
        }
        assert!(wallet.save_offer("to-sam").await.is_none());
        assert!(wallet.save_offer("split").await.is_none());
        assert!(wallet.save_offer("nothing").await.is_none());
        let add = wallet.save_offer("named-sam").await.unwrap();
        assert_eq!((add.add_to.map(|c| c.id), add.button.as_str()), (Some(sam.id), "Add to Sam"));
        let plain_offer = wallet.save_offer("plain").await.unwrap();
        assert_eq!(plain_offer.draft.send_address.as_deref(), Some(plain.as_str()));
        // "Paid, not saved": an address once paid with a named code is only ever paid at.
        let henry_draft = wallet.new_contact_for(&henry).await;
        assert_eq!((henry_draft.name.as_str(), henry_draft.send_address, henry_draft.also_paid_at), ("Henry", None, vec![henry.clone()]));
        assert_eq!(wallet.new_contact_for(&plain).await.send_address.as_deref(), Some(plain.as_str()));
        // So too on Paid after paying it again without the name (from Paid before, or pasted).
        let again = wallet.save_offer("henry-again").await.unwrap();
        assert_eq!((again.draft.name.as_str(), again.draft.send_address, again.draft.also_paid_at), ("Henry", None, vec![henry.clone()]));
        assert_eq!(again.named.as_deref(), Some("to a code named “Henry”"));
        // Added to Sam, the payment is Sam's and nothing more is offered.
        wallet.add_contact_paid_at(sam.id, &add.address).await.unwrap();
        assert!(wallet.save_offer("named-sam").await.is_none());
        let rows = wallet.activity().await;
        assert_eq!(rows.iter().find(|r| r.tx == "named-sam").and_then(|r| r.contact.as_deref()), Some("Sam"));
        // A payment saved before names were kept reads without one.
        let mut old = serde_json::to_value(sent("old", &plain, t, Some("x"))).unwrap();
        old.as_object_mut().unwrap().remove("recipient_name");
        assert_eq!(serde_json::from_value::<Sent>(old).unwrap().recipient_name, None);
    }

    #[tokio::test]
    async fn a_request_is_made_only_on_share_each_on_an_address_no_one_had() {
        let path = dir("requests");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let address = |i: u32| wallet.inner.keys.address(i);
        // Opening Receive and building its code makes nothing.
        let own = wallet.receive_address().await.unwrap();
        assert_eq!(own, address(1));
        wallet.receive_code(&own, Some("0.5"), false, Some("Bike")).await.unwrap();
        assert!(wallet.inner.state.lock().await.requests.live().next().is_none());
        // Splits have the next one; a request skips it.
        assert_eq!(wallet.split_address().await.unwrap(), address(2));
        let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), ..Default::default() }).await.unwrap();
        let (first, code) = wallet.make_request(Some(sam.id), Some("0.5"), false, Some(" Concert tickets ")).await.unwrap();
        assert_eq!((first.subaddress, first.address.as_str(), first.contact), (3, address(3).as_str(), Some(sam.id)));
        assert_eq!(code.uri, format!("monero:{}?tx_amount=0.5&tx_description=Concert%20tickets", address(3)));
        assert_eq!((code.headline.as_deref(), code.detail.as_deref()), (Some("0.5 XMR"), Some("Concert tickets")));
        // Receive keeps its own; its "New address" never gives a request's.
        assert_eq!((wallet.receive_address().await.unwrap(), wallet.receive_index().await), (own.clone(), 1));
        assert_eq!(wallet.new_receive_address().await.unwrap(), address(4));
        let (second, _) = wallet.make_request(None, None, false, None).await.unwrap();
        assert_eq!((second.subaddress, second.contact, second.note, second.asked), (5, None, None, None));
        assert_eq!(wallet.receive_address().await.unwrap(), address(4));
        // A deleted contact can't be asked; nothing is made.
        assert!(wallet.make_request(Some(99), None, false, None).await.is_err());
        assert!(wallet.make_request(None, Some("abc"), false, None).await.is_err());
        assert_eq!(wallet.inner.state.lock().await.requests.live().count(), 2);
        // Kept in the file, as made.
        let again = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(again.receive_address().await.unwrap(), address(4));
        assert_eq!(again.request_code(first.id).await.unwrap(), code);
        // The same ask again while nothing came to it (Share, then Copy; a second tap): the same request.
        let (same, same_code) = again.make_request(None, None, false, None).await.unwrap();
        assert_eq!((same.id, same.subaddress), (second.id, 5));
        assert_eq!(same_code.uri, format!("monero:{}", address(5)));
        // Once something came to it, a new one.
        again.inner.state.lock().await.outputs.push(Owned { subaddress: 5, ..received("paid", now(), 1) });
        assert_eq!(again.make_request(None, None, false, None).await.unwrap().0.subaddress, 6);
        assert_eq!(again.make_request(None, None, false, Some("Bike")).await.unwrap().0.subaddress, 7);
        assert_eq!(again.inner.state.lock().await.requests.live().count(), 4);
    }

    #[tokio::test]
    async fn a_request_asked_in_money_keeps_what_it_came_to_then() {
        let wallet = Wallet::create(&dir("request-fiat"), [1; 32], Network::Stagenet).unwrap();
        assert!(matches!(wallet.make_request(None, Some("80"), true, None).await, Err(FobError::Price(_))));
        wallet.refresh_prices_from(&FakePrices::new(166_060_000, vec![]), Asked::Background, now()).await.unwrap();
        let (request, code) = wallet.make_request(None, Some("80"), true, Some("Concert tickets")).await.unwrap();
        let asked = request.asked.clone().unwrap();
        assert_eq!((asked.typed.as_str(), asked.unit.as_str(), asked.xmr, asked.fiat.as_deref()), ("80", "USD", 481_750_000_000, Some("$80.00")));
        assert_eq!((code.headline.as_deref(), code.detail.as_deref()), (Some("$80.00"), Some("0.48175 XMR · Concert tickets")));
        // A new price changes nothing about it.
        wallet.refresh_prices_from(&FakePrices::new(200_000_000, vec![]), Asked::User, now() + 120).await.unwrap();
        assert_eq!(wallet.request_code(request.id).await.unwrap(), code);
        assert_eq!(wallet.request(request.id, Clock { now: now(), utc_offset: 0, hours_24: true }).await.unwrap().headline, "$80.00");
    }

    #[tokio::test]
    async fn a_request_shows_what_came_and_once_deleted_its_address_still_receives_unnamed() {
        let path = dir("request-delete");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), ..Default::default() }).await.unwrap();
        let (request, _) = wallet.make_request(Some(sam.id), Some("0.5"), false, Some("Pizza")).await.unwrap();
        let (other, _) = wallet.make_request(Some(sam.id), None, false, None).await.unwrap();
        assert_eq!(wallet.request_contacts().await[0].line, "2 requests waiting");
        let to_request = |tx: &str, time: u64, amount: u64| Owned { subaddress: request.subaddress, ..received(tx, time, amount) };
        wallet.inner.state.lock().await.outputs.push(to_request("first", t - 60, XMR / 4));
        // One still in the pool counts too.
        wallet.pool_found([("pooled".into(), Some(request.subaddress), XMR / 10)], t - 30);
        let detail = wallet.request(request.id, clock).await.unwrap();
        assert_eq!((detail.received.as_str(), detail.payments.len()), ("0.35 XMR", 2));
        assert!(detail.payments.iter().all(|p| p.title == "From Sam"), "{:?}", detail.payments);
        assert_eq!(wallet.request_contacts().await[0].line, "1 request paid, 1 waiting");
        let days = wallet.lists(clock).await.days;
        let today: Vec<_> = days[0].rows.iter().map(|r| (r.title.clone(), r.request)).collect();
        assert!(today.contains(&("Request to Sam".into(), Some(request.id))), "{today:?}");
        assert!(today.contains(&("From Sam".into(), None)), "{today:?}");
        let card = wallet.contact_card(sam.id, clock).await.unwrap();
        assert_eq!((card.requests.len(), card.paid.len(), card.paid_title.as_str()), (2, 2, "Payments with Sam"));

        // Deleted: gone from Activity and Sam's card; what came before is still Sam's, the payment still in the
        // pool too, even mined in a block whose time is after the delete.
        wallet.delete_request(request.id).await.unwrap();
        wallet.delete_request(request.id).await.unwrap();
        wallet.inner.pool.lock().unwrap().clear();
        wallet.inner.state.lock().await.outputs.push(to_request("pooled", t + 600, XMR / 10));
        assert_eq!(wallet.activity().await.iter().find(|r| r.tx == "pooled").and_then(|r| r.contact.clone()), Some("Sam".into()));
        assert!(wallet.request(request.id, clock).await.is_none());
        assert!(wallet.request_code(request.id).await.is_none());
        wallet.inner.state.lock().await.outputs.push(to_request("later", t + 3600, XMR));
        let reopened = Wallet::open(&path, [1; 32]).unwrap();
        reopened.inner.state.lock().await.outputs.push(to_request("later", t + 3600, XMR));
        let rows = reopened.activity().await;
        let from = |tx: &str| rows.iter().find(|r| r.tx == tx).map(|r| (r.contact.clone(), r.note.clone()));
        assert_eq!(from("first"), Some((Some("Sam".into()), Some("Pizza".into()))));
        assert_eq!(from("later"), Some((None, Some("Pizza".into()))));
        let days = reopened.lists(Clock { now: t + 3600, ..clock }).await.days;
        assert!(days.iter().flat_map(|d| &d.rows).all(|r| r.request != Some(request.id)));
        let card = reopened.contact_card(sam.id, clock).await.unwrap();
        assert_eq!(card.requests.iter().map(|r| r.id).collect::<Vec<_>>(), [other.id]);
        assert_eq!(card.paid.iter().map(|p| p.activity.tx.as_str()).collect::<Vec<_>>(), ["first"]);
        // Its address is still scanned, and never handed out again.
        let state = reopened.inner.state.lock().await;
        assert!(state.receive_index >= request.subaddress);
        drop(state);
        assert_ne!(reopened.new_receive_address().await.unwrap(), request.address);
    }

    #[tokio::test]
    async fn one_transaction_paying_two_requests_credits_each_its_own_part() {
        let wallet = Wallet::create(&dir("request-parts"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), ..Default::default() }).await.unwrap();
        let jo = wallet.add_contact(contacts::Draft { name: "Jo".into(), ..Default::default() }).await.unwrap();
        let (for_sam, _) = wallet.make_request(Some(sam.id), None, false, Some("Rent")).await.unwrap();
        let (for_jo, _) = wallet.make_request(Some(jo.id), None, false, Some("Bills")).await.unwrap();
        let received_by = |id: u64| {
            let wallet = wallet.clone();
            async move { wallet.request(id, clock).await.unwrap().received }
        };
        // Seen in the pool, the way a pool scan keeps it: a batch payout to both, and one to the main address in it.
        wallet.pool_found(
            [("batch".into(), Some(for_sam.subaddress), XMR / 2), ("batch".into(), None, 7), ("batch".into(), Some(for_jo.subaddress), XMR / 4)],
            t - 60,
        );
        assert_eq!(received_by(for_sam.id).await, "0.5 XMR");
        assert_eq!(received_by(for_jo.id).await, "0.25 XMR");
        let row = wallet.activity().await.into_iter().find(|r| r.tx == "batch").unwrap();
        assert_eq!((row.amount, row.parts.len()), (XMR / 2 + XMR / 4 + 7, 3));
        assert!(wallet.inner.state.lock().await.requests.at(0).is_none(), "the main address is no request's");
        // Mined: the same, from its outputs.
        wallet.inner.pool.lock().unwrap().clear();
        {
            let mut state = wallet.inner.state.lock().await;
            state.outputs.push(Owned { subaddress: for_sam.subaddress, key: "a".into(), ..received("batch", t, XMR / 2) });
            state.outputs.push(Owned { subaddress: for_jo.subaddress, key: "b".into(), ..received("batch", t, XMR / 4) });
        }
        assert_eq!(received_by(for_sam.id).await, "0.5 XMR");
        assert_eq!(received_by(for_jo.id).await, "0.25 XMR");
        let detail = wallet.request(for_jo.id, clock).await.unwrap();
        assert_eq!(detail.payments.iter().map(|p| p.amount.as_str()).collect::<Vec<_>>(), ["+0.25"]);
        assert_eq!(wallet.request_contacts().await.iter().map(|c| c.line.as_str()).collect::<Vec<_>>(), ["1 request paid", "1 request paid"]);
    }

    #[tokio::test]
    async fn a_restored_wallet_hands_out_past_the_other_phone_s_addresses_and_watches_far_enough() {
        let words = Wallet::create(&dir("restore-from"), [1; 32], Network::Stagenet).unwrap().seed_words().unwrap().1;
        let restored = Wallet::restore_words(&dir("restore-to"), [1; 32], Network::Stagenet, &words, Some(0)).unwrap();
        // The other phone was paid at its request on 60, found so far; it may have handed out more, unpaid.
        restored.inner.state.lock().await.outputs.push(Owned { subaddress: 60, ..received("old", 1, XMR) });
        restored.inner.state.lock().await.receive_index = 60;
        let (first, _) = restored.make_request(None, None, false, Some("Rent")).await.unwrap();
        assert_eq!(first.subaddress, 60 + RESTORE_SKIP + 1);
        let (share_free, _) = restored.make_request(None, None, false, Some("Bills")).await.unwrap();
        assert_eq!(share_free.subaddress, first.subaddress + 1);
        // Scanning on finds another of the other phone's, paid at 80: the next new one goes past it too.
        restored.inner.state.lock().await.outputs.push(Owned { subaddress: 80, ..received("old-80", 1, XMR) });
        let (after, _) = restored.make_request(None, None, false, Some("Gas")).await.unwrap();
        assert_eq!(after.subaddress, 80 + RESTORE_SKIP + 1);
        // A payment to one of the other phone's old request addresses (say 70) is no new request's.
        restored.inner.state.lock().await.outputs.push(Owned { subaddress: 70, ..received("to-old-request", 2, XMR) });
        let row = restored.activity().await.into_iter().find(|r| r.tx == "to-old-request").unwrap();
        assert_eq!((row.note, row.contact), (None, None));
        // Paid at what this wallet handed out: no further jump.
        restored.inner.state.lock().await.outputs.push(Owned { subaddress: after.subaddress, ..received("mine", 3, XMR) });
        assert_eq!(restored.make_request(None, None, false, Some("Water")).await.unwrap().0.subaddress, after.subaddress + 1);
        // Nothing found: new ones start past the skip anyway.
        let empty = Wallet::restore_words(&dir("restore-empty"), [1; 32], Network::Stagenet, &words, Some(0)).unwrap();
        assert_eq!(empty.make_request(None, None, false, None).await.unwrap().0.subaddress, RESTORE_SKIP + 1);
        // A wallet made here skips nothing.
        let made = Wallet::create(&dir("restore-none"), [1; 32], Network::Stagenet).unwrap();
        assert_eq!(made.make_request(None, None, false, None).await.unwrap().0.subaddress, 1);
        // Scanning watches 200 past the highest handed out or paid: 150 unpaid requests or shares in a row past the
        // last paid one are still found by a restore, and a payment found moves the edge on.
        assert_eq!(Wallet::watched(60), 260);
        assert!(Wallet::watched(0) >= 150);
        // A file from before restores were marked reads as a wallet made here.
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&made.inner.keys), state: &*made.inner.state.lock().await }).unwrap();
        file["state"].as_object_mut().unwrap().remove("restored");
        assert!(!serde_json::from_value::<FileIn>(file).unwrap().state.restored);
    }

    /// Reading Receive's address before Receive opens (the screens' refresh,
    /// as a restore starts and before any block is scanned) hands nothing
    /// out and saves nothing: the scan still finds the other phone's
    /// payments past the skip as its own, and the address Receive then
    /// hands out goes past them.
    #[tokio::test]
    async fn reading_a_restored_wallet_s_receive_address_hands_nothing_out() {
        let words = Wallet::create(&dir("restore-read-from"), [1; 32], Network::Stagenet).unwrap().seed_words().unwrap().1;
        let path = dir("restore-read-to");
        let restored = Wallet::restore_words(&path, [1; 32], Network::Stagenet, &words, Some(0)).unwrap();
        let marks = |s: &State| (s.receive_index, s.receive_own, s.fresh_from);
        let before = marks(&*restored.inner.state.lock().await);
        let file = std::fs::read(&path).unwrap();
        assert_eq!(restored.shown_receive_address().await, None, "Receive hands one out as it opens");
        assert_eq!(marks(&*restored.inner.state.lock().await), before);
        assert_eq!(std::fs::read(&path).unwrap(), file, "nothing saved");
        // The scan finds the other phone's payment at 60 (past the skip of 50): still not one to show.
        {
            let mut state = restored.inner.state.lock().await;
            state.outputs.push(Owned { subaddress: 60, ..received("theirs", 1, XMR) });
            state.receive_index = 60;
        }
        assert_eq!(restored.shown_receive_address().await, None, "60 is the other phone's");
        // Receive opens: past what they handed out after it.
        let own = 60 + RESTORE_SKIP + 1;
        assert_eq!(restored.receive_address().await.unwrap(), restored.inner.keys.address(own));
        assert_eq!(restored.shown_receive_address().await, Some((restored.inner.keys.address(own), own)));
        assert_eq!(restored.make_request(None, None, false, None).await.unwrap().0.subaddress, own + 1);
        // A wallet made here reads none until Receive first opens, then that one.
        let made = Wallet::create(&dir("restore-read-made"), [1; 32], Network::Stagenet).unwrap();
        assert_eq!(made.shown_receive_address().await, None);
        let first = made.receive_address().await.unwrap();
        assert_eq!(made.shown_receive_address().await, Some((first, 1)));
    }

    fn outgoing(to: &str, inputs: Vec<usize>) -> Outgoing {
        Outgoing {
            to: to.into(),
            amount: XMR / 10,
            inputs,
            input_total: XMR,
            receipt: None,
            note: Some("Lunch".into()),
            recipient_name: Some("Sam".into()),
            pieces: None,
            back: vec![],
        }
    }

    #[tokio::test]
    async fn a_payment_published_is_recorded_even_if_its_caller_goes_away() {
        let path = dir("send-dropped");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        wallet.inner.state.lock().await.outputs.push(received("coin", 1, XMR));
        let to = wallet.inner.keys.address(9);
        // The node takes it only after Confirm has gone (Back pressed while "Sending…", say).
        let (published, node) = tokio::sync::oneshot::channel::<()>();
        let publish = async move {
            node.await.ok();
            Ok(())
        };
        let call = wallet.publish_and_record("hash".into(), 30, outgoing(&to, vec![0]), publish);
        assert!(tokio::time::timeout(Duration::from_millis(20), call).await.is_err(), "still publishing when dropped");
        published.send(()).unwrap();
        for _ in 0..100 {
            if !wallet.inner.state.lock().await.sent.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let reopened = Wallet::open(&path, [1; 32]).unwrap();
        let state = reopened.inner.state.lock().await;
        let sent = state.sent.iter().find(|s| s.tx == "hash").expect("recorded, and saved");
        assert_eq!((sent.note.as_deref(), sent.recipient_name.as_deref(), sent.to.as_str()), (Some("Lunch"), Some("Sam"), to.as_str()));
        assert_eq!(state.outputs[0].spent_in.as_deref(), Some("hash"));
        drop(state);
        // Its inputs spent, another payment from them is refused before anything is published.
        let never = async { panic!("not published") };
        assert!(matches!(wallet.publish_and_record("again".into(), 30, outgoing(&to, vec![0]), never).await, Err(FobError::NoSuchPayment)));
        // A node refusing it records nothing.
        wallet.inner.state.lock().await.outputs.push(received("coin-2", 1, XMR));
        let refused = async { Err(FobError::Node("rejected".into())) };
        assert!(wallet.publish_and_record("refused".into(), 30, outgoing(&to, vec![1]), refused).await.is_err());
        assert!(wallet.inner.state.lock().await.sent.iter().all(|s| s.tx != "refused"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_prepared_payment_left_behind_is_forgotten() {
        let mut pending = Pending::default();
        pending.insert(1, "left behind");
        tokio::time::advance(PREPARED_FOR - Duration::from_secs(60)).await;
        pending.insert(2, "fresh");
        assert_eq!(pending.take(2), Some("fresh"));
        assert_eq!(pending.take(2), None, "sent or cancelled once");
        pending.insert(3, "newer");
        tokio::time::advance(Duration::from_secs(61)).await;
        assert_eq!(pending.take(1), None, "too old to send");
        assert!(pending.items.len() == 1 && pending.items.contains_key(&3), "pruned as others come and go");
        assert_eq!(pending.take(3), Some("newer"));
    }

    #[tokio::test]
    async fn a_new_wallet_asks_to_be_backed_up_until_its_words_are_written_down() {
        let path = dir("backup");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        assert_eq!(wallet.backup_reminder().await, Some(keys::reminder(16)));
        // Fob closed during the backup step: opened again, it still asks.
        let reopened = Wallet::open(&path, [1; 32]).unwrap();
        assert!(reopened.backup_reminder().await.is_some());
        reopened.backup_done().await.unwrap();
        reopened.backup_done().await.unwrap();
        assert!(Wallet::open(&path, [1; 32]).unwrap().backup_reminder().await.is_none());
        // Restored ones, and files from before the reminder, never ask.
        let words = wallet.seed_words().unwrap().1;
        assert!(Wallet::restore_words(&dir("backup-restored"), [1; 32], Network::Stagenet, &words, Some(0)).unwrap().backup_reminder().await.is_none());
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        file["state"].as_object_mut().unwrap().remove("backup_pending");
        assert!(!serde_json::from_value::<FileIn>(file).unwrap().state.backup_pending);
    }

    #[tokio::test]
    async fn backup_and_keys_says_whether_the_words_are_written_down_and_shows_the_restore_height() {
        let path = dir("hub");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        assert_eq!(wallet.backup_hub(true).await.backup[0].line, "16 words that restore everything · not written down yet");
        wallet.backup_done().await.unwrap();
        let hub = Wallet::open(&path, [1; 32]).unwrap().backup_hub(true).await;
        assert_eq!(hub.backup[0].line, "16 words that restore everything · backed up");
        assert_eq!(hub.settings_line, "Seed phrase, wallet code, keys");
        // Restored from 25 words: no word on whether they're written down.
        let classic = keys::Keys::from_spend_key(Network::Stagenet, &wallet.inner.keys.spend_key_hex().unwrap(), None).unwrap();
        let words = classic.seed_words().unwrap().1;
        let restored = Wallet::restore_words(&dir("hub-restored"), [1; 32], Network::Stagenet, &words, Some(2_200_000)).unwrap();
        assert_eq!(restored.backup_hub(true).await.backup[0].line, "25 words that restore everything");
        let page = restored.keys_page().await;
        let height = &page[1].keys[1];
        assert_eq!(height.title, "Restore height");
        assert_eq!(*height.value, format!("2200000 · {}", restore_date(Network::Stagenet, 2_200_000).unwrap()));
        assert_eq!(page[0].keys[0].value, wallet.inner.keys.spend_key_hex().unwrap());
        assert_eq!(*page[1].keys[0].value, wallet.primary_address());
        // A view-only wallet: Wallet code and Keys, and Keys without the
        // secret spend key.
        let watch = Wallet::restore_view_only(&dir("hub-watch"), [1; 32], Network::Stagenet, &wallet.primary_address(), &wallet.inner.keys.view_key_hex(), Some(0)).unwrap();
        let hub = watch.backup_hub(true).await;
        assert_eq!(hub.backup.iter().map(|r| r.item).collect::<Vec<_>>(), [keys::HubItem::WalletCode]);
        assert_eq!(hub.settings_line, "Wallet code, keys");
        let page = watch.keys_page().await;
        assert_eq!(page[0].keys.iter().map(|k| k.title.as_str()).collect::<Vec<_>>(), ["Secret view key"]);
        assert_eq!(*page[1].keys[1].value, "0");
    }

    #[test]
    fn the_restore_height_is_dated_only_when_the_date_can_be_named() {
        let near = Network::Mainnet.height_at(1_788_307_200); // 2 Sep 2026, 00:00 UTC
        assert_eq!(restore_date(Network::Mainnet, near).as_deref(), Some("2 Sep 2026"));
        assert_eq!(restore_date(Network::Mainnet, 1_009_827), None);
        assert_eq!(restore_date(Network::Stagenet, 0), None);
    }

    #[tokio::test]
    async fn a_wallet_file_from_before_requests_opens_and_keeps_receive_s_address() {
        let path = dir("requests-old-file");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        wallet.new_receive_address().await.unwrap();
        let shown = wallet.new_receive_address().await.unwrap();
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        let state = file["state"].as_object_mut().unwrap();
        assert!(state.remove("requests").is_some(), "requests are saved");
        assert!(state.remove("receive_own").is_some(), "Receive's address is saved");
        store::save(&path, &[1; 32], &file).unwrap();
        let old = Wallet::open(&path, [1; 32]).unwrap();
        // Before requests, Receive showed the last one handed out.
        let last = old.receive_address().await.unwrap();
        assert_eq!(last, shown);
        assert_eq!(last, old.inner.keys.address(2));
        let (request, _) = old.make_request(None, None, false, None).await.unwrap();
        assert_eq!(request.subaddress, 3);
        assert_eq!(old.receive_address().await.unwrap(), last);
    }

    #[tokio::test]
    async fn a_wallet_file_from_before_contacts_opens_with_none() {
        let path = dir("contacts-old-file");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        assert!(file["state"].as_object_mut().unwrap().remove("contacts").is_some(), "contacts are saved");
        store::save(&path, &[1; 32], &file).unwrap();
        let old = Wallet::open(&path, [1; 32]).unwrap();
        let clock = Clock { now: now(), utc_offset: 0, hours_24: true };
        assert!(old.contacts("", clock).await.is_empty());
        assert_eq!(old.add_contact(contacts::Draft { name: "Mum".into(), ..Default::default() }).await.unwrap().id, 1);
    }

    /// A node with a self-signed certificate is switched to only when
    /// marked to accept it, and the mark is kept in the wallet file (off in
    /// a file from before it), with the node's checks and Settings' line.
    #[tokio::test]
    async fn a_self_signed_node_is_kept_with_its_mark() {
        let path = dir("self-signed");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        // From before the mark: opens, unmarked.
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        assert!(file["state"].as_object_mut().unwrap().remove("node_self_signed").is_some(), "the mark is saved");
        store::save(&path, &[1; 32], &file).unwrap();
        let wallet = Wallet::open(&path, [1; 32]).unwrap();
        assert!(!wallet.node().await.self_signed);
        assert_eq!(wallet.node_line().await, "stagenet.xmr-tw.org:38081");

        let port = crate::node::tests::canned_node(true).await;
        let url = format!("https://127.0.0.1:{port}");
        let refused = wallet.set_node(&url, false).await.unwrap_err();
        assert_eq!(refused.to_string(), "the node didn't answer: self-signed, not accepted");
        wallet.set_node(&url, true).await.unwrap();
        let reopened = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(reopened.node().await, node::Setting { url: url.clone(), self_signed: true });

        // Checking the node in use names it on Settings' line.
        let report = reopened.check_node(&reopened.node().await, &|_| {}).await;
        assert_eq!(report.rows[0].result, "self-signed, accepted");
        assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
        // Syncing failing since, the line says so, as Home's chip, over the
        // check (and over having synced before).
        reopened.set_status(|s| s.synced = true);
        let offline = FobError::Node(format!("{:?}", InterfaceError::InterfaceError("dns error: failed to lookup address".into())));
        reopened.report(&Quiet, &mut catch_up::Pace::default(), Some(&offline)).await;
        assert_eq!(reopened.status().error.as_deref(), Some("Address not found"));
        assert_eq!(reopened.node_line().await, format!("Can't reach node · 127.0.0.1:{port}"));
        // A check after the failure is newer: it's what the line says.
        reopened.check_node(&reopened.node().await, &|_| {}).await;
        assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
        reopened.set_status(|s| s.error = None);
        assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
        // Failing to save the wallet isn't the node's doing.
        reopened.report(&Quiet, &mut catch_up::Pace::default(), Some(&FobError::Storage("disk full".into()))).await;
        assert!(reopened.status().error_local);
        assert_eq!(reopened.node_line().await, format!("Doesn't send blocks · 127.0.0.1:{port}"));
        *reopened.inner.node_verdict.lock().unwrap() = None;
        reopened.set_status(|s| s.synced = true);
        assert_eq!(reopened.node_line().await, format!("Working · 127.0.0.1:{port}"));
        // The list leaves out the node in use, and an added one comes once.
        let added = node::Setting { url: "node.example:38081/".into(), self_signed: true };
        let others = reopened.other_nodes(&[added.clone(), added]).await;
        assert_eq!(others.len(), Network::Stagenet.default_nodes().len() + 1);
        assert_eq!(others.last(), Some(&node::Setting { url: "http://node.example:38081".into(), self_signed: true }));
    }

    /// Switching to a working node while syncing fails stops the chip and
    /// Node line saying the old node's failure at once, and the sync loop
    /// tries the new node then, not at the end of its backoff.
    #[tokio::test]
    async fn switching_node_clears_the_old_failure_and_tries_the_new_one_at_once() {
        struct Heard(std::sync::Mutex<Vec<Option<String>>>);
        impl SyncListener for Heard {
            fn on_status(&self, status: SyncStatus) {
                self.0.lock().unwrap().push(status.error);
            }
        }
        let wallet = unreachable("switch-clears").await;
        let heard = Arc::new(Heard(std::sync::Mutex::new(vec![])));
        let mut rounds = wallet.inner.rounds.subscribe();
        wallet.start_sync(heard.clone());
        tokio::time::timeout(Duration::from_secs(5), rounds.changed()).await.unwrap().unwrap();
        assert_eq!(wallet.status().error.as_deref(), Some("Refused to connect"));
        assert_eq!(wallet.status().chip().text, "Can't reach node");
        let port = crate::node::tests::canned_node(false).await;
        rounds.mark_unchanged();
        let before = heard.0.lock().unwrap().len();
        wallet.set_node(&format!("127.0.0.1:{port}"), false).await.unwrap();
        // Told at once (the woken loop may already be reporting the new node).
        assert_eq!(heard.0.lock().unwrap().get(before), Some(&None));
        // The loop's 2 s backoff is cut short: a round against the new node now.
        tokio::time::timeout(Duration::from_millis(1500), rounds.changed()).await.expect("a round within the backoff").unwrap();
        wallet.stop_sync();
    }

    /// The connection kept is for the node and its mark: one made while the
    /// mark was on is never used once it's off, whatever order a switch
    /// and a connection being made happen in.
    #[tokio::test]
    async fn a_connection_is_kept_for_the_node_and_its_mark() {
        let wallet = Wallet::create(&dir("connection-mark"), [1; 32], Network::Stagenet).unwrap();
        let port = crate::node::tests::canned_node(true).await;
        {
            let mut state = wallet.inner.state.lock().await;
            state.node = format!("https://127.0.0.1:{port}");
            state.node_self_signed = true;
        }
        let (marked, ..) = wallet.connection().await.unwrap();
        assert!(marked.requests() > 0, "made and used");
        assert_eq!(wallet.connection().await.unwrap().0.requests(), marked.requests(), "kept while nothing changed");
        // The mark goes off without set_node dropping the connection (its
        // save landing between a connection's read of the setting and its
        // keeping one): a new connection, with the usual check.
        wallet.inner.state.lock().await.node_self_signed = false;
        let err = wallet.connection().await.err().expect("the usual check refuses it");
        assert!(format!("{err}").contains("certificate"), "{err}");
    }

    /// Fob has no Tor: an onion node is refused when chosen, and one
    /// already in a wallet file is never looked up or connected to.
    #[tokio::test]
    async fn an_onion_node_is_refused_and_never_reached() {
        let wallet = Wallet::create(&dir("onion"), [1; 32], Network::Stagenet).unwrap();
        let before = wallet.node().await;
        let refused = wallet.set_node("xmrnode.onion:38081", false).await.unwrap_err();
        assert!(matches!(refused, FobError::BadEndpoint(_)));
        assert_eq!(refused.to_string(), "Fob can't reach .onion nodes yet");
        assert_eq!(wallet.node().await, before, "unchanged");
        // An older file with one: syncing fails on it, saying why.
        wallet.inner.state.lock().await.node = "http://xmrnode.onion:38081".into();
        let failed = wallet.sync_step().await.unwrap_err().to_string();
        assert!(failed.contains("Fob can't reach .onion nodes yet"), "{failed}");
        // And instant updates can't point at it either.
        assert!(matches!(wallet.set_zmq(true, Some("tcp://xmrnode.onion:38083")).await, Err(FobError::BadEndpoint(e)) if e == crate::node::NO_ONION));
    }

    #[tokio::test]
    async fn sync_now_without_the_loop_runs_one_round_itself() {
        let wallet = unreachable("sync-now-alone").await;
        let started = Instant::now();
        wallet.sync_now().await;
        assert!(started.elapsed() < SYNC_NOW_WAIT);
    }

    fn webp(len: usize) -> Vec<u8> {
        crate::contacts::test_webp(len, 96)
    }

    #[tokio::test]
    async fn each_share_of_the_profile_is_a_new_address_and_its_own_line() {
        let path = dir("profile-shares");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let clock = Clock { now: t, utc_offset: 0, hours_24: true };
        let address = |i: u32| wallet.inner.keys.address(i);
        assert!(wallet.share_profile().await.is_err(), "no profile yet");
        assert_eq!(wallet.profile_row().await.title, "Add a shareable Fob contact");
        wallet.set_profile("Henry", Some(webp(300))).await.unwrap();
        let own = wallet.receive_address().await.unwrap();
        let (first, code) = wallet.share_profile().await.unwrap();
        let (second, _) = wallet.share_profile().await.unwrap();
        assert_eq!((first.subaddress, second.subaddress), (2, 3));
        assert_eq!(code, uri::profile_uri(&address(2), "Henry", Some(&webp(300))));
        // Receive keeps its own address, and New address never gives a share's.
        assert_eq!(wallet.receive_address().await.unwrap(), own);
        assert_eq!(wallet.new_receive_address().await.unwrap(), address(4));
        wallet.share_went_out(first.id, profile::ShareKind::LinkCopied).await.unwrap();

        // A payment to a share's address comes "Via your profile".
        wallet.inner.state.lock().await.outputs.push(Owned { subaddress: 2, ..received("via", t - 60, XMR / 4) });
        let days = wallet.lists(clock).await.days;
        let rows: Vec<_> = days[0].rows.iter().map(|r| (r.title.as_str(), r.share, r.amount.as_str())).collect();
        assert_eq!(rows.iter().filter(|r| r.0 == "Shared your profile").count(), 2, "{rows:?}");
        let via = days[0].rows.iter().find(|r| r.title == "Via your profile").unwrap();
        let shared = activity::time_of_day(activity::local(first.created, clock).1, clock);
        assert!(via.line.starts_with(&format!("Shared {shared} · ")), "{}", via.line);
        let detail = wallet.payment("via", clock).await.unwrap();
        assert_eq!(detail.party.as_deref(), Some("Address 2 · via your profile"));
        let lines = wallet.profile_shares(clock).await;
        assert_eq!(lines.iter().map(|l| (l.title.as_str(), l.received.as_str())).collect::<Vec<_>>(), [("Code shown", "nothing yet"), ("Link copied", "+0.25 XMR")]);

        // Kept in the file; a share's code again is the profile as it is now.
        let again = Wallet::open(&path, [1; 32]).unwrap();
        again.set_profile("Hen", None).await.unwrap();
        assert_eq!(again.share_code(first.id).await.unwrap().1, uri::profile_uri(&address(2), "Hen", None));
        assert_eq!(again.share_profile().await.unwrap().0.subaddress, 5);
        assert!(again.share_code(99).await.is_none());
    }

    /// One touch between two phones' swaps: `reader` reads `card`.
    fn touch(reader: &swap::Session, card: &swap::Session) -> swap::Step {
        let mut apdu = reader.reader_start().expect("still swapping");
        loop {
            match reader.reader_next(&card.card_answer(&apdu)) {
                swap::Step::Send(next) => apdu = next,
                step => return step,
            }
        }
    }

    /// Share my profile opened on `wallet`, and its swap.
    async fn share_and_swap(wallet: &Wallet) -> (profile::Share, Arc<swap::Session>) {
        let (share, _) = wallet.share_profile().await.unwrap();
        let session = wallet.start_swap(share.id).await.unwrap();
        (share, session)
    }

    #[tokio::test]
    async fn touching_phones_swaps_profiles_on_the_share_s_own_address() {
        let path = dir("swap-henry");
        let henry = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let sam = Wallet::create(&dir("swap-sam"), [2; 32], Network::Stagenet).unwrap();
        let clock = Clock { now: now(), utc_offset: 0, hours_24: true };
        assert!(henry.start_swap(1).await.is_err(), "no profile yet");
        henry.set_profile("Henry", Some(webp(1200))).await.unwrap();
        sam.set_profile("Sam", None).await.unwrap();
        assert!(henry.start_swap(99).await.is_err(), "no such share");
        let own = henry.receive_address().await.unwrap();

        let (shown, ours) = share_and_swap(&henry).await;
        let (_, theirs) = share_and_swap(&sam).await;
        // The swap is on the share's address: one opening, one address.
        assert_eq!((ours.subaddress(), ours.address()), (2, shown.address.as_str()));
        assert_eq!(henry.new_receive_address().await.unwrap(), henry.inner.keys.address(3));
        assert_ne!(own, henry.inner.keys.address(2));

        // Henry's phone reads Sam's: both have the other's.
        assert!(matches!(touch(&ours, &theirs), swap::Step::Done(ref p) if p.name == "Sam"));
        let (share, scanned, sure) = henry.end_swap(&ours).await.unwrap().unwrap();
        assert!(sure, "Henry's phone read Sam's, which said it took Henry's");
        assert_eq!((share.id, share.subaddress, share.kind.clone()), (shown.id, 2, profile::ShareKind::TouchedPhones { name: Some("Sam".into()) }));
        assert_eq!((scanned.code.address.as_str(), scanned.clash.is_none()), (sam.inner.keys.address(theirs.subaddress()).as_str(), true));
        let (_, got, sure) = sam.end_swap(&theirs).await.unwrap().unwrap();
        assert!(!sure, "Sam's phone, the card, can't know its last answer arrived");
        assert_eq!((got.code.name.as_str(), got.code.photo.clone(), got.code.address.clone()), ("Henry", Some(webp(1200)), henry.inner.keys.address(2)));
        // Ended twice (success, then the screen closing): kept once.
        assert!(henry.end_swap(&ours).await.unwrap().is_none());
        assert_eq!(henry.profile_shares(clock).await.iter().map(|l| l.title.as_str()).collect::<Vec<_>>(), ["Touched phones with Sam"]);
        // Adding Sam goes the way a scanned profile does, asking when he clashes.
        let text = uri::profile_uri(&got.code.address, &got.code.name, got.code.photo.as_deref());
        sam.add_contact_from_profile(&text, false).await.unwrap();
        let (_, again) = share_and_swap(&sam).await;
        let (_, ours) = share_and_swap(&henry).await;
        touch(&again, &ours);
        let (_, rescanned, _) = sam.end_swap(&again).await.unwrap().unwrap();
        assert_eq!(rescanned.clash.unwrap().why, "You have a contact named Henry.");
        assert_eq!(henry.end_swap(&ours).await.unwrap().unwrap().0.kind, profile::ShareKind::TouchedPhones { name: Some("Sam".into()) });

        // Nobody touched: the share stays "Code shown".
        let (idle_share, idle) = share_and_swap(&henry).await;
        assert!(henry.end_swap(&idle).await.unwrap().is_none());
        // Ours went out whole but theirs never came: "Touched phones".
        let (_, half) = share_and_swap(&henry).await;
        let (_, other) = share_and_swap(&sam).await;
        let mut apdu = other.reader_start().unwrap();
        while let swap::Step::Send(next) = other.reader_next(&half.card_answer(&apdu)) {
            if next[1] == 0xE2 {
                break; // gone before done
            }
            apdu = next;
        }
        half.card_lost();
        let none = henry.end_swap(&half).await.unwrap();
        assert!(none.is_none());
        let lines = henry.profile_shares(clock).await;
        let titles: Vec<&str> = lines.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(titles, ["Touched phones", "Code shown", "Touched phones with Sam", "Touched phones with Sam"]);
        assert_eq!(lines[1].address, idle_share.address);
        let reopened = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(reopened.profile_shares(clock).await.len(), 4);
        // Four openings, four addresses (2, 4, 5, 6; 3 was Receive's).
        assert_eq!(half.subaddress(), 6);
        assert_eq!(reopened.new_receive_address().await.unwrap(), reopened.inner.keys.address(7));
    }

    #[tokio::test]
    async fn only_the_share_just_made_swaps_and_only_until_its_address_goes_out() {
        let henry = Wallet::create(&dir("swap-fresh"), [1; 32], Network::Stagenet).unwrap();
        henry.set_profile("Henry", None).await.unwrap();
        let can_touch = |share: &profile::Share| {
            let henry = henry.clone();
            let id = share.id;
            async move { henry.share_code(id).await.unwrap().2 }
        };
        // Swapped with Sam, then the screen closed: reopened from Your shares, it neither swaps nor offers to.
        let (first, swap) = share_and_swap(&henry).await;
        assert!(can_touch(&first).await);
        henry.end_swap(&swap).await.unwrap();
        assert!(!can_touch(&first).await);
        assert!(henry.start_swap(first.id).await.is_err(), "reopened");
        // Shown but nobody touched, then a newer share made: the older one is done with.
        let (second, _) = henry.share_profile().await.unwrap();
        let (third, _) = henry.share_profile().await.unwrap();
        assert!(henry.start_swap(second.id).await.is_err(), "an older share");
        assert!(can_touch(&third).await);
        // Its link copied: its address is with someone, so the running swap ends and no other starts.
        let swap = henry.start_swap(third.id).await.unwrap();
        henry.share_went_out(third.id, profile::ShareKind::LinkCopied).await.unwrap();
        assert_eq!(swap.card_answer(&swap::select()), swap::NOT_AVAILABLE.to_vec());
        assert_eq!(swap.reader_start(), None);
        assert!(!can_touch(&third).await);
        assert!(henry.start_swap(third.id).await.is_err());
        // A swap that ended with nothing (the screen paused) can start again while the share is fresh.
        let (fourth, swap) = share_and_swap(&henry).await;
        drop(swap);
        assert!(henry.start_swap(fourth.id).await.is_ok());
    }

    #[tokio::test]
    async fn requests_carry_the_profile_s_name_while_the_switch_is_on() {
        let wallet = Wallet::create(&dir("profile-name-in-requests"), [1; 32], Network::Stagenet).unwrap();
        let (bare, code) = wallet.make_request(None, None, false, None).await.unwrap();
        assert!(!code.uri.contains("recipient_name"));
        assert!(wallet.set_name_in_requests(true).await.is_err(), "no profile yet");
        wallet.set_profile("Henry", None).await.unwrap();
        let (_, code) = wallet.make_request(None, Some("0.5"), false, None).await.unwrap();
        assert_eq!(code.uri, format!("monero:{}?tx_amount=0.5&recipient_name=Henry", wallet.inner.keys.address(2)));
        // A request's code again is the one that went out: no name added since, none taken away after.
        assert!(!wallet.request_code(bare.id).await.unwrap().uri.contains("recipient_name"));
        let (named, _) = wallet.make_request(None, None, false, Some("Named")).await.unwrap();
        wallet.set_profile("Hen", None).await.unwrap();
        assert!(wallet.request_code(named.id).await.unwrap().uri.contains("recipient_name=Henry"));
        wallet.set_name_in_requests(false).await.unwrap();
        assert!(wallet.request_code(named.id).await.unwrap().uri.contains("recipient_name=Henry"));
        assert!(!wallet.make_request(None, None, false, None).await.unwrap().1.uri.contains("recipient_name"));
        assert!(!wallet.profile().await.unwrap().name_in_requests);
    }

    #[tokio::test]
    async fn a_scanned_profile_adds_a_contact_or_updates_one_only_when_asked() {
        let wallet = Wallet::create(&dir("profile-scan"), [1; 32], Network::Stagenet).unwrap();
        let other = Keys::generate(Network::Stagenet).keys;
        let code = uri::profile_uri(&other.address(4), "Henry", Some(&webp(200)));
        let scanned = wallet.scanned_profile(&code).await.unwrap();
        assert!(scanned.clash.is_none());
        let henry = wallet.add_contact_from_profile(&code, false).await.unwrap();
        assert_eq!((henry.name.as_str(), henry.send_address.clone(), henry.photo.clone()), ("Henry", Some(other.address(4)), Some(webp(200))));
        // Scanned again from a new share: the same name, a new address. Nothing changes until Update.
        let newer = uri::profile_uri(&other.address(9), "Henry", None);
        let clash = wallet.scanned_profile(&newer).await.unwrap().clash.unwrap();
        assert_eq!((clash.contact.id, clash.changes.len()), (henry.id, 1));
        assert_eq!(wallet.contact(henry.id).await.unwrap().send_address, Some(other.address(4)));
        let updated = wallet.update_contact_from_profile(henry.id, &newer).await.unwrap();
        assert_eq!((updated.send_address, updated.photo), (Some(other.address(9)), Some(webp(200))));
        // Rust asks too, whatever the screen did: the same name only once asked; a contact's address never.
        let elsewhere = uri::profile_uri(&other.address(11), "Henry", None);
        assert!(wallet.add_contact_from_profile(&elsewhere, false).await.is_err());
        let lookalike = uri::profile_uri(&other.address(12), "Hen\u{200b}ry", None);
        assert!(wallet.add_contact_from_profile(&lookalike, false).await.is_err());
        let refused = wallet.add_contact_from_profile(&newer, true).await.unwrap_err().to_string();
        assert_eq!(refused, "That address is Henry's already: update Henry instead.");
        let renamed = uri::profile_uri(&other.address(9), "Someone else", None);
        assert!(wallet.add_contact_from_profile(&renamed, true).await.is_err(), "an address is never two contacts' send address");
        assert_eq!(wallet.contacts("", Clock { now: now(), utc_offset: 0, hours_24: true }).await.len(), 1);
        // Add as new contact, once asked about the name, keeps both.
        wallet.add_contact_from_profile(&elsewhere, true).await.unwrap();
        assert_eq!(wallet.contacts("henry", Clock { now: now(), utc_offset: 0, hours_24: true }).await.len(), 2);
        assert!(wallet.scanned_profile(&format!("monero:{}", other.address(4))).await.is_err());
        assert!(wallet.update_contact_from_profile(99, &newer).await.is_err());
    }

    #[tokio::test]
    async fn a_wallet_file_from_before_profiles_opens_with_none() {
        let path = dir("profile-old-file");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        wallet.set_profile("Henry", None).await.unwrap();
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        assert!(file["state"].as_object_mut().unwrap().remove("profile").is_some(), "the profile is saved");
        store::save(&path, &[1; 32], &file).unwrap();
        let old = Wallet::open(&path, [1; 32]).unwrap();
        assert!(old.profile().await.is_none());
        assert!(old.profile_shares(Clock { now: now(), utc_offset: 0, hours_24: true }).await.is_empty());
    }

    #[tokio::test]
    async fn a_photo_saved_before_headers_were_read_is_dropped_on_opening() {
        let path = dir("bad-photo");
        let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
        let good = contacts::test_webp(300, 96);
        wallet.set_profile("Henry", Some(good.clone())).await.unwrap();
        let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), photo: Some(good.clone()), ..Default::default() }).await.unwrap();
        let jo = wallet.add_contact(contacts::Draft { name: "Jo".into(), photo: Some(good.clone()), ..Default::default() }).await.unwrap();
        // As an older Fob kept them: a header claiming 16384 × 16384, for Jo and the profile.
        let huge = contacts::test_webp(300, 16384);
        let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
        file["state"]["contacts"]["contacts"][1]["photo"] = hex::encode(&huge).into();
        file["state"]["profile"]["profile"]["photo"] = hex::encode(&huge).into();
        store::save(&path, &[1; 32], &file).unwrap();
        let opened = Wallet::open(&path, [1; 32]).unwrap();
        assert_eq!(opened.contact(sam.id).await.unwrap().photo, Some(good));
        let jo = opened.contact(jo.id).await.unwrap();
        assert_eq!((jo.name.as_str(), jo.photo), ("Jo", None));
        assert_eq!(opened.profile().await.map(|p| (p.name, p.photo)), Some(("Henry".into(), None)));
        // And saved so: opened again, it's still gone.
        assert_eq!(Wallet::open(&path, [1; 32]).unwrap().contact(jo.id).await.unwrap().photo, None);
    }
}
