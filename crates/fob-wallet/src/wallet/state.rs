//! What a wallet keeps in its database, and what it found on the chain.

use super::*;

/// An output the wallet owns.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Owned {
    /// The `WalletOutput`, serialized, in hex.
    pub(super) output: String,
    pub(super) tx: String,
    pub(super) height: u64,
    pub(super) time: u64,
    pub(super) amount: u64,
    pub(super) subaddress: u32,
    pub(super) coinbase: bool,
    /// Its transaction's unlock time as Monero writes it: a block (a mined
    /// reward's is 60 blocks on), so it can't be spent before the chain has
    /// that many blocks; or, from [`TIME_LOCK_FROM`] on, a Unix time, so it
    /// can't be spent before the chain's blocks are that late. 0 for none.
    #[serde(default)]
    pub(super) unlock_block: u64,
    /// The output's one-time key, in hex: a second output with the same key
    /// is the burning bug and isn't counted.
    #[serde(default)]
    pub(super) key: String,
    /// `None` in a view-only wallet, which can't tell when it's spent, and
    /// in a device's until the device has been asked.
    pub(super) key_image: Option<String>,
    pub(super) spent_in: Option<String>,
    /// The transaction key the output was made with, and the extra key at
    /// its index if its transaction carried those: what a device needs to
    /// make its key image and spend it. In hex; `None` before Fob kept
    /// them.
    #[serde(default)]
    pub(super) tx_key: Option<String>,
    #[serde(default)]
    pub(super) extra_key: Option<String>,
}

/// A transaction on the chain that spent the wallet's outputs.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ChainSpend {
    pub(super) tx: String,
    pub(super) height: u64,
    pub(super) time: u64,
    /// 0 when not known: found through a light wallet server that gives no
    /// fees. Every transaction that spends pays one.
    pub(super) fee: u64,
}

