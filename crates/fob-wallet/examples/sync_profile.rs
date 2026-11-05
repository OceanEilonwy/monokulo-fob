//! Where a catch-up's time goes, with a throwaway view-only wallet:
//! `cargo run --release -p fob-wallet --example sync_profile [node] [blocks]`.
//! Prints the whole catch-up through `Wallet::sync_step`, then the parts:
//! fetching alone, and scanning the same blocks on 1 and on every core.

use std::time::Instant;

use fob_wallet::{Network, Wallet, fetch, transport::Transport};
use monero_daemon_rpc::MoneroDaemon;
use monero_wallet::{Scanner, ViewPair, address::{MoneroAddress, SubaddressIndex}, ed25519::Scalar, interface::prelude::*};
use zeroize::Zeroizing;

#[tokio::main]
async fn main() {
    let node = std::env::args().nth(1).unwrap_or_else(|| "http://node.sethforprivacy.com:18089".into());
    let count: u64 = std::env::args().nth(2).and_then(|n| n.parse().ok()).unwrap_or(720);
    // FOB_SCAN=gpu, cpu or auto (the default); FOB_SCAN_CACHE keeps the compiled shader.
    use fob_wallet::scanning::{Mode, gpu_ready, set_cache_dir, set_mode};
    if let Ok(dir) = std::env::var("FOB_SCAN_CACHE") {
        set_cache_dir(dir.into());
    }
    let mode = match std::env::var("FOB_SCAN").as_deref() {
        Ok("gpu") => Mode::Gpu,
        Ok("cpu") => Mode::Cpu,
        _ => Mode::Auto,
    };
    set_mode(mode);
    if mode != Mode::Cpu {
        for _ in 0..600 {
            if gpu_ready() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    println!("scanning: {mode:?}, GPU ready: {}", gpu_ready());
    let transport = Transport::new(&node, false).unwrap();
    let rpc = MoneroDaemon::new(transport.clone()).await.unwrap();
    let tip = rpc.latest_block_number().await.unwrap() as u64;
    let start = tip - count;

    let dir = std::env::temp_dir().join(format!("fob-sync-profile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (_, words) = Wallet::create(&dir.join("new.fob"), [1; 32], Network::Mainnet).unwrap().seed_words().unwrap();
    let wallet = Wallet::restore_words(&dir.join("w.fob"), [1; 32], Network::Mainnet, &words, Some(start)).unwrap();
    let address = wallet.primary_address();
    wallet.set_node(&node, false).await.unwrap();
    let t = Instant::now();
    let mut steps = 0;
    while wallet.sync_step().await.unwrap() {
        steps += 1;
    }
    let whole = t.elapsed().as_secs_f64();
    println!("sync_step: {count} blocks in {whole:.1} s ({:.0} blocks/s, {steps} steps), GPU still ready: {}", count as f64 / whole, gpu_ready());

    let t = Instant::now();
    let blocks = fetch::blocks(&transport, start, tip - 1).await.unwrap();
    let fetched = t.elapsed().as_secs_f64();
    let txs: usize = blocks.iter().map(|b| b.transactions.len()).sum();
    let outs: usize = blocks.iter().flat_map(|b| &b.transactions).map(|t| t.prefix().outputs.len()).sum();
    println!("fetch alone: {fetched:.1} s ({:.0} blocks/s), {txs} txs, {outs} outputs", count as f64 / fetched);

    let address = MoneroAddress::from_str(monero_wallet::address::Network::Mainnet, &address).unwrap();
    let pair = ViewPair::new(address.spend(), Zeroizing::new(Scalar::hash(b"view"))).unwrap();
    let t = Instant::now();
    let mut scanner = Scanner::new(pair);
    for i in 1..=200 {
        scanner.register_subaddress(SubaddressIndex::new(0, i).unwrap());
    }
    println!("scanner with 200 subaddresses: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);

    let t = Instant::now();
    for block in blocks.clone() {
        scanner.scan(block).unwrap();
    }
    println!("scan on 1 thread: {:.2} s", t.elapsed().as_secs_f64());

    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let t = Instant::now();
    let size = blocks.len().div_ceil(threads);
    let mut parts = vec![];
    let mut blocks = blocks.into_iter().peekable();
    while blocks.peek().is_some() {
        parts.push(blocks.by_ref().take(size).collect::<Vec<_>>());
    }
    std::thread::scope(|s| {
        for part in parts {
            let mut scanner = scanner.clone();
            s.spawn(move || {
                for block in part {
                    scanner.scan(block).unwrap();
                }
            });
        }
    });
    println!("scan on {threads} threads: {:.2} s", t.elapsed().as_secs_f64());
    let _ = std::fs::remove_dir_all(dir);
}
