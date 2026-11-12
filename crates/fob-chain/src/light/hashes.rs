//! Block hashes from a node, to check a light wallet server's proofs
//! ([`super::proof`]) against, asked so the node learns nothing of which
//! blocks hold the wallet's payments.
//!
//! The node is asked for hashes over a stretch of the chain, never for the
//! blocks a payment came in alone: `/get_hashes.bin` gives up to 10,000 at
//! a time from a block it knows, the way monerod's own peers catch up. Fob
//! keeps what it has checked ([`Known`]): the hash at each height a payment
//! came in, and a few near the tip where it last stopped, from which the
//! next stretch is asked. From there to the tip is one stretch while it's
//! [`SPAN_CAP`] blocks or fewer; past that (a restore, a long absence) each
//! payment's 1,000-block [`WINDOW`] is asked from its start, so the node
//! learns about when, to a day or two, not the block.
//!
//! A node whose chain no longer has the block Fob last stopped at answers
//! from the newest of the kept ones it still has: the chain was reorganised
//! there, and what Fob checked above it is forgotten and checked again.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    time::Duration,
};

use monero_daemon_rpc::HttpTransport;
use monero_epee::{Epee, HEADER, Type, VERSION};

use fob_monero::network::Network;

use crate::transport::Transport;

/// The longest stretch asked whole, from where Fob last stopped (or the
/// wallet's first block) to the tip: two or three requests, about 640 kB.
pub const SPAN_CAP: u64 = 20_000;
/// Past [`SPAN_CAP`], the stretch asked for each payment: the 1,000 blocks
/// (about a day and a half) it's in, from their start.
pub const WINDOW: u64 = 1_000;
/// The most windows asked at one poll, each a page of up to [`PAGE`]
/// hashes (about 320 kB): a restore with payments in more is checked over
/// a few polls, and a server that makes up blocks costs the node at most
/// this many pages a poll.
pub const WINDOWS: usize = 8;
/// The most hashes a node gives for one `/get_hashes.bin`
/// (`BLOCKS_IDS_SYNCHRONIZING_DEFAULT_COUNT`).
pub const PAGE: u64 = 10_000;
/// The most a node's answer may be: [`PAGE`] hashes, and room for the rest.
pub const LIMIT: usize = 1 << 20;
/// The longest a request may take, as for a light wallet server's.
pub const REQUEST: Duration = super::client::REQUEST;
/// How many of the hashes near where Fob last stopped are kept, to find
/// where a reorganised chain parted from it: that block, then 1, 2, 4, …
/// back.
const HISTORY: usize = 12;

/// The first block's hash on `network`: the last of the blocks a node is
/// told Fob knows, which every chain shares.
pub fn genesis(network: Network) -> [u8; 32] {
    let hex = match network {
        Network::Mainnet => "418015bb9ae982a1975da7d79277c2705727a56894ba0fb246adaabb1f4632e3",
        Network::Stagenet => "76ee3cc98646292206cd3e86f74d88b4dcc1d937088645e9b0cbca84b7ce74eb",
    };
    let mut out = [0; 32];
    hex::decode_to_slice(hex, &mut out).expect("a hash");
    out
}

/// Hashes a node gave: from `start` on, and the node's height.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hashes {
    pub start: u64,
    pub hashes: Vec<[u8; 32]>,
    /// Blocks on the node's chain.
    pub height: u64,
}

impl Hashes {
    /// The height of the last hash given.
    fn end(&self) -> Option<u64> {
        (!self.hashes.is_empty()).then(|| self.start + self.hashes.len() as u64 - 1)
    }

    fn at(&self, height: u64) -> Option<[u8; 32]> {
        let i = usize::try_from(height.checked_sub(self.start)?).ok()?;
        self.hashes.get(i).copied()
    }
}

