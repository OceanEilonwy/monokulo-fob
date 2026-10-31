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
    transaction::{Input, Transaction},
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use crate::{
    amount,
    error::{FobError, Result},
    keys::{Keys, SeedKind, StoredKeys},
    network::Network,
    receipt::ReceiptView,
    store,
    uri::{self, WalletCode},
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
}

#[derive(Clone, Serialize, Deserialize)]
struct State {
    node: String,
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
    /// Arrived but still confirming.
    pub confirming: u64,
}

impl Balance {
    /// "0.5 XMR ready to spend", under the balance while some of it can't be
    /// spent yet; nothing once all of it can.
    pub fn ready_text(&self) -> Option<String> {
        (self.unlocked < self.total).then(|| format!("{} XMR ready to spend", amount::format_short(self.unlocked)))
    }
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
    pub receipt: Option<ReceiptView>,
    pub receipt_json: Option<String>,
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
}

struct PreparedTx {
    signable: SignableTransaction,
    to: String,
    amount: u64,
    inputs: Vec<usize>,
    input_total: u64,
    receipt: Option<String>,
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
            restore_height,
            next_block: restore_height.unwrap_or(0),
            tip: 0,
            last_block_time: 0,
            recent: vec![],
            receive_index: 0,
            outputs: vec![],
            spends: vec![],
            sent: vec![],
        };
        let wallet = Wallet::from_parts(path, file_key, keys, state);
        wallet.save_state(&wallet.inner.state.try_lock().expect("a new wallet is not shared yet"))?;
        Ok(wallet)
    }

    fn from_parts(path: &Path, file_key: [u8; 32], keys: Keys, state: State) -> Wallet {
        let status = SyncStatus {
            scanned: state.next_block,
            tip: state.tip,
            start: state.restore_height.unwrap_or(state.next_block),
            up_to: state.last_block_time,
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
                rounds: tokio::sync::watch::Sender::new(0),
                pool: std::sync::Mutex::new(HashMap::new()),
                pool_seen: std::sync::Mutex::new(HashSet::new()),
                last_block: std::sync::Mutex::new(None),
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
        if state.receive_index == 0 {
            state.receive_index = 1;
            self.save_state(&state)?;
        }
        Ok(self.inner.keys.address(state.receive_index))
    }

    /// A fresh address nobody has seen.
    pub async fn new_receive_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        state.receive_index += 1;
        self.save_state(&state)?;
        Ok(self.inner.keys.address(state.receive_index))
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
        // A node on the other network would never find this wallet's money.
        crate::node::on_network(self.network(), &url).await.map_err(FobError::Node)?;
        {
            let mut state = self.inner.state.lock().await;
            state.node = url;
            self.save_state(&state)?;
        }
        *self.inner.rpc.lock().await = None;
        *self.inner.pipeline.lock().await = None;
        Ok(())
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
            if confirmations(&state, output.height) >= if output.coinbase { COINBASE_UNLOCK } else { UNLOCK } {
                balance.unlocked += output.amount;
            }
        }
        for sent in state.sent.iter().filter(|s| s.height.is_none()) {
            balance.total += sent.change;
        }
        for (amount, _) in self.inner.pool.lock().expect("pool lock").values() {
            balance.total += amount;
        }
        balance.confirming = balance.total - balance.unlocked;
        balance
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
                amount: sent.amount,
                fee: Some(sent.fee),
                to: Some(sent.to.clone()),
                height: sent.height,
                time: sent.time,
                confirmations: sent.height.map(|h| confirmations(&state, h)).unwrap_or(0),
                receipt: receipt.as_ref().map(ReceiptView::from),
                receipt_json: sent.receipt.clone(),
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
                receipt: None,
                receipt_json: None,
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
                receipt: None,
                receipt_json: None,
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
                    receipt: None,
                    receipt_json: None,
                });
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
            for output in found.not_additionally_locked() {
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
                state.receive_index = state.receive_index.max(subaddress);
                let key_image = keys.key_image(output.key(), output.key_offset()).map(hex::encode);
                // An output arriving in a transaction already seen spending (our
                // own change) is still ours to spend later.
                state.outputs.push(Owned {
                    output: hex::encode(output.serialize()),
                    coinbase: tx == miner_tx,
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
        let mut pool = self.inner.pool.lock().expect("pool lock");
        for output in found.ignore_additional_timelock() {
            if output.transaction() == miner {
                continue;
            }
            let entry = pool.entry(hex::encode(output.transaction())).or_insert((0, now()));
            entry.0 += output.commitment().amount;
        }
        self.inner.pool_seen.lock().expect("pool lock").extend(new.iter().map(hex::encode));
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
    /// every 20 s. `listener` hears every step.
    ///
    /// [`stop_sync`]: Wallet::stop_sync
    pub fn start_sync(&self, listener: Arc<dyn SyncListener>) {
        self.stop_sync();
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
                            wallet.wait(POLL).await;
                        }
                    }
                    Err(e) => {
                        wallet.report(listener.as_ref(), &mut rate, Some(e.to_string())).await;
                        wallet.inner.rounds.send_modify(|n| *n += 1);
                        wallet.wait(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                    }
                }
            }
        });
        *self.inner.syncing.lock().expect("sync lock") = Some(handle);
    }

    pub fn stop_sync(&self) {
        if let Some(handle) = self.inner.syncing.lock().expect("sync lock").take() {
            handle.abort();
        }
    }

    /// The sync loop's wait between rounds, cut short by [`sync_now`](Wallet::sync_now).
    async fn wait(&self, time: Duration) {
        tokio::select! {
            _ = tokio::time::sleep(time) => {}
            _ = self.inner.wake.notified() => {}
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
    /// Nothing leaves the wallet until [`send`](Wallet::send).
    pub async fn prepare(&self, to: &str, amount: u64, receipt_json: Option<String>) -> Result<Prepared> {
        self.inner.keys.spend()?;
        let network = self.network();
        let destination = MoneroAddress::from_str(network.monero(), to.trim()).map_err(|_| FobError::BadAddress(network.name()))?;
        if amount == 0 {
            return Err(FobError::BadAmount);
        }
        let (_, rpc, decoys, _) = self.connection().await?;
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let fee_rate = rpc.fee_rate(FeePriority::Normal, MAX_FEE_PER_WEIGHT).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        let latest = rpc.latest_block_number().await.map_err(node_err)?;

        let (mut candidates, available) = {
            let state = self.inner.state.lock().await;
            let mut candidates: Vec<(usize, u64, WalletOutput)> = state
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, o)| {
                    o.spent_in.is_none()
                        && confirmations(&state, o.height) >= if o.coinbase { COINBASE_UNLOCK } else { UNLOCK }
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
        let guess = |inputs: usize| fee_rate.calculate_fee_from_weight(1_600 + 900 * inputs as u64) as u64;
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
                vec![(destination, amount)],
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
                to: to.trim().to_owned(),
                amount,
                inputs: chosen.iter().map(|c| c.0).collect(),
                input_total: total,
                receipt: receipt.is_some().then_some(receipt_json).flatten(),
            },
        );
        Ok(Prepared { id, to: to.trim().to_owned(), amount, fee, receipt: receipt.as_ref().map(ReceiptView::from) })
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
        state.sent.push(Sent {
            tx: hash.clone(),
            to: prepared.to,
            amount: prepared.amount,
            fee,
            change: prepared.input_total.saturating_sub(prepared.amount).saturating_sub(fee),
            time: now(),
            height: None,
            receipt: prepared.receipt,
        });
        self.save_state(&state)?;
        Ok(hash)
    }

    /// Forgets a prepared payment that won't be sent.
    pub async fn cancel(&self, id: u64) {
        self.inner.prepared.lock().await.remove(&id);
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
        assert!(matches!(watch.prepare(&full.primary_address(), 1, None).await, Err(FobError::ViewOnly)));
    }

    #[tokio::test]
    async fn an_empty_wallet_has_nothing_to_show() {
        let wallet = Wallet::create(&dir("empty"), [1; 32], Network::Stagenet).unwrap();
        assert_eq!(wallet.balance().await, Balance::default());
        assert!(wallet.activity().await.is_empty());
    }

    #[test]
    fn the_ready_line_shows_only_while_some_money_is_waiting() {
        let all = Balance { total: 2_500_000_000_000, unlocked: 2_500_000_000_000, confirming: 0 };
        assert_eq!(all.ready_text(), None);
        let some = Balance { total: 2_500_000_000_000, unlocked: 500_000_000_000, confirming: 2_000_000_000_000 };
        assert_eq!(some.ready_text().as_deref(), Some("0.5 XMR ready to spend"));
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

    #[tokio::test]
    async fn sync_now_without_the_loop_runs_one_round_itself() {
        let wallet = unreachable("sync-now-alone").await;
        let started = Instant::now();
        wallet.sync_now().await;
        assert!(started.elapsed() < SYNC_NOW_WAIT);
    }
}
