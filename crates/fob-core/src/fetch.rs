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

/// Blocks asked for per request.
pub const CHUNK: u64 = 100;
/// Requests in flight at once.
pub const PARALLEL: usize = 3;
/// Retries of one request before a sync step gives up.
const RETRIES: u32 = 3;
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

/// Blocks `start..start + count`, from as many requests as the node needs.
async fn chunk(transport: &Transport, start: u64, count: u64) -> Result<Vec<ScannableBlock>, InterfaceError> {
    let mut blocks: Vec<ScannableBlock> = Vec::with_capacity(count as usize);
    while (blocks.len() as u64) < count {
        let from = start + blocks.len() as u64;
        let left = count - blocks.len() as u64;
        let mut attempt = 0;
        let got = loop {
            let result = match transport.post("get_blocks.bin", request(from, left), Some(MAX_RESPONSE)).await {
                Ok(response) => parse(&response),
                Err(e) => Err(e),
            };
            match result {
                // A busy public node drops connections now and then: try again.
                Err(InterfaceError::InterfaceError(_)) if attempt < RETRIES => {
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(500 << attempt)).await;
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
    Ok(blocks)
}

/// Blocks `start..=end` in order, checked to follow on from each other.
/// `start` must be at least 1.
pub async fn blocks(transport: &Transport, start: u64, end: u64) -> Result<Vec<ScannableBlock>, InterfaceError> {
    if start == 0 {
        return Err(InterfaceError::InternalError("get_blocks.bin can't start at block 0".into()));
    }
    // Spawned all at once, but only PARALLEL run at a time: the rest wait, in
    // order, for a permit, so a slow node isn't flooded.
    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(PARALLEL));
    let mut pending = vec![];
    let mut from = start;
    while from <= end {
        let count = CHUNK.min(end - from + 1);
        let transport = transport.clone();
        let permits = permits.clone();
        pending.push(tokio::spawn(async move {
            let _permit = permits.acquire_owned().await.map_err(|e| InterfaceError::InternalError(e.to_string()))?;
            chunk(&transport, from, count).await
        }));
        from += count;
    }
    let mut res = Vec::with_capacity((end - start + 1) as usize);
    for handle in pending {
        res.extend(handle.await.map_err(|e| InterfaceError::InternalError(e.to_string()))??);
    }
    for (i, block) in res.iter().enumerate() {
        if block.block.number() as u64 != start + i as u64 {
            return Err(invalid("the node sent blocks out of order"));
        }
        if i > 0 && block.block.header.previous != res[i - 1].block.hash() {
            return Err(invalid("the node sent blocks that don't follow on"));
        }
    }
    Ok(res)
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
    fn a_response_that_isnt_epee_is_refused() {
        assert!(parse(b"hello").is_err());
    }
}