/// Where block hashes come from: a node ([`NodeHashes`]), or a test's.
pub trait Source {
    /// The hash of the block at `height`: asked only for where a stretch
    /// starts (the wallet's first block, a window's start, the tip).
    fn hash_at(&self, height: u64) -> impl Future<Output = Result<[u8; 32], String>> + Send;
    /// Hashes from the newest of `known` the node has (newest first; the
    /// first block's is added), that block's included.
    fn hashes_from(&self, known: &[[u8; 32]]) -> impl Future<Output = Result<Hashes, String>> + Send;
}

/// What Fob has had a node vouch for: the hash at each height a payment
/// came in, and a few where it last stopped, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known {
    pub blocks: BTreeMap<u64, [u8; 32]>,
    pub history: Vec<(u64, [u8; 32])>,
}

/// What was asked of the node, for tests and the decision's claim that no
/// single payment's height is asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asked {
    /// Heights whose hash alone was asked: stretches' starts.
    pub starts: Vec<u64>,
    /// `/get_hashes.bin` requests.
    pub pages: usize,
}

/// How to cover the heights a node must vouch for: one stretch from
/// `span` to the tip, and windows from these starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub span: Option<u64>,
    pub windows: Vec<u64>,
}

/// The stretch for `needed` heights: from `from` (where Fob last stopped,
/// or the wallet's first block) to `tip` in one, if that's [`SPAN_CAP`]
/// blocks or fewer; else, and for any height below `from`, the
/// [`WINDOW`] each is in.
pub fn plan(needed: &BTreeSet<u64>, from: u64, tip: u64) -> Plan {
    let mut plan = Plan::default();
    if needed.is_empty() {
        return plan;
    }
    let spans = tip.saturating_sub(from) <= SPAN_CAP;
    if spans && needed.iter().any(|&h| h >= from) {
        plan.span = Some(from);
    }
    let windowed = needed.iter().filter(|&&h| h < from || !spans).map(|h| h / WINDOW * WINDOW);
    plan.windows = windowed.collect::<BTreeSet<_>>().into_iter().collect();
    plan
}

impl Known {
    /// The block a node says is at `height`, if one has.
    pub fn at(&self, height: u64) -> Option<[u8; 32]> {
        self.blocks.get(&height).copied()
    }

