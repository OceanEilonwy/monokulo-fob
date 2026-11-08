use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use super::*;

/// A made-up chain: block `h`'s hash, on fork `fork` (0 the first).
fn hash(h: u64, fork: u8) -> [u8; 32] {
    let mut out = [fork; 32];
    out[..8].copy_from_slice(&h.to_le_bytes());
    out
}

fn chain(height: u64) -> Vec<[u8; 32]> {
    (0..height).map(|h| hash(h, 0)).collect()
}

/// A node with `chain`, asked as monerod is, keeping what it was asked.
#[derive(Clone, Default)]
struct Fake {
    chain: Arc<Mutex<Vec<[u8; 32]>>>,
    down: Arc<AtomicBool>,
    asked: Arc<Mutex<Asked>>,
}

impl Fake {
    fn new(chain: Vec<[u8; 32]>) -> Fake {
        Fake { chain: Arc::new(Mutex::new(chain)), ..Fake::default() }
    }
}

impl Source for Fake {
    async fn hash_at(&self, height: u64) -> Result<[u8; 32], String> {
        if self.down.load(Ordering::Relaxed) {
            return Err("connection refused".into());
        }
        self.asked.lock().unwrap().starts.push(height);
        self.chain.lock().unwrap().get(height as usize).copied().ok_or_else(|| "no block there".into())
    }

    async fn hashes_from(&self, known: &[[u8; 32]]) -> Result<Hashes, String> {
        if self.down.load(Ordering::Relaxed) {
            return Err("connection refused".into());
        }
        self.asked.lock().unwrap().pages += 1;
        let chain = self.chain.lock().unwrap();
        // The wire's way, both ends: what monerod reads, what it writes.
        let request = request(known, Network::Stagenet);
        let mut ids = known_in(&request);
        assert_eq!(ids.pop(), Some(genesis(Network::Stagenet)), "the first block's hash goes last");
        let mut chain_with_genesis = chain.clone();
        chain_with_genesis[0] = genesis(Network::Stagenet);
        ids.push(chain_with_genesis[0]);
        parse(&response(&answer(&chain_with_genesis, &ids))).map(|mut h| {
            if h.start == 0 {
                h.hashes[0] = chain[0];
            }
            h
        })
    }
}

fn wanted(heights: &[u64]) -> BTreeMap<u64, [u8; 32]> {
    heights.iter().map(|&h| (h, hash(h, 0))).collect()
}

#[test]
fn a_stretch_below_the_cap_is_one_and_past_it_windows() {
    let needed: BTreeSet<u64> = [2_244_510, 2_245_514, 2_245_517].into();
    // From the wallet's first block to the tip, under 20,000: one stretch.
    assert_eq!(plan(&needed, 2_244_400, 2_246_000), Plan { span: Some(2_244_400), windows: vec![] });
    // A restore from far back: each payment's 1,000 blocks, from their start.
    assert_eq!(plan(&needed, 2_000_000, 2_246_000), Plan { span: None, windows: vec![2_244_000, 2_245_000] });
    // Exactly the cap still spans.
    assert_eq!(plan(&needed, 2_226_000, 2_246_000).span, Some(2_226_000));
    // Below where it stopped: windowed, the rest from there on.
    let needed: BTreeSet<u64> = [1_000_500, 2_245_514].into();
    assert_eq!(plan(&needed, 2_245_000, 2_246_000), Plan { span: Some(2_245_000), windows: vec![1_000_000] });
    assert_eq!(plan(&BTreeSet::new(), 0, 10), Plan::default());
}

#[test]
fn the_wire_reads_back() {
    let page = Hashes { start: 5, hashes: vec![hash(5, 0), hash(6, 0)], height: 9 };
    assert_eq!(parse(&response(&page)).unwrap(), page);
    assert_eq!(known_in(&request(&[hash(7, 0)], Network::Mainnet)), vec![hash(7, 0), genesis(Network::Mainnet)]);
    assert!(parse(b"not epee").is_err());
}

/// A contiguous stretch from the first block, pages of 10,000, kept so the
/// next poll asks nothing for what's checked, and goes on from the tip.
#[tokio::test]
async fn a_stretch_is_asked_once_and_kept() {
    let node = Fake::new(chain(25_000));
    let mut known = Known::default();
    let asked = known.learn(&node, &wanted(&[7_000, 21_000]), 6_000, 25_000).await.unwrap();
    assert_eq!(asked.starts, vec![6_000], "only where the stretch starts, never a payment's block");
    assert_eq!(asked.pages, 2, "6,000 to 16,000, then on to the tip");
    assert_eq!((known.at(7_000), known.at(21_000)), (Some(hash(7_000, 0)), Some(hash(21_000, 0))));
    assert_eq!(known.history.last(), Some(&(24_999, hash(24_999, 0))));
    assert!(known.history.iter().any(|(h, _)| *h == 24_998) && known.history.iter().any(|(h, _)| *h == 24_999 - 1024));
    // Again: nothing asked.
    let again = known.learn(&node, &wanted(&[7_000, 21_000]), 6_000, 25_000).await.unwrap();
    assert_eq!(again, Asked::default());
    // A new payment past where it stopped: on from there, no start asked.
    node.chain.lock().unwrap().extend((25_000..25_100).map(|h| hash(h, 0)));
    let more = known.learn(&node, &wanted(&[7_000, 21_000, 25_050]), 6_000, 25_100).await.unwrap();
    assert_eq!((more.starts.len(), more.pages), (0, 1));
    assert_eq!(known.at(25_050), Some(hash(25_050, 0)));
}

