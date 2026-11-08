//! A light wallet server's proof that a transaction is in a block
//! ([`lws::Proof`]), checked: the block as Monero serialises it and the
//! transaction's pruned parts, from which Fob works out the transaction's
//! hash, finds it in the block, and reads back what the server said of it
//! (its keys, its one-time keys, commitments, fee, unlock time and time).
//! What comes out is the block's hash at the transaction's height, for a
//! node to vouch for ([`super::hashes`]): a server that holds the view key
//! can make up an output whose key, mask and commitment fit, but not a
//! block whose hash the chain has.
//!
//! Nothing here needs the wallet's keys: the output's key and mask are
//! checked against the view key apart from this (`Keys::light_output` in
//! the wallet), and this binds them to the chain.

use monero_wallet::{
    block::Block,
    extra::Extra,
    primitives::keccak256,
    transaction::{Input, Pruned, Timelock, Transaction},
};

use super::lws;

/// A transaction's proof, checked: where it is, and what the chain holds
/// for each output the server listed, in the order listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proven {
    /// The block's height, from its miner transaction.
    pub height: u64,
    /// The block's hash, for a node to vouch for.
    pub block: [u8; 32],
    /// The block's time.
    pub time: u64,
    /// For each output received, in the order listed: the commitment the
    /// chain holds for it (`None` for a mined reward's, whose amount is in
    /// the clear and was compared already).
    pub commitments: Vec<Option<[u8; 32]>>,
}

/// Why a proof doesn't hold. None of these happens with an honest server:
/// each is the server's answer disagreeing with the bytes it gave as proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broken {
    /// The block, or the transaction's parts, don't read as Monero's.
    Unreadable,
    /// The parts don't hash to the transaction's hash.
    Hash,
    /// The block doesn't hold the transaction (or isn't at its height).
    NotInBlock,
    /// An output's transaction key isn't in the transaction's extra.
    TxKey,
    /// An output's index, one-time key, commitment or (for a mined reward)
    /// amount isn't the transaction's.
    Output,
    /// The fee, the unlock time or the time isn't the transaction's.
    Terms,
}

/// Monero's unlock time as a number, as the API gives it.
fn unlock_time(timelock: &Timelock) -> u64 {
    match timelock {
        Timelock::None => 0,
        Timelock::Block(block) => *block as u64,
        Timelock::Time(time) => *time,
    }
}

