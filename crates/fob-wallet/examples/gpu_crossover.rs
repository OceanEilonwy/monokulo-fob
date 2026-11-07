//! Where the GPU starts to pay, in wall time: scans batches of mainnet blocks with
//! the `Scanner` alone and with the GPU prefilter first, through `scan::scan_from`:
//! `cargo run --release -p fob-wallet --example gpu_crossover [node] [first block] [repetitions]`.
//!
//! The blocks are fetched once (by default from 3,797,000, monokulo-8ar's fixture's)
//! and scanned with a throwaway wallet that nothing pays. Each size is run warm, the
//! GPU's buffers kept from the last batch as while catching up, and rested, with
//! `rest()` before each batch as after a wallet catches up at the tip. Medians.

use std::time::Instant;

use fob_wallet::{fetch, scanning::{Prefilter, engine, gpu_ready, rest, scan::scan_from, set_cache_dir, warm_up}, transport::Transport};
use monero_wallet::{Scanner, ViewPair, address::{MoneroAddress, Network}, ed25519::Scalar, extra::Extra, interface::ScannableBlock, transaction::TransactionPrefix};
use zeroize::Zeroizing;

const SIZES: [usize; 9] = [1, 2, 3, 4, 6, 9, 20, 50, 100];

/// The transaction keys in `block`, as the prefilter counts them against its
/// `min_keys`: each transaction's keys, and its additional keys up to one an output.
fn keys_in(block: &ScannableBlock) -> usize {
    let of = |prefix: &TransactionPrefix| match Extra::read(&mut prefix.extra.as_slice()).ok().and_then(|e| e.keys()) {
        Some((keys, additional)) => keys.len() + additional.map_or(0, |a| a.len().min(prefix.outputs.len())),
        None => 0,
    };
    of(block.block.miner_transaction().prefix()) + block.transactions.iter().map(|tx| of(tx.prefix())).sum::<usize>()
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let node = args.get(1).cloned().unwrap_or_else(|| "http://node.sethforprivacy.com:18089".into());
    let first: u64 = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(3_797_000);
    let reps: usize = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(15);
    if let Ok(dir) = std::env::var("FOB_SCAN_CACHE") {
        set_cache_dir(dir.into());
    }
    warm_up();
    for _ in 0..600 {
        if gpu_ready() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    println!("scanning: {:?}", engine());
    assert!(gpu_ready(), "no GPU to compare with");

    // Enough for every repetition of the largest batch to have its own blocks.
    let span = (SIZES[SIZES.len() - 1] * reps.min(5)) as u64;
    let transport = Transport::new(&node, false).unwrap();
    let blocks = fetch::blocks(&transport, first, first + span - 1).await.unwrap();
    let hashes: Vec<[u8; 32]> = blocks.iter().map(|b| b.block.hash()).collect();
    let keys = |part: &[ScannableBlock]| part.iter().map(keys_in).sum::<usize>();
    println!("{} blocks from {first}, {:.0} transaction keys a block", blocks.len(), keys(&blocks) as f64 / blocks.len() as f64);

    let wallet = fob_wallet::keys::Keys::generate(fob_wallet::Network::Mainnet).keys;
    let address = MoneroAddress::from_str(Network::Mainnet, &wallet.primary_address()).unwrap();
    let view = Scalar::hash(b"gpu crossover view key");
    let mut view_bytes = Zeroizing::new([0u8; 32]);
    view.write(&mut &mut view_bytes[..]).unwrap();
    let pair = ViewPair::new(address.spend(), Zeroizing::new(view)).unwrap();
    let scanner = Scanner::new(pair.clone());
    let prefilter = Prefilter::new(&pair, &view_bytes);

    let run = |at: usize, size: usize, min_keys: usize, rested: bool| {
        let part = blocks[at..at + size].to_vec();
        if rested {
            rest();
        }
        let t = Instant::now();
        let (results, _) = scan_from(&scanner, &prefilter, part, &hashes[at..at + size], min_keys);
        let seconds = t.elapsed().as_secs_f64();
        assert!(results.iter().all(Result::is_ok));
        seconds
    };
    // A throwaway batch, so the GPU's buffers and the threads are warm.
    run(0, 100, 0, false);

    println!("blocks keys | oxide ms | GPU warm ms | GPU rested ms   (medians of {reps})");
    for size in SIZES {
        let (mut oxide, mut warm, mut rested) = (vec![], vec![], vec![]);
        for rep in 0..reps {
            let at = (rep * size) % (blocks.len() - size + 1);
            // Alternated, so neither always goes first.
            if rep % 2 == 0 {
                oxide.push(run(at, size, usize::MAX, false));
                warm.push(run(at, size, 0, false));
            } else {
                warm.push(run(at, size, 0, false));
                oxide.push(run(at, size, usize::MAX, false));
            }
            rested.push(run(at, size, 0, true));
            // Leaves the GPU warm for the next warm batch.
            run(at, size, 0, false);
        }
        let mean_keys = (0..reps).map(|rep| { let at = (rep * size) % (blocks.len() - size + 1); keys(&blocks[at..at + size]) }).sum::<usize>() / reps;
        println!("{size} {mean_keys} | {:.2} | {:.2} | {:.2}", median(oxide) * 1e3, median(warm) * 1e3, median(rested) * 1e3);
    }
}