/// Past 20,000 blocks, each payment's window from its start, and the tip to
/// go on from; a second payment in the first's answer asks nothing more.
#[tokio::test]
async fn far_back_is_asked_by_windows() {
    let node = Fake::new(chain(60_000));
    let mut known = Known::default();
    let asked = known.learn(&node, &wanted(&[10_500, 12_100, 40_999]), 100, 60_000).await.unwrap();
    assert_eq!(asked.starts, vec![10_000, 40_000, 59_999], "windows' starts and the tip, never a payment's block");
    assert_eq!([10_500, 12_100, 40_999].map(|h| known.at(h)), [10_500, 12_100, 40_999].map(|h| Some(hash(h, 0))));
    assert_eq!(known.history, vec![(59_999, hash(59_999, 0))]);
}

/// The block Fob last stopped at reorganised away: what was checked above
/// where the chains part is forgotten and asked again; below stays.
#[tokio::test]
async fn a_reorganisation_is_found_and_checked_again() {
    let node = Fake::new(chain(1_000));
    let mut known = Known::default();
    known.learn(&node, &wanted(&[500, 995]), 0, 1_000).await.unwrap();
    assert_eq!(known.at(995), Some(hash(995, 0)));
    // Blocks from 990 on replaced, and the chain grown.
    {
        let mut chain = node.chain.lock().unwrap();
        chain.truncate(990);
        chain.extend((990..1_010).map(|h| hash(h, 1)));
    }
    // The server now lists the payment in the new block 995, and a new one.
    let mut now = wanted(&[500, 1_005]);
    now.insert(995, hash(995, 1));
    now.insert(1_005, hash(1_005, 1));
    let asked = known.learn(&node, &now, 0, 1_010).await.unwrap();
    assert!(asked.starts.is_empty(), "on from the kept blocks near the tip");
    assert_eq!(known.at(995), Some(hash(995, 1)), "checked again, on the new chain");
    assert_eq!(known.at(1_005), Some(hash(1_005, 1)));
    assert_eq!(known.at(500), Some(hash(500, 0)), "below the fork, kept");
    assert_eq!(known.history.last(), Some(&(1_009, hash(1_009, 1))));
    // A server still naming the old block at 995 has the node's word against it.
    let mut old = now.clone();
    old.insert(995, hash(995, 0));
    known.learn(&node, &old, 0, 1_010).await.unwrap();
    assert_ne!(known.at(995), Some(hash(995, 0)));
}

#[tokio::test]
async fn a_node_that_doesn_t_answer_is_an_error_and_keeps_what_was_known() {
    let node = Fake::new(chain(100));
    let mut known = Known::default();
    known.learn(&node, &wanted(&[50]), 0, 100).await.unwrap();
    node.down.store(true, Ordering::Relaxed);
    let before = known.clone();
    assert!(known.learn(&node, &wanted(&[50, 60]), 0, 100).await.is_err());
    assert_eq!(known.at(50), before.at(50));
    // What's kept asks nothing, node or not.
    assert!(known.learn(&node, &wanted(&[50]), 0, 100).await.is_ok());
}

/// A real stagenet node (`LIVE_NODE`, or Fob's default) answers
/// `/get_hashes.bin` as this reads it: block 2,244,000's hash, known from
/// the node's own JSON, comes out of a stretch from 2,240,000.
#[tokio::test]
#[ignore = "needs a stagenet node and the network"]
async fn live_hashes_from_a_stagenet_node() {
    let url = std::env::var("LIVE_NODE").unwrap_or_else(|_| Network::Stagenet.default_nodes()[0].to_owned());
    let node = NodeHashes { transport: Transport::new(&url, false).unwrap(), network: Network::Stagenet };
    let mut expected = [0; 32];
    hex::decode_to_slice("2b5d19e3fd8b91f53d78fbf000240aaf3f1623faaaf87d8e5e60d2db6424c289", &mut expected).unwrap();
    assert_eq!(node.hash_at(2_244_000).await.unwrap(), expected);
    let tip = node.hashes_from(&[expected]).await.unwrap().height;
    let mut known = Known::default();
    let asked = known.learn(&node, &[(2_244_000, expected)].into(), 2_240_000, tip).await.unwrap();
    println!("asked {asked:?}, tip {tip}, history {:?}", known.history.last().map(|(h, _)| h));
    assert_eq!(known.at(2_244_000), Some(expected));
    assert_eq!(known.history.last().map(|(h, _)| *h), Some(tip - 1));
}