/// Checks `tx`'s proof, which it must have, against everything the server
/// said of `tx`: the transaction's hash from its parts (or the block's miner
/// transaction, for a mined reward) is `tx.hash` and in the block, the
/// block is at `tx.height`, each output received has its transaction key in
/// the extra and its one-time key and commitment (or plain amount) at its
/// index, and the fee, unlock time and time are the transaction's. A
/// version 1 transaction (before RingCT) has no pruned hash to check by, so
/// its proof proves nothing: `Ok(None)`, as if there were none.
pub fn check(tx: &lws::Transaction) -> Result<Option<Proven>, Broken> {
    let Some(proof) = &tx.proof else { return Ok(None) };
    let mut bytes = proof.block.0.as_slice();
    let block = Block::read(&mut bytes).map_err(|_| Broken::Unreadable)?;
    if !bytes.is_empty() {
        return Err(Broken::Unreadable);
    }
    if tx.height != Some(block.number() as u64) {
        return Err(Broken::NotInBlock);
    }

    // The transaction itself, pruned, and where it is in the block.
    let miner = block.miner_transaction();
    let pruned: Transaction<Pruned> = match (&proof.tx_prefix, &proof.rct_base, &proof.prunable_hash) {
        (None, None, None) => {
            if miner.hash() != tx.hash.0 || !tx.coinbase {
                return Err(Broken::NotInBlock);
            }
            miner.clone().into()
        }
        (Some(prefix), Some(base), Some(prunable)) => {
            if tx.coinbase {
                return Err(Broken::NotInBlock);
            }
            let whole = [prefix.0.as_slice(), base.0.as_slice()].concat();
            let mut reading = whole.as_slice();
            let pruned = Transaction::<Pruned>::read(&mut reading).map_err(|_| Broken::Unreadable)?;
            if !reading.is_empty() {
                return Err(Broken::Unreadable);
            }
            if pruned.version() == 1 {
                return Ok(None);
            }
            // As Monero hashes a transaction: the prefix's hash, the RingCT
            // base's, and the prunable part's, hashed together. The parts as
            // given, and as read back, so the split between them is the one
            // read.
            let parts = [keccak256(&prefix.0), keccak256(&base.0), prunable.0].concat();
            let hash = keccak256(parts);
            if hash != tx.hash.0 || pruned.hash_with_prunable_hash(prunable.0) != Some(hash) {
                return Err(Broken::Hash);
            }
            if !block.transactions.contains(&hash) {
                return Err(Broken::NotInBlock);
            }
            pruned
        }
        _ => return Err(Broken::Unreadable),
    };
    let is_miner = matches!(pruned.prefix().inputs.first(), Some(Input::Gen(_)));
    if is_miner != tx.coinbase {
        return Err(Broken::NotInBlock);
    }

    // What the server said of it.
    let prefix = pruned.prefix();
    if unlock_time(&prefix.additional_timelock) != tx.unlock_time {
        return Err(Broken::Terms);
    }
    let time = block.header.timestamp;
    if tx.timestamp.as_ref().is_some_and(|t| unix_time(&t.0) != Some(time)) {
        return Err(Broken::Terms);
    }
    let base = match &pruned {
        Transaction::V2 { proofs: Some(proofs), .. } => Some(&proofs.base),
        _ => None,
    };
    let fee = base.map_or(0, |b| b.fee);
    if tx.fee.is_some_and(|f| f.0 != fee) {
        return Err(Broken::Terms);
    }
    let keys = Extra::read(&mut prefix.extra.as_slice()).ok().and_then(|e| e.keys());
    let (main, additional) = keys.ok_or(Broken::TxKey)?;
    let main: Vec<[u8; 32]> = main.iter().map(|k| k.compress().to_bytes()).collect();
    let additional: Vec<[u8; 32]> = additional.unwrap_or_default().iter().map(|k| k.compress().to_bytes()).collect();
    let mut commitments = Vec::with_capacity(tx.receives.len());
    for received in &tx.receives {
        let index = usize::from(received.index);
        let output = prefix.outputs.get(index).ok_or(Broken::Output)?;
        let key = received.tx_pub_key.0;
        if !main.contains(&key) && additional.get(index) != Some(&key) {
            return Err(Broken::TxKey);
        }
        if output.key.to_bytes() != received.public_key.0 {
            return Err(Broken::Output);
        }
        let commitment = match (output.amount, base) {
            // In the clear: a mined reward's, or a version 2 transaction's
            // without RingCT outputs.
            (Some(amount), _) => {
                if amount != received.amount.0 {
                    return Err(Broken::Output);
                }
                None
            }
            (None, Some(base)) => {
                let chain = base.commitments.get(index).ok_or(Broken::Output)?.to_bytes();
                if received.commitment.as_ref().is_some_and(|c| c.0 != chain) {
                    return Err(Broken::Output);
                }
                Some(chain)
            }
            (None, None) => return Err(Broken::Output),
        };
        commitments.push(commitment);
    }
    Ok(Some(Proven { height: block.number() as u64, block: block.hash(), time, commitments }))
}

/// Seconds since the epoch from `YYYY-MM-DDTHH:MM:SSZ`, as the API writes times.
pub fn unix_time(text: &str) -> Option<u64> {
    let (date, time) = text.trim().trim_end_matches('Z').split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(|p| p.split('.').next().unwrap_or("").parse::<i64>());
    let (h, min, s) = (t.next()?.ok()?, t.next()?.ok()?, t.next().and_then(|s| s.ok()).unwrap_or(0));
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + min * 60 + s).ok()
}

/// The proof an honest server gives for `tx` (with its prunable hash) in
/// `block`, or for the block's miner transaction with `None`: for tests.
#[cfg(any(test, feature = "test-support"))]
pub fn make(block: &Block, tx: Option<(&Transaction<Pruned>, [u8; 32])>) -> lws::Proof {
    let (tx_prefix, rct_base, prunable_hash) = match tx {
        None => (None, None, None),
        Some((tx, prunable)) => {
            let whole = tx.serialize();
            let base = match tx {
                Transaction::V2 { proofs: Some(proofs), .. } => {
                    let mut base = vec![];
                    proofs.base.write(&mut base, proofs.rct_type).expect("writing to a Vec");
                    base
                }
                _ => vec![0],
            };
            let prefix = whole[..whole.len() - base.len()].to_vec();
            (Some(lws::HexBytes(prefix)), Some(lws::HexBytes(base)), Some(lws::Hex(prunable)))
        }
    };
    lws::Proof { block: lws::HexBytes(block.serialize()), tx_prefix, rct_base, prunable_hash }
}

#[cfg(test)]
mod tests;
