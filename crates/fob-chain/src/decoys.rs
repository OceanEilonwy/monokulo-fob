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
    time::{Duration, Instant},
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

/// How long a tip taken by [`Decoys::note_tip`] stands in for asking the node.
const TIP_FOR: Duration = Duration::from_secs(10);

/// The cumulative RingCT output count at each block from `start`.
struct Distribution {
    start: usize,
    counts: Vec<u64>,
}

impl Distribution {
    fn end(&self) -> Option<usize> {
        (self.start + self.counts.len()).checked_sub(1)
    }

    /// Whether the counts never go down, as a running total can't, and as
    /// monero-interface refuses to choose decoys from otherwise.
    fn increasing(&self) -> bool {
        self.counts.windows(2).all(|w| w[0] <= w[1])
    }

    /// Whether counts from block `start`, built on `base` outputs before it,
    /// follow on from these: they start within them, and the count kept for
    /// the block before is `base`. A node that counts otherwise (another
    /// node's lie, kept or new, or a reorganisation deeper than the blocks
    /// fetched again) fits nothing kept, which is then fetched whole.
    fn followed_by(&self, start: usize, base: u64) -> bool {
        match start.checked_sub(self.start) {
            Some(0) => true,
            Some(at) => self.counts.get(at - 1) == Some(&base),
            None => false,
        }
    }

    fn read(path: &PathBuf) -> Option<Distribution> {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() < 8 || bytes.len() % 8 != 0 {
            return None;
        }
        let mut words = bytes.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c));
        let start = usize::try_from(words.next()?).ok()?;
        Some(Distribution { start, counts: words.collect() }).filter(Distribution::increasing)
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
    /// The chain tip a payment has just asked the node for. Choosing each
    /// input's decoys asks again, a round trip apiece, for the same answer.
    tip: Arc<std::sync::Mutex<Option<(Instant, usize)>>>,
}

impl Decoys {
    pub fn new(daemon: Daemon, transport: Transport, file: PathBuf) -> Decoys {
        let cached = Distribution::read(&file);
        Decoys { daemon, transport, cache: Arc::new(Mutex::new(cached)), file: Arc::new(file), tip: Arc::default() }
    }

    /// Takes `tip`, just asked of the node, as the tip for the next few seconds.
    pub fn note_tip(&self, tip: usize) {
        *self.tip.lock().expect("tip lock") = Some((Instant::now(), tip));
    }

