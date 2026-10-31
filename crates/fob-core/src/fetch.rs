//! Catching up fast: blocks from `get_blocks.bin`, many at a time, several
//! requests in flight.
//!
//! monero-oxide reads `get_blocks.bin` too, but gives up on a response whose
//! transactions carry an all-zero `prunable_hash` (monero-project/monero#10120,
//! which public nodes send) and falls back to two or three JSON calls per
//! block: about a block a second. A wallet doesn't need the prunable hash:
//! the block lists its transactions' hashes, so this reader accepts it, like
//! wallet2 does. The request and parsing follow monero-oxide's
//! `monero-daemon-rpc` (MIT), which this is adapted from.
//!
//! The chunked, pipelined fetching follows UkoeHB's async scanner
//! (UkoeHB/monero#23): small chunks keep progress smooth, several in flight
//! hide the round trips.

use monero_daemon_rpc::HttpTransport;
use monero_epee::{Epee, HEADER, Type, VERSION};
use monero_wallet::{
    block::Block,
    interface::{InterfaceError, ScannableBlock},
    transaction::{Pruned, Transaction},
};

use crate::transport::Transport;

/// Retries of one request before a sync step gives up.
const RETRIES: u32 = 5;
/// The most a node may send in one response.
const MAX_RESPONSE: usize = 100 * 1024 * 1024;

fn invalid(what: impl Into<String>) -> InterfaceError {
    InterfaceError::InvalidInterface(what.into())
}

fn epee_err(e: monero_epee::EpeeError) -> InterfaceError {
    invalid(format!("get_blocks.bin: {e:?}"))
}

fn key(request: &mut Vec<u8>, name: &str, kind: Type) {
    request.push(name.len() as u8);
    request.extend(name.as_bytes());
    request.push(kind as u8);
}

/// `get_blocks.bin` for up to `count` blocks from `start` (which must not be
/// 0: monerod ignores a zero start height).
fn request(start: u64, count: u64) -> Vec<u8> {
    let mut request = Vec::with_capacity(80);
    request.extend(HEADER);
    request.push(VERSION);
    // Three fields, as an epee varint.
    request.push(3 << 2);
    key(&mut request, "prune", Type::Bool);
    request.push(1);
    key(&mut request, "start_height", Type::Uint64);
    request.extend(start.to_le_bytes());
    key(&mut request, "max_block_count", Type::Uint64);
    request.extend(count.to_le_bytes());
    request
}

