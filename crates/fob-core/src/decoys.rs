//! Decoys for sending, without downloading the chain's output distribution
//! every time.
//!
//! Choosing a ring's decoys needs the cumulative count of RingCT outputs at
//! every block since RingCT began: 8 bytes a block, uncompressed, about 18 MB
//! on stagenet and 30 MB on mainnet. monero-oxide asks the node for all of it
//! on each payment, which took minutes from a public node. This keeps it, on
//! disk beside the wallet (it's public chain data), and fetches only the
//! blocks since, plus the last few again in case they were reorganised.
//!
//! It also asks for the distribution compressed: each block's own output
//! count as a varint, mostly one byte, instead of the running total in eight.
//! The first download is several times smaller (monero-oxide has a TODO for
//! this).

use std::{
    ops::{Bound, RangeBounds},
    path::PathBuf,
    sync::Arc,
};

use monero_wallet::{
    ed25519::Point,
    interface::{EvaluateUnlocked, InterfaceError, ProvidesBlockchainMeta, ProvidesUnvalidatedDecoys, TransactionsError},
};
use tokio::sync::Mutex;

use crate::transport::{Daemon, Transport};
use monero_daemon_rpc::HttpTransport;
use monero_epee::{Epee, HEADER, Type, VERSION};

/// Blocks fetched again at the end of the cached distribution, in case a
/// reorganisation changed them.
const REFETCH: usize = 30;

/// The cumulative RingCT output count at each block from `start`.
struct Distribution {
    start: usize,
    counts: Vec<u64>,
}

impl Distribution {
    fn end(&self) -> Option<usize> {
        (self.start + self.counts.len()).checked_sub(1)
    }

