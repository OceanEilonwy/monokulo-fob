//! Scanning a batch of blocks on several cores, with the wallet's state left
//! free meanwhile.
//!
//! Finding what's yours is the slow part of catching up on a phone: mainnet's
//! blocks carry about 135 outputs each, and each costs a scalar multiplication.
//! Like wallet2's thread pool, this spreads it over the cores: each thread
//! takes the next block when it's done with the last, so a big core does more
//! than a little one. It hands back what it found for the sync step to apply, in
//! block order, under the lock.
//!
//! [`crate::Prefilter`] goes first, on the GPU, and finds the few blocks that
//! could hold the wallet's money. Only those cost a scanner's time. When it
//! doesn't filter (no GPU that can, or too small a batch), every block does.

use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use std::collections::HashMap;

use crate::Prefilter;
use monero_wallet::{Scanner, WalletOutput, block::Block, extra::Extra, interface::ScannableBlock, transaction::{Input, Transaction}};

/// Fewer blocks than this aren't worth starting threads for: a wallet near the
/// tip sees one or two at a time.
const WORTH_THREADS: usize = 8;
/// The most threads a scan takes, whatever the phone has.
const MAX_THREADS: usize = 16;
/// One thread reads blocks beside the filter for this many it may use.
const READERS_PER_THREAD: usize = 4;

/// A transaction in a scanned block, as much as the wallet needs to notice its
/// own: its hash, fee and the key images it spends. In bytes, as nearly all of
/// them are looked up and dropped.
pub struct SeenTx {
    pub hash: [u8; 32],
    pub fee: u64,
    pub key_images: Vec<[u8; 32]>,
}

/// The public keys a transaction's extra carries: the transaction keys,
/// and the extra ones (one per output) if it has those. Without torsion,
/// as a hardware wallet takes them.
pub type TxKeys = (Vec<[u8; 32]>, Option<Vec<[u8; 32]>>);

/// One block, scanned.
pub struct Scanned {
    pub height: u64,
    pub time: u64,
    pub hash: String,
    /// The miner transaction's hash, only for a block that may hold money.
    pub miner_tx: String,
    pub txs: Vec<SeenTx>,
    pub found: Vec<WalletOutput>,
    /// For each transaction something was found in, its keys.
    pub tx_keys: HashMap<String, TxKeys>,
}

/// The keys in `extra`, as [`TxKeys`].
pub fn tx_keys(extra: &[u8]) -> Option<TxKeys> {
    let (keys, additional) = Extra::read(&mut &extra[..]).ok()?.keys()?;
    let clean = |points: Vec<monero_wallet::ed25519::Point>| {
        points
            .into_iter()
            .map(|p| {
                // Any torsion component removed, as Monero's pubkey_clear_torsion.
                let point: curve25519_dalek::EdwardsPoint = p.into();
                (point.mul_by_cofactor() * curve25519_dalek::Scalar::from(8u8).invert()).compress().to_bytes()
            })
            .collect::<Vec<_>>()
    };
    Some((clean(keys), additional.map(clean)))
}

/// What's in `block` for anyone, without looking for money: `found` comes later,
/// and only for a block that may hold some.
fn seen(block: &ScannableBlock, hash: &[u8; 32]) -> Scanned {
    let txs = block
        .block
        .transactions
        .iter()
        .zip(&block.transactions)
        .map(|(hash, tx)| SeenTx {
            hash: *hash,
            fee: match tx {
                Transaction::V2 { proofs: Some(proofs), .. } => proofs.base.fee,
                _ => 0,
            },
            key_images: tx
                .prefix()
                .inputs
                .iter()
                .filter_map(|input| match input {
                    Input::ToKey { key_image, .. } => Some(key_image.to_bytes()),
                    _ => None,
                })
                .collect(),
        })
        .collect();
    Scanned { height: block.block.number() as u64, time: block.block.header.timestamp, hash: hex::encode(hash), miner_tx: String::new(), txs, found: vec![], tx_keys: HashMap::new() }
}

/// Finds the money in a block that may hold some. The `Scanner` takes the
/// block, and frees it: a copy, made and freed on this thread, as freeing the
/// fetched block here would fight the other threads over the allocator
/// (decision 350).
fn look_for_money(scanner: &mut Scanner, block: &ScannableBlock, scanned: &mut Scanned) -> Result<(), String> {
    let miner = block.block.miner_transaction().hash();
    scanned.miner_tx = hex::encode(miner);
    let height = scanned.height;
    scanned.found = scanner.scan(block.clone()).map_err(|e| format!("scanning block {height}: {e:?}"))?.ignore_additional_timelock();
    // Each found output's transaction keys, which a hardware wallet needs.
    for output in &scanned.found {
        let hash = output.transaction();
        let tx = hex::encode(hash);
        if scanned.tx_keys.contains_key(&tx) {
            continue;
        }
        let extra = if hash == miner {
            Some(&block.block.miner_transaction().prefix().extra)
        } else {
            block.block.transactions.iter().position(|h| *h == hash).and_then(|i| block.transactions.get(i)).map(|t| &t.prefix().extra)
        };
        if let Some(keys) = extra.and_then(|e| tx_keys(e)) {
            scanned.tx_keys.insert(tx, keys);
        }
    }
    Ok(())
}