/// The blocks in one `get_blocks.bin` response, with their pruned
/// transactions and each block's first RingCT output index.
fn parse(response: &[u8]) -> Result<Vec<ScannableBlock>, InterfaceError> {
    let mut epee = Epee::new(response).map_err(epee_err)?;
    let mut fields = epee.entry().map_err(epee_err)?.fields().map_err(epee_err)?;
    let mut blocks = vec![];
    let mut output_indexes = vec![];
    let mut status_ok = false;
    while let Some(field) = fields.next() {
        let (name, value) = field.map_err(epee_err)?;
        match name.consume() {
            b"status" => status_ok = value.to_str().map_err(epee_err)?.consume() == b"OK",
            b"blocks" => {
                let mut entries = value.iterate().map_err(epee_err)?;
                while let Some(entry) = entries.next() {
                    let mut entry = entry.map_err(epee_err)?.fields().map_err(epee_err)?;
                    let mut block = None;
                    let mut transactions = vec![];
                    while let Some(field) = entry.next() {
                        let (name, value) = field.map_err(epee_err)?;
                        match name.consume() {
                            b"block" => {
                                let mut bytes = value.to_str().map_err(epee_err)?.consume();
                                block = Some(Block::read(&mut bytes).map_err(|e| invalid(format!("block: {e:?}")))?);
                            }
                            b"txs" => {
                                let mut txs = value.iterate().map_err(epee_err)?;
                                while let Some(tx) = txs.next() {
                                    let mut tx = tx.map_err(epee_err)?.fields().map_err(epee_err)?;
                                    while let Some(field) = tx.next() {
                                        let (name, value) = field.map_err(epee_err)?;
                                        if name.consume() == b"blob" {
                                            let mut bytes = value.to_str().map_err(epee_err)?.consume();
                                            transactions.push(
                                                Transaction::<Pruned>::read(&mut bytes).map_err(|e| invalid(format!("transaction: {e:?}")))?,
                                            );
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    let block = block.ok_or_else(|| invalid("a block entry without its block"))?;
                    if block.transactions.len() != transactions.len() {
                        return Err(invalid("a block's transactions are missing"));
                    }
                    blocks.push((block, transactions));
                }
            }
            b"output_indices" => {
                let mut per_block = value.iterate().map_err(epee_err)?;
                while let Some(block) = per_block.next() {
                    let mut block_fields = block.map_err(epee_err)?.fields().map_err(epee_err)?;
                    while let Some(field) = block_fields.next() {
                        let (name, value) = field.map_err(epee_err)?;
                        if name.consume() != b"indices" {
                            continue;
                        }
                        let mut per_tx = value.iterate().map_err(epee_err)?;
                        while let Some(tx) = per_tx.next() {
                            let mut tx_fields = tx.map_err(epee_err)?.fields().map_err(epee_err)?;
                            while let Some(field) = tx_fields.next() {
                                let (name, value) = field.map_err(epee_err)?;
                                if name.consume() != b"indices" {
                                    continue;
                                }
                                let mut indexes = value.iterate().map_err(epee_err)?;
                                while let Some(index) = indexes.next() {
                                    output_indexes.push(index.map_err(epee_err)?.to_u64().map_err(epee_err)?);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if !status_ok {
        return Err(InterfaceError::InterfaceError("get_blocks.bin: the node's status wasn't OK".into()));
    }

    // Output indexes come flattened, miner transaction first in each block;
    // keep the first index of a RingCT output per block.
    let mut indexes = output_indexes.as_slice();
    let mut res = Vec::with_capacity(blocks.len());
    for (block, transactions) in blocks {
        let mut first_ringct = None;
        let miner: Transaction<Pruned> = block.miner_transaction().clone().into();
        for tx in std::iter::once(&miner).chain(&transactions) {
            let outputs = tx.prefix().outputs.len();
            if indexes.len() < outputs {
                return Err(invalid("get_blocks.bin left out output indexes"));
            }
            if !matches!(tx, Transaction::V1 { .. }) && outputs != 0 {
                first_ringct = first_ringct.or(Some(indexes[0]));
            }
            indexes = &indexes[outputs..];
        }
        res.push(ScannableBlock { block, transactions, output_index_for_first_ringct_output: first_ringct });
    }
    if !indexes.is_empty() {
        return Err(invalid("get_blocks.bin sent more output indexes than outputs"));
    }
    Ok(res)
}

/// How the fetcher sizes and spreads its requests. One set of values, used
/// in the app and in tests alike.
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    /// Response size to aim each request at. Blocks vary a lot: stagenet's
    /// are a few kB, mainnet's 50-100 kB pruned.
    pub target_bytes: usize,
    pub min_chunk: u64,
    pub max_chunk: u64,
    /// Blocks per request before the first response says how big they are:
    /// few, so a slow node with full mainnet blocks answers quickly.
    pub first_chunk: u64,
    /// Requests in flight at the start.
    pub start_parallel: usize,
    /// Never more than this many at once, however well the node keeps up.
    /// Public nodes cap connections per IP (monerod resets the fourth or
    /// fifth); a node of your own takes all of them.
    pub max_parallel: usize,
    /// Chunks fetched ahead of the scanner.
    pub queue: usize,
}

impl Tuning {
    pub const DEFAULT: Tuning = Tuning {
        target_bytes: 2 * 1024 * 1024,
        min_chunk: 10,
        max_chunk: 500,
        first_chunk: 20,
        start_parallel: 3,
        max_parallel: 8,
        queue: 6,
    };
}

/// What the fetcher has learned about a node: how big its blocks are and how
/// many requests it takes at once. Shared by every fetch from that node.
pub struct Adaptive {
    tuning: Tuning,
    state: std::sync::Mutex<Learned>,
}

struct Learned {
    parallel: usize,
    /// The level a node last refused: don't climb back up to it.
    ceiling: usize,
    bytes_per_block: Option<f64>,
    good_in_a_row: usize,
}

impl Adaptive {
    pub fn new(tuning: Tuning) -> Adaptive {
        Adaptive {
            tuning,
            state: std::sync::Mutex::new(Learned {
                parallel: tuning.start_parallel,
                ceiling: tuning.max_parallel,
                bytes_per_block: None,
                good_in_a_row: 0,
            }),
        }
    }

    fn learned(&self) -> std::sync::MutexGuard<'_, Learned> {
        self.state.lock().expect("fetch tuning lock")
    }

    pub fn parallel(&self) -> usize {
        self.learned().parallel
    }

    /// Blocks to ask for next, so the response lands near `target_bytes`.
    pub fn chunk(&self) -> u64 {
        let t = self.tuning;
        match self.learned().bytes_per_block {
            Some(bpb) => ((t.target_bytes as f64 / bpb.max(1.0)) as u64).clamp(t.min_chunk, t.max_chunk),
            None => t.first_chunk,
        }
    }

    fn record_ok(&self, blocks: usize, bytes: usize) {
        let mut l = self.learned();
        if blocks > 0 {
            let this = bytes as f64 / blocks as f64;
            l.bytes_per_block = Some(match l.bytes_per_block {
                Some(avg) => avg * 0.7 + this * 0.3,
                None => this,
            });
        }
        // A full window's worth of answers without trouble: try one more.
        l.good_in_a_row += 1;
        if l.good_in_a_row >= 2 * l.parallel && l.parallel < l.ceiling {
            l.parallel += 1;
            l.good_in_a_row = 0;
        }
    }

    /// The node dropped or refused a connection: one request fewer, and stay below that.
    fn record_refused(&self) {
        let mut l = self.learned();
        l.ceiling = l.parallel.saturating_sub(1).max(1);
        l.parallel = l.ceiling;
        l.good_in_a_row = 0;
    }
}

/// Blocks `start..start + count`, from as many requests as the node needs,
/// with the response's size in bytes.
async fn chunk(transport: &Transport, adaptive: &Adaptive, start: u64, count: u64) -> Result<(Vec<ScannableBlock>, usize), InterfaceError> {
    let mut blocks: Vec<ScannableBlock> = Vec::with_capacity(count as usize);
    let mut bytes = 0;
    while (blocks.len() as u64) < count {
        let from = start + blocks.len() as u64;
        let left = count - blocks.len() as u64;
        let mut attempt = 0;
        let got = loop {
            let result = match transport.post("get_blocks.bin", request(from, left), Some(MAX_RESPONSE)).await {
                Ok(response) => {
                    bytes += response.len();
                    transport.count_received(response.len());
                    parse(&response)
                }
                Err(e) => Err(e),
            };
            match result {
                Err(InterfaceError::InterfaceError(e)) if attempt < RETRIES => {
                    attempt += 1;
                    if stale(&e) {
                        // A kept-alive connection the node had already closed:
                        // not the node pushing back, so just try again.
                        continue;
                    }
                    // A busy public node drops connections: fewer at once, and try again.
                    adaptive.record_refused();
                    tokio::time::sleep(std::time::Duration::from_millis(250 << attempt)).await;
                }
                other => break other?,
            }
        };
        if got.is_empty() {
            return Err(invalid(format!("the node sent no blocks from {from}")));
        }
        // Older nodes ignore max_block_count and send more: keep what was asked.
        blocks.extend(got.into_iter().take(left as usize));
    }
    Ok((blocks, bytes))
}

/// An error from reusing a connection the node had closed while it sat idle.
pub(crate) fn stale(error: &str) -> bool {
    error.contains("connection closed before message completed") || error.contains("IncompleteMessage")
}

/// A spawned task that's aborted when dropped, so a dropped pipeline leaves
/// no request running.
struct Owned<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for Owned<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

type Chunk = Result<Vec<ScannableBlock>, InterfaceError>;
/// One request's blocks, with the response's size in bytes.
type Fetched = Result<(Vec<ScannableBlock>, usize), InterfaceError>;

/// Blocks fetched ahead of the scanner, in order.
///
/// A task keeps up to [`Adaptive::parallel`] requests in flight and hands
/// finished chunks over in order through a short queue, so fetching goes on
/// while the wallet scans and saves the previous blocks, and a slow request
/// only holds up the ones after it.
pub struct Pipeline {
    /// The first block not yet handed over.
    pub next: u64,
    /// The last block this pipeline fetches.
    pub end: u64,
    rx: tokio::sync::mpsc::Receiver<Chunk>,
    /// The last block handed over, to check the next one follows it.
    last: Option<[u8; 32]>,
    _task: Owned<()>,
}

impl Pipeline {
    /// Fetches `start..=end` (`start` at least 1).
    pub fn start(transport: Transport, adaptive: std::sync::Arc<Adaptive>, start: u64, end: u64) -> Pipeline {
        let (tx, rx) = tokio::sync::mpsc::channel(adaptive.tuning.queue.max(1));
        let task = tokio::spawn(async move {
            let mut window: std::collections::VecDeque<Owned<Fetched>> = Default::default();
            let mut from = start;
            loop {
                while window.len() < adaptive.parallel() && from <= end {
                    let count = adaptive.chunk().min(end - from + 1);
                    let (transport, adaptive) = (transport.clone(), adaptive.clone());
                    window.push_back(Owned(tokio::spawn(async move { chunk(&transport, &adaptive, from, count).await })));
                    from += count;
                }
                let Some(mut next) = window.pop_front() else { break };
                let done = match (&mut next.0).await {
                    Ok(Ok((blocks, bytes))) => {
                        adaptive.record_ok(blocks.len(), bytes);
                        Ok(blocks)
                    }
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(InterfaceError::InternalError(e.to_string())),
                };
                let failed = done.is_err();
                if tx.send(done).await.is_err() || failed {
                    break;
                }
            }
        });
        Pipeline { next: start, end, rx, last: None, _task: Owned(task) }
    }

    /// The next blocks in order: at least one chunk, plus whatever else has
    /// arrived, up to about `max` blocks. Empty once everything up to `end`
    /// has been handed over.
    pub async fn next_batch(&mut self, max: usize) -> Result<Vec<ScannableBlock>, InterfaceError> {
        let mut blocks = match self.rx.recv().await {
            Some(chunk) => chunk?,
            None => return Ok(vec![]),
        };
        while blocks.len() < max {
            match self.rx.try_recv() {
                Ok(chunk) => blocks.extend(chunk?),
                Err(_) => break,
            }
        }
        for block in &blocks {
            if block.block.number() as u64 != self.next {
                return Err(invalid("the node sent blocks out of order"));
            }
            if self.last.is_some_and(|last| block.block.header.previous != last) {
                return Err(invalid("the node sent blocks that don't follow on"));
            }
            self.last = Some(block.block.hash());
            self.next += 1;
        }
        Ok(blocks)
    }
}

/// Blocks `start..=end` in order: a [`Pipeline`] read to the end.
pub async fn blocks(transport: &Transport, start: u64, end: u64) -> Result<Vec<ScannableBlock>, InterfaceError> {
    if start == 0 {
        return Err(InterfaceError::InternalError("get_blocks.bin can't start at block 0".into()));
    }
    let adaptive = std::sync::Arc::new(Adaptive::new(Tuning::DEFAULT));
    let mut pipeline = Pipeline::start(transport.clone(), adaptive, start, end);
    let mut res = Vec::with_capacity((end - start + 1) as usize);
    loop {
        let batch = pipeline.next_batch(usize::MAX).await?;
        if batch.is_empty() {
            return Ok(res);
        }
        res.extend(batch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_is_the_epee_monero_oxide_sends() {
        let r = request(1_000, 20);
        assert_eq!(&r[..HEADER.len()], HEADER);
        // header, version, field count, then "prune" true, "start_height" and "max_block_count".
        assert_eq!(r.len(), HEADER.len() + 2 + (1 + 5 + 1 + 1) + (1 + 12 + 1 + 8) + (1 + 15 + 1 + 8));
        assert!(r.windows(15).any(|w| w == b"max_block_count"));
        assert!(r.windows(8).any(|w| w == 1_000u64.to_le_bytes()));
    }

    #[test]
    fn chunks_aim_at_the_target_size_within_bounds() {
        let a = Adaptive::new(Tuning::DEFAULT);
        assert_eq!(a.chunk(), Tuning::DEFAULT.first_chunk);
        a.record_ok(100, 100 * 80_000); // mainnet-sized blocks
        assert_eq!(a.chunk(), (2 * 1024 * 1024 / 80_000) as u64);
        let small = Adaptive::new(Tuning::DEFAULT);
        small.record_ok(100, 100 * 500); // tiny stagenet blocks
        assert_eq!(small.chunk(), Tuning::DEFAULT.max_chunk);
    }

    #[test]
    fn parallelism_climbs_while_the_node_keeps_up_and_backs_off_for_good() {
        let a = Adaptive::new(Tuning::DEFAULT);
        assert_eq!(a.parallel(), 3);
        for _ in 0..6 {
            a.record_ok(10, 10);
        }
        assert_eq!(a.parallel(), 4);
        a.record_refused();
        assert_eq!(a.parallel(), 3);
        for _ in 0..100 {
            a.record_ok(10, 10);
        }
        assert_eq!(a.parallel(), 3, "never back up to a level the node refused");
    }

    #[test]
    fn a_response_that_isnt_epee_is_refused() {
        assert!(parse(b"hello").is_err());
    }
}