    fn read(path: &PathBuf) -> Option<Distribution> {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() < 8 || bytes.len() % 8 != 0 {
            return None;
        }
        let mut words = bytes.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c));
        let start = usize::try_from(words.next()?).ok()?;
        Some(Distribution { start, counts: words.collect() })
    }

    fn write(&self, path: &PathBuf) {
        let mut bytes = Vec::with_capacity(8 * (self.counts.len() + 1));
        bytes.extend((self.start as u64).to_le_bytes());
        for c in &self.counts {
            bytes.extend(c.to_le_bytes());
        }
        let tmp = path.with_extension("tmp");
        // A cache: if it can't be written, the next payment fetches it again.
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

/// The node, with the output distribution kept between payments.
#[derive(Clone)]
pub struct Decoys {
    daemon: Daemon,
    transport: Transport,
    cache: Arc<Mutex<Option<Distribution>>>,
    file: Arc<PathBuf>,
}

impl Decoys {
    pub fn new(daemon: Daemon, transport: Transport, file: PathBuf) -> Decoys {
        let cached = Distribution::read(&file);
        Decoys { daemon, transport, cache: Arc::new(Mutex::new(cached)), file: Arc::new(file) }
    }

    /// Brings the distribution up to block `to`, fetching only what's new.
    pub async fn update(&self, to: usize) -> Result<(), InterfaceError> {
        let mut cache = self.cache.lock().await;
        let from = match cache.as_ref().and_then(Distribution::end) {
            Some(end) if end >= to => return Ok(()),
            Some(end) => (end + 1).saturating_sub(REFETCH).max(cache.as_ref().map_or(0, |d| d.start)),
            None => 0,
        };
        let fresh = compressed_distribution(&self.transport, from, to).await?;
        // The node starts the distribution where RingCT began, if that's after `from`.
        let fresh_start = (to + 1).checked_sub(fresh.len()).ok_or_else(|| InterfaceError::InvalidInterface("too long a distribution".into()))?;
        match cache.as_mut() {
            Some(d) if fresh_start >= d.start && fresh_start <= d.start + d.counts.len() => {
                d.counts.truncate(fresh_start - d.start);
                d.counts.extend(fresh);
            }
            _ => *cache = Some(Distribution { start: fresh_start, counts: fresh }),
        }
        if let Some(d) = cache.as_ref() {
            d.write(&self.file);
        }
        Ok(())
    }
}

/// Monero's varint: seven bits a byte, low bits first.
fn varints(mut bytes: &[u8]) -> Option<Vec<u64>> {
    let mut out = Vec::with_capacity(bytes.len());
    while !bytes.is_empty() {
        let (mut value, mut shift) = (0u64, 0u32);
        loop {
            let (&byte, rest) = bytes.split_first()?;
            bytes = rest;
            value |= u64::from(byte & 0x7f).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
        out.push(value);
    }
    Some(out)
}

fn key(request: &mut Vec<u8>, name: &str, kind: u8) {
    request.push(name.len() as u8);
    request.extend(name.as_bytes());
    request.push(kind);
}

/// The cumulative RingCT output count at each block `from..=to`, fetched
/// compressed. The node starts at RingCT's first block if `from` is earlier.
async fn compressed_distribution(transport: &Transport, from: usize, to: usize) -> Result<Vec<u64>, InterfaceError> {
    let invalid = |what: &str| InterfaceError::InvalidInterface(format!("get_output_distribution.bin: {what}"));
    let mut request = Vec::with_capacity(128);
    request.extend(HEADER);
    request.push(VERSION);
    request.push(6 << 2);
    key(&mut request, "amounts", Type::Uint64 as u8 | 0x80);
    request.push(1 << 2);
    request.extend(0u64.to_le_bytes());
    key(&mut request, "from_height", Type::Uint64 as u8);
    request.extend((from as u64).to_le_bytes());
    key(&mut request, "to_height", Type::Uint64 as u8);
    // monerod reads a to_height of 0 as "to the tip".
    request.extend((to.max(1) as u64).to_le_bytes());
    key(&mut request, "cumulative", Type::Bool as u8);
    request.push(0);
    key(&mut request, "binary", Type::Bool as u8);
    request.push(1);
    key(&mut request, "compress", Type::Bool as u8);
    request.push(1);

    let response = transport.post("get_output_distribution.bin", request, Some(64 * 1024 * 1024)).await?;
    let epee_err = |e: monero_epee::EpeeError| invalid(&format!("{e:?}"));
    let mut epee = Epee::new(response.as_slice()).map_err(epee_err)?;
    let mut fields = epee.entry().map_err(epee_err)?.fields().map_err(epee_err)?;
    let (mut status, mut start, mut base, mut counts) = (false, None, None, None);
    while let Some(field) = fields.next() {
        let (name, value) = field.map_err(epee_err)?;
        match name.consume() {
            b"status" => {
                let text = value.to_str().map_err(epee_err)?.consume();
                status = text == b"OK";
                if !status {
                    return Err(InterfaceError::InterfaceError(format!("get_output_distribution.bin: {}", String::from_utf8_lossy(text))));
                }
            }
            b"distributions" => {
                let mut items = value.iterate().map_err(epee_err)?;
                if let Some(item) = items.next() {
                    let mut item = item.map_err(epee_err)?.fields().map_err(epee_err)?;
                    while let Some(field) = item.next() {
                        let (name, value) = field.map_err(epee_err)?;
                        match name.consume() {
                            b"start_height" => start = Some(value.to_u64().map_err(epee_err)?),
                            b"base" => base = Some(value.to_u64().map_err(epee_err)?),
                            b"compressed_data" => counts = Some(varints(value.to_str().map_err(epee_err)?.consume()).ok_or_else(|| invalid("bad varints"))?),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if !status {
        return Err(InterfaceError::InterfaceError("get_output_distribution.bin: the node's status wasn't OK".into()));
    }
    let (start, base, mut counts) = (start.ok_or_else(|| invalid("no start"))?, base.ok_or_else(|| invalid("no base"))?, counts.ok_or_else(|| invalid("no data"))?);
    if start < from as u64 || start > to as u64 {
        return Err(invalid("a distribution from the wrong block"));
    }
    let expected = to + 1 - start as usize;
    if to == 0 {
        counts.truncate(1);
    }
    if counts.len() != expected {
        return Err(invalid(&format!("{} blocks, expected {expected}", counts.len())));
    }
    // Per-block counts on top of `base`, made cumulative as monero-oxide wants.
    let mut total = base;
    for c in &mut counts {
        total = total.checked_add(*c).ok_or_else(|| invalid("counts overflow"))?;
        *c = total;
    }
    Ok(counts)
}

impl ProvidesBlockchainMeta for Decoys {
    fn latest_block_number(&self) -> impl Send + Future<Output = Result<usize, InterfaceError>> {
        self.daemon.latest_block_number()
    }
}

impl ProvidesUnvalidatedDecoys for Decoys {
    fn ringct_output_distribution(
        &self,
        range: impl Send + RangeBounds<usize>,
    ) -> impl Send + Future<Output = Result<Vec<u64>, InterfaceError>> {
        let from = match range.start_bound() {
            Bound::Included(f) => Some(*f),
            Bound::Excluded(f) => f.checked_add(1),
            Bound::Unbounded => Some(0),
        };
        let to = match range.end_bound() {
            Bound::Included(t) => Some(Some(*t)),
            Bound::Excluded(t) => Some(t.checked_sub(1)),
            Bound::Unbounded => None,
        };
        async move {
            let bad = || InterfaceError::InternalError("malformed range".into());
            let from = from.ok_or_else(bad)?;
            let to = match to {
                Some(t) => t.ok_or_else(bad)?,
                None => self.daemon.latest_block_number().await?,
            };
            self.update(to).await?;
            let cache = self.cache.lock().await;
            let d = cache.as_ref().ok_or_else(|| InterfaceError::InternalError("no distribution".into()))?;
            if to < d.start {
                return Ok(vec![]);
            }
            let lo = from.max(d.start) - d.start;
            let hi = to - d.start;
            Ok(d.counts.get(lo..=hi).ok_or_else(bad)?.to_vec())
        }
    }

    fn unlocked_ringct_outputs(
        &self,
        indexes: &[u64],
        evaluate_unlocked: EvaluateUnlocked,
    ) -> impl Send + Future<Output = Result<Vec<Option<[Point; 2]>>, TransactionsError>> {
        ProvidesUnvalidatedDecoys::unlocked_ringct_outputs(&self.daemon, indexes, evaluate_unlocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a public stagenet node"]
    async fn the_compressed_distribution_matches_monero_oxides_over_http() {
        let transport = Transport::new("http://stagenet.xmr-tw.org:38081", false).unwrap();
        let daemon = monero_daemon_rpc::MoneroDaemon::new(transport.clone()).await.unwrap();
        let tip = daemon.latest_block_number().await.unwrap();
        for (from, to) in [(tip - 500, tip), (1_000_000, 1_000_000), (1_200_000, 1_250_000)] {
            let ours = compressed_distribution(&transport, from, to).await.unwrap();
            let theirs = ProvidesUnvalidatedDecoys::ringct_output_distribution(&daemon, from..=to).await.unwrap();
            assert_eq!(ours, theirs, "{from}..={to}");
        }
        let started = std::time::Instant::now();
        let before = transport.requests();
        let ours = compressed_distribution(&transport, 0, tip).await.unwrap();
        let compressed = started.elapsed();
        let started = std::time::Instant::now();
        let theirs = ProvidesUnvalidatedDecoys::ringct_output_distribution(&daemon, 0..=tip).await.unwrap();
        println!("whole chain: compressed {compressed:?}, uncompressed {:?} ({} requests)", started.elapsed(), transport.requests() - before);
        assert_eq!(ours, theirs);
    }

    #[test]
    fn varints_decode_like_monero_writes_them() {
        assert_eq!(varints(&[0x01, 0x0b, 0x00]), Some(vec![1, 11, 0]));
        assert_eq!(varints(&[0xac, 0x02]), Some(vec![300]));
        assert_eq!(varints(&[0xff, 0xff, 0xff, 0xff, 0x0f]), Some(vec![u64::from(u32::MAX)]));
        assert_eq!(varints(&[0x80]), None, "cut off mid-number");
    }

    #[test]
    fn the_cache_file_round_trips() {
        let path = std::env::temp_dir().join(format!("fob-decoys-{}", std::process::id()));
        Distribution { start: 1_220_516, counts: vec![1, 5, 9, 14] }.write(&path);
        let back = Distribution::read(&path).unwrap();
        assert_eq!(back.end(), Some(1_220_519));
        assert_eq!((back.start, back.counts), (1_220_516, vec![1, 5, 9, 14]));
        std::fs::write(&path, [1, 2, 3]).unwrap();
        assert!(Distribution::read(&path).is_none(), "a torn file is ignored");
        std::fs::remove_file(path).unwrap();
    }
}
