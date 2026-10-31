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
    amount,
    error::{FobError, Result},
    keys::{Keys, SeedKind, StoredKeys},
    network::Network,
    not_ready::{self, Coins, NotReady, Piece, Source},
    price::{self, Source as _},
    receipt::ReceiptView,
    split, store,
    uri::{self, WalletCode},
    zmq::{self, ZmqStatus},
};


/// The most blocks scanned per sync step: whatever the pipeline has ready,
/// up to this, so progress shows often.
const BATCH: u64 = 1_000;
/// Subaddresses watched past the last one handed out.
const LOOKAHEAD: u32 = 50;
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
}

#[derive(Clone, Serialize, Deserialize)]
struct State {
    node: String,
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
    /// The highest subaddress handed out.
    receive_index: u32,
    outputs: Vec<Owned>,
    spends: Vec<ChainSpend>,
    sent: Vec<Sent>,
    /// How prices are shown; on, in dollars, from Kraken over the internet
    /// for a file from before prices.
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
    /// shown in; `None` with prices off or no price to be had.
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
    /// Under the field, for an amount in money: "The code asks for 0.1204
    /// XMR · 1 XMR = $166.10".
    pub asks: Option<String>,
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
    pub seconds_left: Option<u64>,
    /// When the last scanned block was mined.
    pub up_to: u64,
    pub error: Option<String>,
    /// Instant updates from the node's ZMQ publisher.
    pub zmq: ZmqStatus,
    /// [`zmq`](Self::zmq) as Settings shows it: "node.example:18083 · listening".
    pub zmq_text: String,
}

/// Told about every change while the wallet syncs.
pub trait SyncListener: Send + Sync {
    fn on_status(&self, status: SyncStatus);
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
    to: String,
    amount: u64,
    inputs: Vec<usize>,
    input_total: u64,
    receipt: Option<String>,
    note: Option<String>,
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
    rpc: Mutex<Option<(String, Connection)>>,
    /// Blocks being fetched ahead of the scanner.
    pipeline: Mutex<Option<Pipeline>>,
    prepared: Mutex<HashMap<u64, PreparedTx>>,
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
    /// Payments to this wallet still in the node's pool: amount by
    /// transaction. Not saved: the pool is asked again after every block.
    pool: std::sync::Mutex<HashMap<String, (u64, u64)>>,
    /// Pool transactions already looked at, so each is fetched once.
    pool_seen: std::sync::Mutex<HashSet<String>>,
    /// The last block scanned, to scan pool transactions against.
    last_block: std::sync::Mutex<Option<monero_wallet::block::Block>>,
    /// Asks for prices every 5 minutes while Fob is on screen.
    prices_task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Has the price task ask at once (a setting changed).
    prices_wake: tokio::sync::Notify,
    /// Why the last price request failed, until one works.
    price_error: std::sync::Mutex<Option<String>>,
    /// The node's fee per unit of weight and its rounding mask, as last
    /// asked: Split up's fee "about" before a payment is built.
    fee_rate: std::sync::Mutex<Option<(u64, u64)>>,
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
            zmq_endpoint: None,
            zmq_on: false,
            restore_height,
            next_block: restore_height.unwrap_or(0),
            tip: 0,
            last_block_time: 0,
            recent: vec![],
            receive_index: 0,
            outputs: vec![],
            spends: vec![],
            sent: vec![],
            prices: price::Settings::default(),
            price_cache: price::Cache::default(),
            price_records: HashMap::new(),
            notes: HashMap::new(),
            split_index: None,
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
                pipeline: Mutex::new(None),
                prepared: Mutex::new(HashMap::new()),
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
                price_error: std::sync::Mutex::new(None),
                fee_rate: std::sync::Mutex::new(None),
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