/// A payment this wallet sent, with what only the sender knows.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Sent {
    pub(super) tx: String,
    pub(super) to: String,
    pub(super) amount: u64,
    pub(super) fee: u64,
    pub(super) change: u64,
    pub(super) time: u64,
    pub(super) height: Option<u64>,
    /// The ORF receipt JSON from the payment code.
    pub(super) receipt: Option<String>,
    /// The payment code's description (`tx_description`), if it had one.
    #[serde(default)]
    pub(super) note: Option<String>,
    /// How many pieces, for a split: money paid back to this wallet's
    /// split address. `amount` is then the pieces' total, and `change`
    /// everything that comes back, pieces and change.
    #[serde(default)]
    pub(super) pieces: Option<u32>,
    /// A day on, and the node has never heard of it. Its coins stay spent
    /// until someone releases them.
    #[serde(default)]
    pub(super) not_confirmed: bool,
    /// Its coins were released: as far as anyone knows, it didn't go through.
    #[serde(default)]
    pub(super) released: bool,
    /// Each output coming back to this wallet (a split's pieces, then the
    /// change), as sent, so they show one by one before the payment is
    /// mined, as they will after. Empty in older files: `change` as one.
    #[serde(default)]
    pub(super) back: Vec<u64>,
    /// The name in the payment code (`recipient_name`), cleaned: who the
    /// code said it was, not checked by anyone.
    #[serde(default)]
    pub(super) recipient_name: Option<String>,
    /// The signed transaction, in hex, while the node may not have it: the
    /// node gave no answer when it was sent (it timed out, the line dropped),
    /// so it's offered again with each sync until the node takes it or turns
    /// it down. `None` once it has, and for every payment the node answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) signed: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct State {
    pub(super) node: String,
    /// The node's certificate is accepted even if self-signed (not
    /// verified, still encrypted). Off in older files.
    #[serde(default)]
    pub(super) node_self_signed: bool,
    /// For a node whose self-signed certificate is accepted: the fingerprint
    /// of its key, in hex, from the first connection to it, which every later
    /// one must match. Choosing a node, this one again included, forgets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) node_pin: Option<String>,
    /// Nodes this wallet was given, besides its network's own: kept to change
    /// back to. Each URL once, as [`node::normalize`] leaves it.
    #[serde(default)]
    pub(super) custom_nodes: Vec<node::Setting>,
    /// The GPU and driver this wallet's GPU check last ran on, and whether it
    /// passed. A pass isn't run again on them; a failure is, once per opening.
    #[serde(default)]
    pub(super) gpu_checked_on: Option<String>,
    #[serde(default)]
    pub(super) gpu_check_passed: bool,
    /// Only scan when asked. Off by default.
    #[serde(default)]
    pub(super) manual_sync: bool,
    #[serde(default)]
    pub(super) pull_mode: PullMode,
    #[serde(default = "default_incoming_scan_blocks")]
    pub(super) incoming_scan_blocks: u32,
    #[serde(default = "default_settled_confirmations")]
    pub(super) settled_confirmations: u32,
    /// The newest block the last pull saw, to tell if the chain has changed since.
    #[serde(default)]
    pub(super) pull_mark_height: Option<u64>,
    #[serde(default)]
    pub(super) pull_mark_hash: Option<String>,
    /// The node's ZMQ publisher (`tcp://host:port`, on the node's host), as
    /// last set; kept while turned off, so turning it on again uses it.
    #[serde(default)]
    pub(super) zmq_endpoint: Option<String>,
    /// Instant updates on. Off by default.
    #[serde(default)]
    pub(super) zmq_on: bool,
    /// Where syncing comes from: the node, or a light wallet server.
    #[serde(default)]
    pub(super) sync_mode: SyncMode,
    /// The light wallet server, as last chosen; kept while syncing through
    /// the node, so switching back uses it.
    #[serde(default)]
    pub(super) light_url: Option<String>,
    #[serde(default)]
    pub(super) light_self_signed: bool,
    /// For a server whose self-signed certificate is accepted: its key's
    /// fingerprint from the first answer, as `node_pin` is for the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) light_pin: Option<String>,
    /// The server has taken this wallet (`/login` answered).
    #[serde(default)]
    pub(super) light_registered: bool,
    /// The highest minor of account 0 the server has been told to watch.
    #[serde(default)]
    pub(super) light_subaddrs_sent: u32,
    /// The last block the server had scanned at the last poll.
    #[serde(default)]
    pub(super) light_scanned_height: u64,
    /// First block to scan. `None` until the first sync of a new wallet,
    /// which starts at the chain's tip.
    pub(super) restore_height: Option<u64>,
    pub(super) next_block: u64,
    /// Blocks on the chain at the last look.
    pub(super) tip: u64,
    pub(super) last_block_time: u64,
    pub(super) recent: Vec<(u64, String)>,
    /// The highest subaddress handed out (to Receive, a request or splits).
    pub(super) receive_index: u32,
    /// The subaddress Receive shows, by index; `None` in files from before
    /// requests, where it was `receive_index`.
    #[serde(default)]
    pub(super) receive_own: Option<u32>,
    pub(super) outputs: Vec<Owned>,
    pub(super) spends: Vec<ChainSpend>,
    pub(super) sent: Vec<Sent>,
    /// How prices are shown; in dollars, from Kraken over the internet for
    /// a file from before prices.
    #[serde(default)]
    pub(super) prices: price::Settings,
    /// The last current price and the daily histories asked for.
    #[serde(default)]
    pub(super) price_cache: price::Cache,
    /// What XMR was worth when each payment moved, by transaction: one
    /// record per currency.
    #[serde(default)]
    pub(super) price_records: HashMap<String, Vec<fiat::Record>>,
    /// The note last put in a payment code for each address, by its index:
    /// what payments to that address are for.
    #[serde(default)]
    pub(super) notes: HashMap<u32, String>,
    /// The address splits pay their pieces to, by index: taken from the
    /// next unused one the first time, never handed out on Receive.
    #[serde(default)]
    pub(super) split_index: Option<u32>,
    /// The people this wallet pays.
    #[serde(default)]
    pub(super) contacts: contacts::Book,
    /// Requests to be paid, each on its own subaddress.
    #[serde(default)]
    pub(super) requests: requests::Book,
    /// Your name and photo, and every share of them.
    #[serde(default)]
    pub(super) profile: profile::Book,
    /// Restored from words or a code: another phone may have handed out
    /// addresses this file doesn't know.
    #[serde(default)]
    pub(super) restored: bool,
    /// The first address this restored wallet handed out: payments found
    /// below it went to the other phone's.
    #[serde(default)]
    pub(super) fresh_from: Option<u32>,
    /// Made here, and its seed phrase not yet said to be written down: Home
    /// reminds until it is, even if Fob was closed during the backup step.
    #[serde(default)]
    pub(super) backup_pending: bool,
}