/// How many threads to scan `blocks` blocks on, of `cores`: one is left for the
/// screen.
fn threads(blocks: usize, cores: usize) -> usize {
    if blocks < WORTH_THREADS { 1 } else { cores.saturating_sub(1).clamp(1, MAX_THREADS).min(blocks) }
}

/// Which of `count` blocks the `Scanner` reads: those the prefilter flags, or all
/// of them when it didn't filter.
fn to_read(flags: Option<Vec<bool>>, count: usize) -> Vec<bool> {
    flags.unwrap_or_else(|| vec![true; count])
}

/// Scans `blocks`, whose hashes are `hashes`, giving each one's result in the order
/// given, and the last block for the pool's stand-in. Without `gpu_allowed` (this
/// wallet's GPU check hasn't passed) nothing is filtered, and the `Scanner` reads
/// every block; asking still starts the GPU, for later.
pub fn scan(scanner: &Scanner, prefilter: &Prefilter, blocks: Vec<ScannableBlock>, hashes: &[[u8; 32]], gpu_allowed: bool) -> (Vec<Result<Scanned, String>>, Option<Block>) {
    scan_from(scanner, prefilter, blocks, hashes, if gpu_allowed { 0 } else { usize::MAX })
}

/// [`scan`], filtering on the GPU only from `min_keys` transaction keys
/// (`usize::MAX` never filters). For measuring where the GPU starts to pay:
/// `fob-wallet`'s `examples/gpu_crossover.rs`.
pub fn scan_from(scanner: &Scanner, prefilter: &Prefilter, blocks: Vec<ScannableBlock>, hashes: &[[u8; 32]], min_keys: usize) -> (Vec<Result<Scanned, String>>, Option<Block>) {
    let last = blocks.last().map(|b| b.block.clone());
    let count = blocks.len();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = threads(count, cores);
    if threads == 1 {
        let flags = to_read(prefilter.flag(&blocks, threads, min_keys), count);
        let mut clone = None;
        let results = blocks.iter().zip(hashes).zip(flags).map(|((block, hash), flagged)| {
            let mut scanned = seen(block, hash);
            if flagged {
                look_for_money(clone.get_or_insert_with(|| scanner.clone()), block, &mut scanned)?;
            }
            Ok(scanned)
        });
        return (results.collect(), last);
    }

    // Reading the blocks doesn't depend on the filter, so a few threads do it meanwhile.
    let results: Vec<Mutex<Option<Result<Scanned, String>>>> = (0..count).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let flags = std::thread::scope(|scope| {
        for _ in 0..(threads / READERS_PER_THREAD).max(1) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(block) = blocks.get(i) else { break };
                    *results[i].lock().expect("scan slot") = Some(Ok(seen(block, &hashes[i])));
                }
            });
        }
        to_read(prefilter.flag(&blocks, threads, min_keys), count)
    });

    // Almost no block is flagged, and most filtered scans start no threads here.
    let flagged: Vec<usize> = flags.iter().enumerate().filter_map(|(i, flagged)| flagged.then_some(i)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.min(flagged.len()) {
            scope.spawn(|| {
                let mut scanner = scanner.clone();
                while let Some(&i) = flagged.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let mut slot = results[i].lock().expect("scan slot");
                    if let Some(Ok(scanned)) = &mut *slot
                        && let Err(e) = look_for_money(&mut scanner, &blocks[i], scanned)
                    {
                        *slot = Some(Err(e));
                    }
                }
            });
        }
    });
    let results = results.into_iter().map(|slot| slot.into_inner().expect("scan slot").expect("every block is read")).collect();
    // Tens of thousands of small frees, which needn't hold up the next step. Made on
    // the threads reading the blocks, they fought over the allocator and cost more than
    // anything else after the filter. If there's no thread to do them, they're done here.
    let _ = std::thread::Builder::new().name("fob-scanner-free".into()).spawn(move || drop(blocks));
    (results, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfiltered_the_scanner_reads_every_block() {
        assert_eq!(to_read(None, 3), [true, true, true]);
        assert_eq!(to_read(Some(vec![false, true]), 2), [false, true]);
    }

    #[test]
    fn a_few_blocks_stay_on_one_thread_and_many_leave_a_core_free() {
        assert_eq!(threads(1, 8), 1);
        assert_eq!(threads(WORTH_THREADS - 1, 8), 1);
        assert_eq!(threads(25, 8), 7);
        assert_eq!(threads(25, 1), 1);
        assert_eq!(threads(25, 2), 1);
        assert_eq!(threads(1000, 32), MAX_THREADS);
        assert_eq!(threads(WORTH_THREADS, 32), WORTH_THREADS);
    }
}