    /// Asks `source` for what's needed to vouch for or against each block
    /// in `wanted` (a height and the hash a server says is there), from
    /// what's kept, up to `tip`. `first` is the wallet's first block: where
    /// the first stretch starts. A height already kept with the hash
    /// wanted asks nothing; one kept with another has the node asked
    /// whether the chain was reorganised there and, if not, is the node's
    /// word against the server's, and nothing more is asked. A failure
    /// leaves what was learnt before it.
    pub async fn learn(&mut self, source: &impl Source, wanted: &BTreeMap<u64, [u8; 32]>, first: u64, tip: u64) -> Result<Asked, String> {
        let mut asked = Asked::default();
        let needed = |known: &Known| wanted.iter().filter(|(h, hash)| known.at(**h) != Some(**hash)).map(|(h, _)| *h).collect::<BTreeSet<u64>>();
        if needed(self).is_empty() {
            return Ok(asked);
        }
        // Heights the node has answered for this time: asked no more.
        let mut answered: BTreeSet<u64> = BTreeSet::new();

        // On from where it last stopped, if that's a stretch: which also
        // tells whether the chain was reorganised since, and so is asked
        // first when a server names another block where one was checked.
        let mut page = None;
        let changed = needed(self).iter().any(|h| self.blocks.contains_key(h));
        if let Some(&(top, _)) = self.history.last()
            && tip.saturating_sub(top) <= SPAN_CAP
            && (changed || plan(&needed(self), top, tip).span.is_some())
        {
            let from = self.on_from_history(source, &mut asked).await?;
            if !self.history.is_empty() {
                page = Some(from);
            }
        }
        // A height the node gave another block for, and still does (the
        // chain wasn't reorganised there, as the kept blocks near the tip
        // just told): its word against the server's, which refuses the
        // answer. Neither it nor anything else is asked until the server's
        // answer changes, so one that makes up blocks can't have every poll
        // ask the node for them again.
        let recent = self.history.last().is_some_and(|(top, _)| tip.saturating_sub(*top) <= SPAN_CAP);
        if recent && needed(self).iter().any(|h| self.blocks.contains_key(h)) {
            return Ok(asked);
        }
        let from = self.history.last().map_or(first, |(h, _)| *h);
        let plan = plan(&needed(self), from, tip);

        // One stretch to the tip, from there or from the wallet's first block.
        if plan.span.is_some() {
            let mut page = match page {
                Some(page) => page,
                None => self.start(source, from, &mut asked).await?,
            };
            loop {
                answered.extend(self.take(&page, wanted));
                let Some(end) = page.end() else { break };
                if end + 1 >= tip.min(page.height) || page.hashes.len() < 2 || asked.pages > (SPAN_CAP / PAGE) as usize + 2 {
                    self.keep_history(&page);
                    break;
                }
                let last = page.at(end).expect("the end");
                asked.pages += 1;
                let next = source.hashes_from(&[last]).await?;
                if next.start != end || next.at(end) != Some(last) {
                    return Err("the node's hashes don't follow on".into());
                }
                page = next;
            }
        }

        // Windows, for what the stretch didn't or couldn't cover.
        // At most [`WINDOWS`] of them: the rest at the next poll.
        let mut node_height = None;
        let mut windows = 0;
        for start in plan.windows {
            let left = needed(self).into_iter().filter(|h| !answered.contains(h) && h / WINDOW * WINDOW == start).count();
            if left == 0 || node_height.is_some_and(|height| start >= height) {
                continue;
            }
            if windows == WINDOWS {
                break;
            }
            windows += 1;
            let page = self.start(source, start, &mut asked).await?;
            node_height = Some(page.height);
            answered.extend(self.take(&page, wanted));
            if page.end().is_some_and(|end| end + 1 >= tip.min(page.height)) {
                self.keep_history(&page);
            }
        }
        // Somewhere near the tip to go on from next time, if the windows
        // didn't reach it: the tip itself, which says nothing of the wallet.
        let stale = self.history.last().is_none_or(|(h, _)| tip.saturating_sub(*h) > SPAN_CAP);
        if stale && tip > 0 {
            let at = tip - 1;
            asked.starts.push(at);
            let hash = source.hash_at(at).await?;
            self.history = vec![(at, hash)];
        }
        Ok(asked)
    }

    /// Hashes from `start` on, asking its hash first: where a stretch or a
    /// window starts.
    async fn start(&mut self, source: &impl Source, start: u64, asked: &mut Asked) -> Result<Hashes, String> {
        asked.starts.push(start);
        let hash = source.hash_at(start).await?;
        asked.pages += 1;
        let page = source.hashes_from(&[hash]).await?;
        if page.start != start || page.at(start) != Some(hash) {
            return Err("the node's hashes don't start where asked".into());
        }
        Ok(page)
    }

    /// Hashes on from the newest kept block near the tip that the node
    /// still has. One it no longer has was reorganised away: it and what
    /// was checked above where the chains part are forgotten.
    async fn on_from_history(&mut self, source: &impl Source, asked: &mut Asked) -> Result<Hashes, String> {
        let known: Vec<[u8; 32]> = self.history.iter().rev().map(|(_, hash)| *hash).collect();
        asked.pages += 1;
        let page = source.hashes_from(&known).await?;
        let parted = self.history.iter().find(|(h, _)| *h == page.start).is_none_or(|(_, hash)| page.at(page.start) != Some(*hash));
        if parted {
            // Not one of them: the chains part below them all.
            self.history.clear();
            self.blocks.retain(|h, _| *h < page.start);
        } else {
            self.history.retain(|(h, _)| *h <= page.start);
            self.blocks.retain(|h, hash| *h <= page.start || page.at(*h).is_none_or(|at| at == *hash));
        }
        Ok(page)
    }

