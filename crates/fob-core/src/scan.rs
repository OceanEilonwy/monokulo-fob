//! Scanning a batch of blocks on several cores, with the wallet's state left
//! free meanwhile.
//!
//! Finding what's yours is the slow part of catching up on a phone: mainnet's
//! blocks carry about 135 outputs each, and each costs a scalar multiplication.
//! Like wallet2's thread pool, this spreads it over the cores: each thread
//! takes the next block when it's done with the last, so a big core does more
//! than a little one. It works from a clone of the scanner and hands back what
//! it found for the sync step to apply, in block order, under the lock.
//!
//! [`fob_scan::Prefilter`] goes first and finds the few blocks that could hold
//! the wallet's money. Only those cost a scanner's time.

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use fob_scan::Prefilter;
use monero_wallet::{Scanner, WalletOutput, block::Block, extra::Extra, interface::ScannableBlock, transaction::{Input, Transaction}};

/// Fewer blocks than this aren't worth starting threads for: a wallet near the
/// tip sees one or two at a time.
const WORTH_THREADS: usize = 8;
/// The most threads a scan takes, whatever the phone has.
const MAX_THREADS: usize = 8;

/// A transaction in a scanned block, as much as the wallet needs to notice its
/// own: its hash, fee and the key images it spends.
pub(crate) struct SeenTx {
    pub hash: String,
    pub fee: u64,
    pub key_images: Vec<String>,
}

/// The public keys a transaction's extra carries: the transaction keys,
/// and the extra ones (one per output) if it has those. Without torsion,
/// as a hardware wallet takes them.
pub(crate) type TxKeys = (Vec<[u8; 32]>, Option<Vec<[u8; 32]>>);

/// One block, scanned.
pub(crate) struct Scanned {
    pub height: u64,
    pub time: u64,
    pub hash: String,
    pub miner_tx: String,
    pub txs: Vec<SeenTx>,
    pub found: Vec<WalletOutput>,
    /// For each transaction something was found in, its keys.
    pub tx_keys: HashMap<String, TxKeys>,
}

/// The keys in `extra`, as [`TxKeys`].
pub(crate) fn tx_keys(extra: &[u8]) -> Option<TxKeys> {
    let (keys, additional) = Extra::read(&mut &extra[..]).ok()?.keys()?;
    let clean = |points: Vec<monero_wallet::ed25519::Point>| points.into_iter().map(|p| crate::trezor::clear_torsion(p.into()).compress().to_bytes()).collect::<Vec<_>>();
    Some((clean(keys), additional.map(clean)))
}

fn scan_block(scanner: &mut Scanner, block: ScannableBlock, flagged: bool) -> Result<Scanned, String> {
    let height = block.block.number() as u64;
    let time = block.block.header.timestamp;
    let hash = hex::encode(block.block.hash());
    let miner_tx = hex::encode(block.block.miner_transaction().hash());
    let txs = block
        .block
        .transactions
        .iter()
        .zip(&block.transactions)
        .map(|(hash, tx)| SeenTx {
            hash: hex::encode(hash),
            fee: match tx {
                Transaction::V2 { proofs: Some(proofs), .. } => proofs.base.fee,
                _ => 0,
            },
            key_images: tx
                .prefix()
                .inputs
                .iter()
                .filter_map(|input| match input {
                    Input::ToKey { key_image, .. } => Some(hex::encode(key_image.to_bytes())),
                    _ => None,
                })
                .collect(),
        })
        .collect();
    let (found, tx_keys) = if flagged {
        // Each transaction's extra, kept aside: the scan takes the block.
        let mut extras: Vec<(String, Vec<u8>)> = vec![(miner_tx.clone(), block.block.miner_transaction().prefix().extra.clone())];
        extras.extend(block.block.transactions.iter().zip(&block.transactions).map(|(hash, tx)| (hex::encode(hash), tx.prefix().extra.clone())));
        let found = scanner.scan(block).map_err(|e| format!("scanning block {height}: {e:?}"))?.ignore_additional_timelock();
        let mut tx_keys = HashMap::new();
        for output in &found {
            let tx = hex::encode(output.transaction());
            if tx_keys.contains_key(&tx) {
                continue;
            }
            if let Some(keys) = extras.iter().find(|(h, _)| *h == tx).and_then(|(_, extra)| self::tx_keys(extra)) {
                tx_keys.insert(tx, keys);
            }
        }
        (found, tx_keys)
    } else {
        (vec![], HashMap::new())
    };
    Ok(Scanned { height, time, hash, miner_tx, txs, found, tx_keys })
}

/// How many threads to scan `blocks` blocks on, of `cores`: one is left for the
/// screen.
fn threads(blocks: usize, cores: usize) -> usize {
    if blocks < WORTH_THREADS { 1 } else { cores.saturating_sub(1).clamp(1, MAX_THREADS).min(blocks) }
}

/// Scans `blocks`, giving each one's result in the order given, and the last
/// block for the pool's stand-in. Without `gpu_allowed` the CPU does the filtering.
pub(crate) fn scan(scanner: &Scanner, prefilter: &Prefilter, blocks: Vec<ScannableBlock>, gpu_allowed: bool) -> (Vec<Result<Scanned, String>>, Option<Block>) {
    let last = blocks.last().map(|b| b.block.clone());
    let count = blocks.len();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = threads(count, cores);
    let flags = prefilter.flag(&blocks, threads, gpu_allowed);
    if threads == 1 {
        let mut scanner = scanner.clone();
        return (blocks.into_iter().zip(flags).map(|(block, flagged)| scan_block(&mut scanner, block, flagged)).collect(), last);
    }
    let waiting: Vec<_> = blocks.into_iter().map(|block| Mutex::new(Some(block))).collect();
    let done: Vec<Mutex<Option<Result<Scanned, String>>>> = (0..count).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let mut scanner = scanner.clone();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(slot) = waiting.get(i) else { break };
                    let block = slot.lock().expect("scan slot").take().expect("each block is taken once");
                    *done[i].lock().expect("scan slot") = Some(scan_block(&mut scanner, block, flags[i]));
                }
            });
        }
    });
    let results = done.into_iter().map(|slot| slot.into_inner().expect("scan slot").expect("every block is scanned")).collect();
    (results, last)
}

#[cfg(test)]
mod tests {
    use super::*;

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