/// Where a wallet's syncing comes from, as the wallet file says it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SyncMode {
    #[default]
    Node,
    Light,
}

/// An unlock time from here on is a Unix time, not a block (Monero's
/// `CRYPTONOTE_MAX_BLOCK_NUMBER`).
pub(super) const TIME_LOCK_FROM: u64 = 500_000_000;

/// A block's time, in seconds, as Monero reckons a time lock in blocks.
const BLOCK_SECONDS: u64 = 120;

impl Owned {
    /// Confirmations before it can be spent: 10 (60 for a mined reward), or
    /// more if its unlock time names a later block. Monero lets an output
    /// with unlock time `b` be spent once the chain has `b` blocks. A time
    /// lock is [`State::needed`]'s, which knows the chain's time.
    pub(super) fn needed(&self) -> u64 {
        let window = if self.coinbase { COINBASE_UNLOCK } else { UNLOCK };
        let block = if self.unlock_block >= TIME_LOCK_FROM { 0 } else { self.unlock_block };
        window.max(block.saturating_sub(self.height))
    }

    /// The Unix time it's locked until, if it's locked to a time.
    pub(super) fn unlock_time(&self) -> Option<u64> {
        (self.unlock_block >= TIME_LOCK_FROM).then_some(self.unlock_block)
    }

    /// Whether this is the coin `id` names: its transaction and the output
    /// as kept, which says where on the chain it is. A payment names its
    /// coins this way, not by their place in the wallet's list, which moves
    /// when a rescan forgets coins and finds them again.
    pub(super) fn is(&self, id: &(String, String)) -> bool {
        self.tx == id.0 && self.output == id.1
    }

    /// How it's named in a payment: see [`is`](Owned::is).
    pub(super) fn id(&self) -> (String, String) {
        (self.tx.clone(), self.output.clone())
    }
}

pub(super) fn confirmations(state: &State, height: u64) -> u64 {
    state.tip.saturating_sub(height)
}

pub(super) fn default_incoming_scan_blocks() -> u32 {
    5
}

pub(super) fn default_settled_confirmations() -> u32 {
    manual::SETTLED_MIN
}

impl State {
    pub(super) fn manual(&self) -> ManualSettings {
        ManualSettings {
            on: self.manual_sync,
            pull_mode: self.pull_mode,
            incoming_scan_blocks: self.incoming_scan_blocks,
            settled_confirmations: self.settled_confirmations,
        }
    }

    /// How long `output` waits before it can be spent. One locked to a time
    /// waits until the last block scanned is that late, the chain's clock and
    /// not the phone's, counted as the blocks that time is from it (so it
    /// shows as not ready yet, with about how long). monerod judges it by
    /// its own clock plus two hours; a block's time can be at most two hours
    /// ahead of that, so this is never early.
    pub(super) fn needed(&self, output: &Owned) -> u64 {
        let rules = match output.unlock_time() {
            Some(time) => {
                let left = time.saturating_sub(self.last_block_time).div_ceil(BLOCK_SECONDS);
                output.needed().max(if left == 0 { 0 } else { confirmations(self, output.height) + left })
            }
            None => output.needed(),
        };
        manual::needed(rules, self.manual())
    }

    /// Manual Sync holds scanning until asked: on, and syncing from the node.
    /// Through a light wallet server there's no scanning to hold, so it's moot.
    pub(super) fn manual_scanning(&self) -> bool {
        self.manual_sync && self.sync_mode == SyncMode::Node
    }

    /// What can be spent now.
    pub(super) fn spendable(&self) -> u64 {
        self.outputs.iter().filter(|o| o.spent_in.is_none() && confirmations(self, o.height) >= self.needed(o)).map(|o| o.amount).sum()
    }

    /// A scan from before the wallet's first block makes that its first block.
    pub(super) fn lower_start(&mut self, from: u64) {
        if self.restore_height.is_none_or(|start| from < start) {
            self.restore_height = Some(from);
        }
    }
}