    /// Keeps the node's hash at each wanted height `page` covers, and says
    /// which those were.
    fn take(&mut self, page: &Hashes, wanted: &BTreeMap<u64, [u8; 32]>) -> Vec<u64> {
        let mut took = vec![];
        for height in wanted.keys() {
            if let Some(hash) = page.at(*height) {
                self.blocks.insert(*height, hash);
                took.push(*height);
            }
        }
        took
    }

    /// Keeps the end of `page` and blocks 1, 2, 4, … before it, to go on
    /// from next time.
    fn keep_history(&mut self, page: &Hashes) {
        let Some(end) = page.end() else { return };
        let mut history: Vec<(u64, [u8; 32])> = (0..HISTORY as u32)
            .map(|i| if i == 0 { 0 } else { 1u64 << (i - 1) })
            .filter_map(|back| end.checked_sub(back))
            .filter_map(|h| page.at(h).map(|hash| (h, hash)))
            .collect();
        history.dedup_by_key(|(h, _)| *h);
        history.reverse();
        self.history = history;
    }
}

/// A node, asked over its own transport: the node path's rules (certificate,
/// pin, Basic auth, no redirects, the onion refusal, 15 s to connect and
/// 30 s between pieces), [`REQUEST`] in all and [`LIMIT`] for an answer.
#[derive(Clone)]
pub struct NodeHashes {
    pub transport: Transport,
    pub network: Network,
}

impl NodeHashes {
    async fn post(&self, route: &str, body: Vec<u8>) -> Result<Vec<u8>, String> {
        match tokio::time::timeout(REQUEST, self.transport.post(route, body, Some(LIMIT))).await {
            Err(_) => Err(format!("no answer in {} s", REQUEST.as_secs())),
            Ok(Err(e)) => Err(match e {
                monero_wallet::interface::InterfaceError::InterfaceError(text) => text,
                other => format!("{other:?}"),
            }),
            Ok(Ok(body)) => Ok(body),
        }
    }
}