    /// Brings the distribution up to block `to`, fetching only what's new.
    /// What's new has to follow on from what's kept; if it doesn't, what's
    /// kept is forgotten, file and all, and the whole of it fetched again,
    /// rather than every payment failing on a distribution that goes down.
    pub async fn update(&self, to: usize) -> Result<(), InterfaceError> {
        let mut cache = self.cache.lock().await;
        let mut from = match cache.as_ref().and_then(Distribution::end) {
            Some(end) if end >= to => return Ok(()),
            Some(end) => (end + 1).saturating_sub(REFETCH).max(cache.as_ref().map_or(0, |d| d.start)),
            None => 0,
        };
        loop {
            let (base, fresh) = compressed_distribution(&self.transport, from, to).await?;
            // The node starts the distribution where RingCT began, if that's after `from`.
            let fresh_start = (to + 1).checked_sub(fresh.len()).ok_or_else(|| InterfaceError::InvalidInterface("too long a distribution".into()))?;
            let next = match cache.take() {
                Some(mut kept) if from > 0 && kept.followed_by(fresh_start, base) => {
                    kept.counts.truncate(fresh_start - kept.start);
                    kept.counts.extend(fresh);
                    Some(kept)
                }
                // Only the newest blocks, and they don't fit: nothing to keep.
                _ if from > 0 => None,
                _ => Some(Distribution { start: fresh_start, counts: fresh }),
            };
            match next {
                Some(next) if next.increasing() => {
                    next.write(&self.file);
                    *cache = Some(next);
                    return Ok(());
                }
                // Fetched whole and still wrong: the node's own doing.
                _ if from == 0 => return Err(InterfaceError::InvalidInterface("get_output_distribution.bin: a distribution that goes down".into())),
                _ => {
                    let _ = std::fs::remove_file(self.file.as_ref());
                    from = 0;
                }
            }
        }
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
/// compressed, with the count before them it builds on. The node starts at
/// RingCT's first block if `from` is earlier.
async fn compressed_distribution(transport: &Transport, from: usize, to: usize) -> Result<(u64, Vec<u64>), InterfaceError> {
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
    Ok((base, counts))
}

impl ProvidesBlockchainMeta for Decoys {
    fn latest_block_number(&self) -> impl Send + Future<Output = Result<usize, InterfaceError>> {
        let noted = self.tip.lock().expect("tip lock").filter(|(at, _)| at.elapsed() < TIP_FOR).map(|(_, tip)| tip);
        let daemon = self.daemon.clone();
        async move {
            match noted {
                Some(tip) => Ok(tip),
                None => daemon.latest_block_number().await,
            }
        }
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
            let (_, ours) = compressed_distribution(&transport, from, to).await.unwrap();
            let theirs = ProvidesUnvalidatedDecoys::ringct_output_distribution(&daemon, from..=to).await.unwrap();
            assert_eq!(ours, theirs, "{from}..={to}");
        }
        let started = std::time::Instant::now();
        let before = transport.requests();
        let (_, ours) = compressed_distribution(&transport, 0, tip).await.unwrap();
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

    /// epee's varint, for lengths and counts: the size in its low two bits.
    fn epee_varint(out: &mut Vec<u8>, n: usize) {
        match n {
            0..64 => out.push((n << 2) as u8),
            64..16384 => out.extend(((n << 2) as u16 | 1).to_le_bytes()),
            _ => out.extend(((n << 2) as u32 | 2).to_le_bytes()),
        }
    }

    /// A `get_output_distribution.bin` answer: from `start`, on `base`, each
    /// block's own count, compressed as monerod sends it.
    fn distribution_answer(start: u64, base: u64, per_block: &[u64]) -> Vec<u8> {
        let mut data = vec![];
        for &n in per_block {
            let mut n = n;
            while n >= 0x80 {
                data.push(n as u8 | 0x80);
                n >>= 7;
            }
            data.push(n as u8);
        }
        let mut out = HEADER.to_vec();
        out.push(VERSION);
        epee_varint(&mut out, 2);
        key(&mut out, "status", Type::String as u8);
        epee_varint(&mut out, 2);
        out.extend(b"OK");
        key(&mut out, "distributions", Type::Object as u8 | 0x80);
        epee_varint(&mut out, 1);
        epee_varint(&mut out, 3);
        key(&mut out, "start_height", Type::Uint64 as u8);
        out.extend(start.to_le_bytes());
        key(&mut out, "base", Type::Uint64 as u8);
        out.extend(base.to_le_bytes());
        key(&mut out, "compressed_data", Type::String as u8);
        epee_varint(&mut out, data.len());
        out.extend(data);
        out
    }

    /// A node whose chain has RingCT from block 100, each block with `per`
    /// outputs on top of `base`'s at the block before 100 (honestly 0). Any
    /// json_rpc call gets an empty answer.
    async fn distribution_node(base: u64, per: u64) -> Transport {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else { return };
                tokio::spawn(async move {
                    let mut request = vec![];
                    let mut buf = [0; 4096];
                    loop {
                        let Ok(n) = stream.read(&mut buf).await else { return };
                        if n == 0 {
                            return;
                        }
                        request.extend_from_slice(&buf[..n]);
                        let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                        let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                        let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                        if request.len() >= end + 4 + len {
                            break;
                        }
                    }
                    let number = |name: &[u8]| {
                        let at = request.windows(name.len()).position(|w| w == name).unwrap() + name.len() + 1;
                        u64::from_le_bytes(request[at..at + 8].try_into().unwrap())
                    };
                    let body = if request.windows(9).any(|w| w == b"get_outpu") {
                        let (from, to) = (number(b"from_height").max(100), number(b"to_height"));
                        let per_block = vec![per; (to + 1 - from) as usize];
                        distribution_answer(from, base + (from - 100) * per, &per_block)
                    } else {
                        br#"{"jsonrpc":"2.0","id":0,"result":{}}"#.to_vec()
                    };
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Transport::new(&format!("http://127.0.0.1:{port}"), false).unwrap()
    }

    async fn decoys(transport: Transport, file: &std::path::Path) -> Decoys {
        let daemon = monero_daemon_rpc::MoneroDaemon::new(transport.clone()).await.unwrap();
        Decoys::new(daemon, transport, file.to_owned())
    }

    /// What's kept is fetched only past its end, the last blocks again; a
    /// node whose counts don't follow on from it has it fetched whole, not
    /// spliced into a distribution that goes down and fails every payment
    /// until the file is deleted by hand (audit #64).
    #[tokio::test]
    async fn a_distribution_that_doesnt_follow_on_is_fetched_whole() {
        let file = std::env::temp_dir().join(format!("fob-decoys-splice-{}", std::process::id()));
        // Kept from an honest node: ten outputs a block from block 100 to 199.
        Distribution { start: 100, counts: (1..=100).map(|i| i * 10).collect() }.write(&file);
        let honest = distribution_node(0, 10).await;
        let kept = decoys(honest.clone(), &file).await;
        kept.update(209).await.unwrap();
        assert_eq!(honest.requests(), 2, "connecting, then only the newest blocks");
        let d = Distribution::read(&file).unwrap();
        assert_eq!((d.start, d.counts.len(), d.counts.last()), (100, 110, Some(&1100)));

        // A node counting otherwise, as after one that lied: what's new doesn't
        // follow on, so all of it comes from this node.
        Distribution { start: 100, counts: (1..=100).map(|i| i * 1_000).collect() }.write(&file);
        let other = distribution_node(0, 10).await;
        let lied_to = decoys(other.clone(), &file).await;
        lied_to.update(209).await.unwrap();
        assert_eq!(other.requests(), 3, "connecting, the newest, then the whole");
        let d = Distribution::read(&file).unwrap();
        assert_eq!((d.start, d.counts.len(), d.counts.last()), (100, 110, Some(&1100)));
        assert!(d.increasing());
        let picked = ProvidesUnvalidatedDecoys::ringct_output_distribution(&lied_to, 0..=209).await.unwrap();
        assert_eq!(picked.len(), 110);

        // A file that goes down is never read.
        Distribution { start: 100, counts: vec![5, 4] }.write(&file);
        assert!(Distribution::read(&file).is_none());
        std::fs::remove_file(file).unwrap();
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