    /// The wallet saved at `path`.
    pub fn open(path: &Path, file_key: [u8; 32]) -> Result<Wallet> {
        let file: FileIn = store::load(path, &file_key)?;
        Ok(Wallet::from_parts(path, file_key, file.keys.into_keys()?, file.state))
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

    /// A wallet code for restoring this wallet elsewhere.
    pub async fn wallet_code(&self) -> Zeroizing<String> {
        let keys = &self.inner.keys;
        let height = {
            let state = self.inner.state.lock().await;
            state.restore_height.unwrap_or(state.next_block)
        };
        uri::wallet_code(&keys.primary_address(), keys.spend_key_hex().as_deref().map(|s| s.as_str()), &keys.view_key_hex(), height)
    }

    pub fn primary_address(&self) -> String {
        self.inner.keys.primary_address()
    }

    /// The address to give out now: the last one handed out, or a first one.
    pub async fn receive_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        // Never the address splits pay to.
        if state.receive_index == 0 || Some(state.receive_index) == state.split_index {
            state.receive_index += 1;
            if Some(state.receive_index) == state.split_index {
                state.receive_index += 1;
            }
            self.save_state(&state)?;
        }
        Ok(self.inner.keys.address(state.receive_index))
    }

    /// A fresh address nobody has seen.
    pub async fn new_receive_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        state.receive_index += 1;
        // Never the address splits pay to.
        if Some(state.receive_index) == state.split_index {
            state.receive_index += 1;
        }
        self.save_state(&state)?;
        Ok(self.inner.keys.address(state.receive_index))
    }

    /// Keeps the note put in a payment code for the address at `index`, so
    /// payments to it say what they were for; a blank one forgets it.
    pub async fn remember_receive_note(&self, index: u32, note: &str) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let note = note.trim();
        let changed = if note.is_empty() { state.notes.remove(&index).is_some() } else { state.notes.insert(index, note.to_owned()).as_deref() != Some(note) };
        if changed { self.save_state(&state) } else { Ok(()) }
    }

    pub async fn receive_index(&self) -> u32 {
        self.inner.state.lock().await.receive_index
    }

    pub async fn node(&self) -> String {
        self.inner.state.lock().await.node.clone()
    }

    /// Talks to another node from now on.
    pub async fn set_node(&self, url: &str) -> Result<()> {
        let url = crate::node::normalize(url);
        // An onion node can't be reached without Tor, and isn't looked up.
        crate::node::reachable(&url).map_err(FobError::BadEndpoint)?;
        // A node on the other network would never find this wallet's money.
        crate::node::on_network(self.network(), &url).await.map_err(FobError::Node)?;
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
            self.save_state(&state)?;
            dropped
        };
        if zmq_dropped {
            self.restart_zmq().await;
        }
        *self.inner.rpc.lock().await = None;
        *self.inner.pipeline.lock().await = None;
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
        let node = self.inner.state.lock().await.node.clone();
        let mut rpc = self.inner.rpc.lock().await;
        if let Some((url, connection)) = rpc.as_ref()
            && *url == node
        {
            return Ok(connection.clone());
        }
        let err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let transport = Transport::new(&node).map_err(err)?;
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
        for (amount, _) in self.inner.pool.lock().expect("pool lock").values() {
            balance.total += amount;
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
        for (amount, seen) in self.inner.pool.lock().expect("pool lock").values() {
            waiting.push(Piece { amount: *amount, confirmations: 0, needed: UNLOCK, source: Source::Received, time: *seen });
        }
        (ready, waiting, state.last_block_time)
    }

    /// Every coin, ready or not, as the Coins screen shows it.
    pub async fn coins(&self, clock: Clock) -> Coins {
        let (ready, waiting, last_block_time) = self.coin_pieces().await;
        let price = self.price_now().await;
        not_ready::coins(ready, waiting, last_block_time, UNLOCK, price.as_ref().map(|(c, p)| (c, *p)), clock)
    }

    /// The money not ready yet as Home shows it, beside what's ready to
    /// spend, or `None` when all of it is.
    pub async fn not_ready(&self, clock: Clock) -> Option<NotReady> {
        let ready = self.balance().await.unlocked;
        let (_, pieces, last_block_time) = self.coin_pieces().await;
        let mut summary = not_ready::summarise(pieces, ready, last_block_time, clock)?;
        if let Some((currency, p)) = self.price_now().await {
            for piece in &mut summary.pieces {
                piece.fiat = Some(price::value(piece.atomic, p, &currency));
            }
            for sum in &mut summary.sums {
                sum.fiat = Some(price::value(sum.atomic, p, &currency));
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
            });
            row.amount += output.amount;
        }
        rows.extend(incoming.into_values());
        for (tx, (amount, seen)) in self.inner.pool.lock().expect("pool lock").iter() {
            if !rows.iter().any(|r| r.tx == *tx) {
                rows.push(Activity {
                    tx: tx.clone(),
                    direction: Direction::In,
                    amount: *amount,
                    fee: None,
                    to: None,
                    height: None,
                    time: *seen,
                    confirmations: 0,
                    needed: UNLOCK,
                    receipt: None,
                    receipt_json: None,
                    price: None,
                    subaddress: None,
                    pieces: vec![],
                    note: None,
                    split: None,
                });
            }
        }
        for row in &mut rows {
            if state.prices.on {
                row.price = state.price_records.get(&row.tx).and_then(|r| r.iter().find(|r| r.currency == state.prices.currency)).cloned();
            }
            match row.direction {
                Direction::In => {
                    let outputs: Vec<&Owned> = state.outputs.iter().filter(|o| o.tx == row.tx).collect();
                    row.pieces = outputs.iter().map(|o| o.amount).collect();
                    row.needed = outputs.iter().map(|o| o.needed()).max().unwrap_or(UNLOCK);
                    row.subaddress = outputs.first().map(|o| o.subaddress);
                    row.note = row.subaddress.and_then(|s| state.notes.get(&s)).cloned();
                }
                Direction::Out => {
                    row.pieces = state.outputs.iter().filter(|o| o.spent_in.as_deref() == Some(row.tx.as_str())).map(|o| o.amount).collect();
                    let sent = state.sent.iter().find(|s| s.tx == row.tx);
                    row.note = sent.and_then(|s| s.note.clone());
                    row.split = sent.and_then(|s| s.pieces);
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
        for i in 1..=state.receive_index.saturating_add(LOOKAHEAD) {
            if let Some(sub) = SubaddressIndex::new(0, i) {
                scanner.register_subaddress(sub);
            }
        }
        for scannable in blocks {
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
        for i in 1..=receive_index.saturating_add(LOOKAHEAD) {
            if let Some(sub) = SubaddressIndex::new(0, i) {
                scanner.register_subaddress(sub);
            }
        }
        let found = scanner.scan(stand_in).map_err(|e| FobError::Node(format!("scanning the pool: {e:?}")))?;
        {
            let mut pool = self.inner.pool.lock().expect("pool lock");
            for output in found.ignore_additional_timelock() {
                if output.transaction() == miner {
                    continue;
                }
                let entry = pool.entry(hex::encode(output.transaction())).or_insert((0, now()));
                entry.0 += output.commitment().amount;
            }
        }
        self.inner.pool_seen.lock().expect("pool lock").extend(new.iter().map(hex::encode));
        // A payment first seen in the pool is priced as it's seen.
        let mut state = self.inner.state.lock().await;
        if self.record_prices(&mut state) {
            self.save_state(&state)?;
        }
        Ok(())
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

    async fn report(&self, listener: &dyn SyncListener, rate: &mut Option<(Instant, u64)>, error: Option<String>) {
        self.refresh_status().await;
        let (next, tip) = {
            let status = self.status();
            (status.scanned, status.tip)
        };
        let left = tip.saturating_sub(next);
        let seconds_left = match rate {
            _ if left == 0 => Some(0),
            Some((since, from)) if next > *from => {
                let per_block = since.elapsed().as_secs_f64() / (next - *from) as f64;
                Some((per_block * left as f64).ceil() as u64)
            }
            _ => None,
        };
        if rate.is_none() && left > 0 {
            *rate = Some((Instant::now(), next));
        }
        let status = self.set_status(|s| {
            s.seconds_left = seconds_left;
            s.error = error;
        });
        listener.on_status(status);
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
            let mut rate = None;
            let mut backoff = Duration::from_secs(2);
            loop {
                match wallet.sync_step().await {
                    Ok(more) => {
                        backoff = Duration::from_secs(2);
                        wallet.report(listener.as_ref(), &mut rate, None).await;
                        if !more {
                            rate = None;
                            wallet.inner.rounds.send_modify(|n| *n += 1);
                            let poll = if wallet.status().zmq == ZmqStatus::Listening { POLL_LISTENING } else { POLL };
                            wallet.wait(poll, listener.as_ref()).await;
                        }
                    }
                    Err(e) => {
                        wallet.report(listener.as_ref(), &mut rate, Some(e.to_string())).await;
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
                        self.report(listener, &mut None, None).await;
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

    /// Builds a payment to `to`, choosing the money to spend and the fee.
    /// Nothing leaves the wallet until [`send`](Wallet::send). `note` (the
    /// payment code's description) is kept with the payment once sent.
    pub async fn prepare(&self, to: &str, amount: u64, receipt_json: Option<String>, note: Option<String>) -> Result<Prepared> {
        self.inner.keys.spend()?;
        let network = self.network();
        let destination = MoneroAddress::from_str(network.monero(), to.trim()).map_err(|_| FobError::BadAddress(network.name()))?;
        if amount == 0 {
            return Err(FobError::BadAmount);
        }
        self.prepare_payments(to.trim(), vec![(destination, amount)], receipt_json, note, None).await
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
                to: to.to_owned(),
                amount,
                inputs: chosen.iter().map(|c| c.0).collect(),
                input_total: total,
                receipt: receipt.is_some().then_some(receipt_json).flatten(),
                note: note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()),
                pieces,
                back: if pieces.is_some() { payments.iter().map(|p| p.1).collect() } else { vec![] },
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

    /// What Split up opens with, learned from the payments of the last 30
    /// days, or an even mix.
    pub async fn split_suggestion(&self) -> split::Suggestion {
        let (money, notes, budget) = self.split_setup().await;
        let since = now().saturating_sub(split::LOOKBACK);
        let payments: Vec<u64> = self
            .activity()
            .await
            .into_iter()
            .filter(|a| a.direction == Direction::Out && a.split.is_none() && a.time >= since && a.amount > 0)
            .map(|a| a.amount)
            .collect();
        split::suggest(&notes, &payments, &budget, money.as_ref().map(|(c, p)| (c, *p)))
    }

    /// Split up's screen for `counts` of each note.
    pub async fn split_view(&self, counts: &[u32]) -> split::View {
        let (money, notes, budget) = self.split_setup().await;
        split::view(&notes, counts, &budget, money.as_ref().map(|(c, p)| (c, *p)))
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
        let mut prepared = self.prepare_payments(&address, payments, None, None, Some(n)).await?;
        prepared.split = split::lines(&notes, counts);
        Ok(prepared)
    }

    /// Signs and broadcasts a prepared payment. Returns its transaction hash.
    pub async fn send(&self, id: u64) -> Result<String> {
        let prepared = self.inner.prepared.lock().await.remove(&id).ok_or(FobError::NoSuchPayment)?;
        let spend = self.inner.keys.spend()?;
        let fee = prepared.signable.necessary_fee();
        let tx = prepared.signable.sign(&mut OsRng, spend).map_err(|e| FobError::Send(format!("{e:?}")))?;
        let hash = hex::encode(tx.hash());
        {
            // Spent already by a payment sent in the meantime?
            let state = self.inner.state.lock().await;
            if prepared.inputs.iter().any(|&i| state.outputs.get(i).is_none_or(|o| o.spent_in.is_some())) {
                return Err(FobError::NoSuchPayment);
            }
        }
        let rpc = self.rpc().await?;
        rpc.publish_transaction(&tx).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        let mut state = self.inner.state.lock().await;
        for &i in &prepared.inputs {
            if let Some(output) = state.outputs.get_mut(i) {
                output.spent_in = Some(hash.clone());
            }
        }
        // A split's pieces come back to this wallet with the change.
        let back = if prepared.pieces.is_some() { prepared.input_total } else { prepared.input_total.saturating_sub(prepared.amount) };
        let change = prepared.input_total.saturating_sub(prepared.amount).saturating_sub(fee);
        let outputs = prepared.back.iter().copied().chain((change > 0).then_some(change)).collect();
        state.sent.push(Sent {
            tx: hash.clone(),
            to: prepared.to,
            amount: prepared.amount,
            fee,
            change: back.saturating_sub(fee),
            time: now(),
            height: None,
            receipt: prepared.receipt,
            note: prepared.note,
            pieces: prepared.pieces,
            back: outputs,
        });
        self.record_prices(&mut state);
        self.save_state(&state)?;
        Ok(hash)
    }

    /// Forgets a prepared payment that won't be sent.
    pub async fn cancel(&self, id: u64) {
        self.inner.prepared.lock().await.remove(&id);
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
        for (tx, (_, seen)) in self.inner.pool.lock().expect("pool lock").iter() {
            times.entry(tx.clone()).or_insert(*seen);
        }
        times.into_iter().collect()
    }

    /// Records what XMR was worth with each payment that has no price yet
    /// in the currency shown: the current price if it was asked within an
    /// hour of the payment, else the day's from the history. Nothing while
    /// prices are off; a payment with nothing to go on stays without, and
    /// gets one if the history later has its day. Returns whether any was
    /// recorded.
    fn record_prices(&self, state: &mut State) -> bool {
        if !state.prices.on {
            return false;
        }
        let mut recorded = false;
        for (tx, time) in self.payment_times(state) {
            let records = state.price_records.get(&tx);
            if records.is_some_and(|r| r.iter().any(|r| r.currency == state.prices.currency)) {
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

    /// Why prices couldn't be had last time, until they can.
    pub fn price_error(&self) -> Option<String> {
        self.inner.price_error.lock().expect("price lock").clone()
    }

    async fn change_prices(&self, change: impl FnOnce(&mut price::Settings) -> Result<()>) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let mut settings = state.prices.clone();
        change(&mut settings)?;
        if settings != state.prices {
            state.prices = settings;
            self.record_prices(&mut state);
            self.save_state(&state)?;
            *self.inner.price_error.lock().expect("price lock") = None;
            self.inner.prices_wake.notify_one();
        }
        Ok(())
    }

    /// "Show prices". Off, Fob never asks for one.
    pub async fn set_prices_on(&self, on: bool) -> Result<()> {
        self.change_prices(|s| {
            s.on = on;
            Ok(())
        })
        .await
    }

    /// Prices in `code`, one the provider lists.
    pub async fn set_currency(&self, code: &str) -> Result<()> {
        self.change_prices(|s| {
            if !s.provider.currencies().contains(&code) {
                return Err(FobError::Price(format!("{} has no XMR price in {code}", s.provider.name())));
            }
            s.currency = code.to_owned();
            Ok(())
        })
        .await
    }

    /// How price requests travel. Tor isn't available yet, and is refused.
    pub async fn set_price_transport(&self, transport: price::Transport) -> Result<()> {
        self.change_prices(|s| {
            if !transport.available() {
                return Err(FobError::Price("prices over Tor aren't available yet".into()));
            }
            s.transport = transport;
            Ok(())
        })
        .await
    }

    /// "Show amounts in": the currency first, or XMR.
    pub async fn set_fiat_first(&self, fiat_first: bool) -> Result<()> {
        self.change_prices(|s| {
            s.fiat_first = fiat_first;
            Ok(())
        })
        .await
    }

    /// The current price shown now, when prices are on and one was asked
    /// within the hour: (currency, price).
    pub async fn price_now(&self) -> Option<(price::Currency, u64)> {
        let state = self.inner.state.lock().await;
        if !state.prices.on {
            return None;
        }
        state.price_cache.fresh(&state.prices, now()).map(|c| (price::currency(&c.currency), c.price))
    }

    /// What `atomic` XMR is worth now: "$83.03"; `None` without a current price.
    pub async fn worth_now(&self, atomic: u64) -> Option<String> {
        self.price_now().await.map(|(currency, p)| price::value(atomic, p, &currency))
    }

    /// The payment `tx` as its own screen shows it, or `None` if there's no
    /// such payment.
    pub async fn payment(&self, tx: &str, clock: Clock) -> Option<activity::Detail> {
        let row = self.activity().await.into_iter().find(|a| a.tx == tx)?;
        let settings = self.price_settings().await;
        let now = self.price_now().await;
        Some(activity::detail(&row, now.as_ref().map(|(c, p)| (c, *p)), settings.on, settings.provider.name(), clock))
    }

    /// Whether Receive can take an amount in money: the currency and what
    /// goes before what's typed ("$", or the code), with prices on and a
    /// current price.
    pub async fn receive_currency(&self) -> Option<(String, String)> {
        let (currency, _) = self.price_now().await?;
        Some((currency.symbol.map(str::to_owned).unwrap_or_else(|| currency.code.clone()), currency.code))
    }

    /// Receive's amount as typed, the other way: money to XMR (`to_fiat`
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

    /// Receive's payment code for `address`, with an amount typed in XMR or,
    /// when `in_fiat`, in money (the code still asks in XMR, converted at
    /// the current price), and a note for the payer.
    pub async fn receive_code(&self, address: &str, amount_text: Option<&str>, in_fiat: bool, note: Option<&str>) -> Result<ReceiveCode> {
        let typed = amount_text.map(str::trim).filter(|a| !a.is_empty());
        let price = self.price_now().await;
        let (xmr, fiat) = match (typed, in_fiat) {
            (None, _) => (None, None),
            (Some(t), false) => (Some(amount::parse(t)?), None),
            (Some(t), true) => {
                let (currency, p) = price.as_ref().ok_or_else(|| FobError::Price("no price to convert with".into()))?;
                let minor = price::parse(t, currency)?;
                (Some(price::to_atomic(minor, *p, currency).ok_or(FobError::BadAmount)?), Some(price::format(minor, currency)))
            }
        };
        let xmr = xmr.filter(|a| *a > 0);
        let xmr_text = xmr.map(amount::format);
        let code = uri::receive(address, xmr_text.as_deref(), note)?;
        Ok(match (fiat.filter(|_| xmr.is_some()), xmr_text, price) {
            (Some(fiat), Some(xmr), Some((currency, p))) => ReceiveCode {
                uri: code.uri,
                headline: Some(fiat),
                detail: Some([Some(format!("{xmr} XMR")), code.detail].into_iter().flatten().collect::<Vec<_>>().join(" · ")),
                asks: Some(format!("The code asks for {xmr} XMR · {}", price::rate(p, &currency))),
            },
            _ => ReceiveCode { uri: code.uri, headline: code.headline, detail: code.detail, asks: None },
        })
    }

    /// Asks the provider for what's due, if prices are on: the current price
    /// every 5 minutes (a minute when `pulled`), the daily history once a
    /// day while a payment lacks a price. Returns whether anything changed.
    pub async fn refresh_prices(&self, pulled: bool) -> Result<bool> {
        let provider = self.inner.state.lock().await.prices.provider;
        let result = self.refresh_prices_from(&provider, pulled, now()).await;
        *self.inner.price_error.lock().expect("price lock") = result.as_ref().err().map(ToString::to_string);
        result
    }

    async fn refresh_prices_from(&self, source: &impl price::Source, pulled: bool, now: u64) -> Result<bool> {
        let (settings, current_due, history_due) = {
            let state = self.inner.state.lock().await;
            let settings = state.prices.clone();
            if !settings.on {
                return Ok(false);
            }
            let window = price::window(source, now);
            let lacking = self.payment_times(&state).into_iter().any(|(tx, time)| {
                window.contains(&price::day(time))
                    && !state.price_records.get(&tx).is_some_and(|r| r.iter().any(|r| r.currency == settings.currency))
            });
            let current_due = price::current_due(&state.price_cache, &settings, now, pulled);
            let history_due = lacking && price::history_due(&state.price_cache, &settings, now);
            (settings, current_due, history_due)
        };
        if !settings.transport.available() {
            return Err(FobError::Price("prices over Tor aren't available yet".into()));
        }
        let current = if current_due { Some(source.current(&settings.currency, settings.transport).await) } else { None };
        let history = if history_due { Some(source.daily(&settings.currency, price::window(source, now), settings.transport).await) } else { None };

        let mut state = self.inner.state.lock().await;
        if state.prices != settings {
            // Changed while asking: these answers are for settings gone.
            return Ok(false);
        }
        let mut changed = false;
        let mut error = None;
        match current {
            Some(Ok(p)) => {
                state.price_cache.current = Some(price::Current { currency: settings.currency.clone(), provider: source.name().into(), price: p, at: now });
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
                let before = wallet.price_error();
                let changed = wallet.refresh_prices(false).await.unwrap_or(false);
                if changed || wallet.price_error() != before {
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
        let _ = tokio::join!(self.sync_now(), self.refresh_prices(true));
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
        wallet.inner.pool.lock().unwrap().insert("pool".into(), (70, 6_000));

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
        let lines: HashMap<String, String> = activity::by_day(wallet.activity().await, Clock { now: 6_000, utc_offset: 0, hours_24: true })
            .into_iter()
            .flat_map(|d| d.rows)
            .map(|r| (r.activity.tx.clone(), r.line.split(" · ").take(2).collect::<Vec<_>>().join(" · ")))
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
    }

    impl FakePrices {
        fn new(now: u64, days: Vec<(i64, u64)>) -> FakePrices {
            FakePrices { now, days, fail: false, asked_now: AtomicU64::new(0), asked_days: AtomicU64::new(0) }
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

        async fn current(&self, _: &str, _: price::Transport) -> Result<u64> {
            self.asked_now.fetch_add(1, Ordering::Relaxed);
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
        assert!(wallet.refresh_prices_from(&fake, false, t).await.unwrap());
        assert_eq!(fake.asked(), (1, 0));
        assert_eq!(wallet.worth_now(XMR / 2).await.as_deref(), Some("$83.03"));
        {
            let mut state = wallet.inner.state.lock().await;
            state.outputs.push(received("in", t - 120, XMR / 2));
            state.sent.push(Sent { tx: "out".into(), to: "86Ux".into(), amount: XMR / 10, fee: 1, change: 0, time: t, height: None, receipt: None, note: None, pieces: None, back: vec![] });
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
        assert!(!wallet.refresh_prices_from(&fake, false, t + 299).await.unwrap());
        assert!(wallet.refresh_prices_from(&fake, true, t + 60).await.unwrap());
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
        assert!(wallet.refresh_prices_from(&fake, false, t).await.unwrap());
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
        wallet.refresh_prices_from(&fake, false, t + 600).await.unwrap();
        assert_eq!(fake.asked(), (2, 1));
        assert_eq!(price_of(&wallet.activity().await, "last-week"), None);
        // The next day it is.
        wallet.refresh_prices_from(&fake, false, t + DAY).await.unwrap();
        assert_eq!(fake.asked(), (3, 2));
    }

    #[tokio::test]
    async fn with_prices_off_nothing_is_asked_or_shown() {
        let wallet = Wallet::create(&dir("price-off"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        let fake = FakePrices::new(166_000_000, vec![(price::day(t), 165_000_000)]);
        wallet.refresh_prices_from(&fake, false, t).await.unwrap();
        wallet.set_prices_on(false).await.unwrap();
        wallet.inner.state.lock().await.outputs.push(received("while-off", t, XMR));
        assert!(!wallet.refresh_prices_from(&fake, true, t + DAY).await.unwrap());
        assert_eq!(fake.asked(), (1, 0), "no request while off");
        assert_eq!(wallet.worth_now(XMR).await, None);
        assert!(!wallet.record_prices(&mut *wallet.inner.state.lock().await));
        assert!(wallet.activity().await.iter().all(|a| a.price.is_none()));
        // Back on: the payment seen while off is priced from what's cached.
        wallet.set_prices_on(true).await.unwrap();
        assert_eq!(price_of(&wallet.activity().await, "while-off"), Some((166_000_000, false)));
    }

    #[tokio::test]
    async fn a_failed_request_leaves_the_payment_not_available() {
        let wallet = Wallet::create(&dir("price-fail"), [1; 32], Network::Stagenet).unwrap();
        let t = now();
        wallet.inner.state.lock().await.outputs.push(received("in", t - 3 * DAY, XMR));
        let fake = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
        assert!(wallet.refresh_prices_from(&fake, false, t).await.is_err());
        assert_eq!(fake.asked(), (1, 1));
        assert_eq!(price_of(&wallet.activity().await, "in"), None);
        assert_eq!(wallet.worth_now(XMR).await, None);
    }

    #[tokio::test]
    async fn receive_takes_an_amount_in_money_and_asks_in_xmr() {
        let wallet = Wallet::create(&dir("receive-fiat"), [1; 32], Network::Stagenet).unwrap();
        let address = wallet.receive_address().await.unwrap();
        // No price yet: XMR only.
        assert_eq!(wallet.receive_currency().await, None);
        assert!(matches!(wallet.receive_code(&address, Some("20"), true, None).await, Err(FobError::Price(_))));
        wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), false, now()).await.unwrap();
        assert_eq!(wallet.receive_currency().await, Some(("$".into(), "USD".into())));

        let code = wallet.receive_code(&address, Some("20"), true, Some("Coffee beans")).await.unwrap();
        assert_eq!(code.uri, format!("monero:{address}?tx_amount=0.1204&tx_description=Coffee%20beans"));
        assert_eq!(code.headline.as_deref(), Some("$20.00"));
        assert_eq!(code.detail.as_deref(), Some("0.1204 XMR · Coffee beans"));
        assert_eq!(code.asks.as_deref(), Some("The code asks for 0.1204 XMR · 1 XMR = $166.10"));
        // In XMR, as before.
        let code = wallet.receive_code(&address, Some("0.5"), false, None).await.unwrap();
        assert_eq!((code.headline.as_deref(), code.detail, code.asks), (Some("0.5 XMR"), None, None));
        assert!(wallet.receive_code(&address, Some("20.123"), true, None).await.is_err());
        // Switching the field converts what's typed.
        assert_eq!(wallet.convert_amount("20", false).await, "0.1204");
        assert_eq!(wallet.convert_amount("0.1204", true).await, "20.00");
        assert_eq!(wallet.convert_amount("", true).await, "");
        // Prices off: no money at all.
        wallet.set_prices_on(false).await.unwrap();
        assert_eq!(wallet.receive_currency().await, None);
        assert_eq!(wallet.convert_amount("20", false).await, "");
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
            state.sent.push(Sent { tx: "out".into(), to: "86Ux".into(), amount: 1, fee: 1, change: 0, time: t, height: None, receipt: None, note: Some("Rent".into()), pieces: None, back: vec![] });
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
        // No price: notes in XMR.
        let suggestion = wallet.split_suggestion().await;
        assert_eq!(suggestion.text, "Picked from your last 30 days: most payments were about 0.1 XMR.");
        assert_eq!(suggestion.counts, [0, 0, 15, 0, 0, 0]);
        let view = wallet.split_view(&suggestion.counts).await;
        assert_eq!((view.summary.as_deref(), view.button.as_str()), (Some("15 coins · 1.5 XMR"), "Split into 15 coins"));
        assert!(view.footer.starts_with("Network fee about 0.0001"), "{}", view.footer);
        let refused = wallet.split_add(&suggestion.counts, 0).await.unwrap_err();
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
        let row = activity::by_day(wallet.activity().await, clock).into_iter().flat_map(|d| d.rows).find(|r| r.activity.tx == "split").unwrap();
        assert_eq!((row.title.as_str(), row.amount.as_str()), ("Split up", "−0.000000000001"));
        assert!(row.line.starts_with("Sending · 15 coins · "), "{}", row.line);
    }

    #[tokio::test]
    async fn price_settings_refuse_what_fob_cant_do() {
        let wallet = Wallet::create(&dir("price-settings"), [1; 32], Network::Stagenet).unwrap();
        assert!(matches!(wallet.set_currency("GBP").await, Err(FobError::Price(_))));
        assert_eq!(
            wallet.set_price_transport(price::Transport::Tor).await.unwrap_err().to_string(),
            "prices over Tor aren't available yet"
        );
        wallet.set_currency("EUR").await.unwrap();
        wallet.set_fiat_first(true).await.unwrap();
        let again = Wallet::open(&wallet.inner.path, [1; 32]).unwrap();
        let settings = again.price_settings().await;
        assert_eq!((settings.currency.as_str(), settings.fiat_first, settings.transport), ("EUR", true, price::Transport::Web));
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

    /// Fob has no Tor: an onion node is refused when chosen, and one
    /// already in a wallet file is never looked up or connected to.
    #[tokio::test]
    async fn an_onion_node_is_refused_and_never_reached() {
        let wallet = Wallet::create(&dir("onion"), [1; 32], Network::Stagenet).unwrap();
        let before = wallet.node().await;
        let refused = wallet.set_node("xmrnode.onion:38081").await.unwrap_err();
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
}