impl Source for NodeHashes {
    async fn hash_at(&self, height: u64) -> Result<[u8; 32], String> {
        let request = format!(r#"{{"jsonrpc":"2.0","id":"0","method":"on_get_block_hash","params":[{height}]}}"#);
        let body = self.post("json_rpc", request.into_bytes()).await?;
        let value: serde_json::Value = serde_json::from_slice(&body).map_err(|_| "the node's answer isn't JSON".to_owned())?;
        let hex = value.get("result").and_then(|r| r.as_str()).ok_or_else(|| "the node has no block there".to_owned())?;
        let mut hash = [0; 32];
        hex::decode_to_slice(hex, &mut hash).map_err(|_| "the node's hash isn't one".to_owned())?;
        Ok(hash)
    }

    async fn hashes_from(&self, known: &[[u8; 32]]) -> Result<Hashes, String> {
        let body = self.post("get_hashes.bin", request(known, self.network)).await?;
        parse(&body)
    }
}

fn varint(out: &mut Vec<u8>, n: usize) {
    // Epee's: the low two bits say how many bytes follow.
    match n {
        0..=0x3f => out.push((n as u8) << 2),
        0x40..=0x3fff => out.extend(((n as u16) << 2 | 1).to_le_bytes()),
        0x4000..=0x3fff_ffff => out.extend(((n as u32) << 2 | 2).to_le_bytes()),
        _ => out.extend(((n as u64) << 2 | 3).to_le_bytes()),
    }
}

fn key(out: &mut Vec<u8>, name: &str, kind: Type) {
    out.push(name.len() as u8);
    out.extend(name.as_bytes());
    out.push(kind as u8);
}

/// `/get_hashes.bin` from the newest of `known` the node has: `block_ids`
/// (newest first, the first block's last, as one blob) and `start_height`,
/// which monerod reads but goes by the blocks instead.
pub fn request(known: &[[u8; 32]], network: Network) -> Vec<u8> {
    let mut ids: Vec<u8> = known.iter().flatten().copied().collect();
    ids.extend(genesis(network));
    let mut out = Vec::with_capacity(64 + ids.len());
    out.extend(HEADER);
    out.push(VERSION);
    varint(&mut out, 2);
    key(&mut out, "block_ids", Type::String);
    varint(&mut out, ids.len());
    out.extend(ids);
    key(&mut out, "start_height", Type::Uint64);
    out.extend(0u64.to_le_bytes());
    out
}

fn epee_err(e: monero_epee::EpeeError) -> String {
    format!("get_hashes.bin: {e:?}")
}

/// A node's `/get_hashes.bin` answer.
pub fn parse(response: &[u8]) -> Result<Hashes, String> {
    let mut epee = Epee::new(response).map_err(epee_err)?;
    let mut fields = epee.entry().map_err(epee_err)?.fields().map_err(epee_err)?;
    let (mut ids, mut start, mut height, mut ok) = (None, None, None, false);
    while let Some(field) = fields.next() {
        let (name, value) = field.map_err(epee_err)?;
        match name.consume() {
            b"status" => ok = value.to_str().map_err(epee_err)?.consume() == b"OK",
            b"m_block_ids" => ids = Some(value.to_str().map_err(epee_err)?.consume().to_vec()),
            b"start_height" => start = Some(value.to_u64().map_err(epee_err)?),
            b"current_height" => height = Some(value.to_u64().map_err(epee_err)?),
            _ => {}
        }
    }
    if !ok {
        return Err("get_hashes.bin: the node's status wasn't OK".into());
    }
    let ids = ids.unwrap_or_default();
    let (Some(start), Some(height)) = (start, height) else { return Err("get_hashes.bin: heights missing".into()) };
    if ids.len() % 32 != 0 || ids.len() as u64 > PAGE * 32 * 3 {
        return Err("get_hashes.bin: hashes aren't whole".into());
    }
    let hashes = ids.as_chunks::<32>().0.to_vec();
    Ok(Hashes { start, hashes, height })
}

/// A `/get_hashes.bin` answer as monerod writes one: for fake nodes in tests.
#[cfg(any(test, feature = "test-support"))]
pub fn response(page: &Hashes) -> Vec<u8> {
    let ids: Vec<u8> = page.hashes.iter().flatten().copied().collect();
    let mut out = Vec::new();
    out.extend(HEADER);
    out.push(VERSION);
    varint(&mut out, 4);
    key(&mut out, "current_height", Type::Uint64);
    out.extend(page.height.to_le_bytes());
    key(&mut out, "m_block_ids", Type::String);
    varint(&mut out, ids.len());
    out.extend(ids);
    key(&mut out, "start_height", Type::Uint64);
    out.extend(page.start.to_le_bytes());
    key(&mut out, "status", Type::String);
    varint(&mut out, 2);
    out.extend(b"OK");
    out
}

/// The blocks a `/get_hashes.bin` request says Fob knows, newest first: for
/// fake nodes in tests.
#[cfg(any(test, feature = "test-support"))]
pub fn known_in(request: &[u8]) -> Vec<[u8; 32]> {
    let Ok(mut epee) = Epee::new(request) else { return vec![] };
    let Ok(entry) = epee.entry() else { return vec![] };
    let Ok(mut fields) = entry.fields() else { return vec![] };
    while let Some(Ok((name, value))) = fields.next() {
        if name.consume() == b"block_ids" {
            let Ok(ids) = value.to_str() else { return vec![] };
            return ids.consume().as_chunks::<32>().0.to_vec();
        }
    }
    vec![]
}

/// What a chain of `hashes` (from block 0) answers `/get_hashes.bin` with,
/// as monerod does: from the newest of `known` it has, [`PAGE`] at most.
#[cfg(any(test, feature = "test-support"))]
pub fn answer(hashes: &[[u8; 32]], known: &[[u8; 32]]) -> Hashes {
    let start = known.iter().find_map(|k| hashes.iter().position(|h| h == k)).unwrap_or(0);
    let end = hashes.len().min(start + PAGE as usize);
    Hashes { start: start as u64, hashes: hashes[start..end].to_vec(), height: hashes.len() as u64 }
}

#[cfg(test)]
mod tests;
