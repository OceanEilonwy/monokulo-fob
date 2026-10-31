//! How fast Fob catches up from a node:
//! `cargo run --release -p fob-core --example fetch_speed [node] [blocks]`.

use std::time::Instant;

use fob_core::{fetch, transport::Transport};
use monero_daemon_rpc::MoneroDaemon;
use monero_wallet::{Scanner, ViewPair, ed25519::Point, interface::prelude::*};
use zeroize::Zeroizing;

#[tokio::main]
async fn main() {
    let node = std::env::args().nth(1).unwrap_or_else(|| "http://node.monerodevs.org:38089".into());
    let count: u64 = std::env::args().nth(2).and_then(|n| n.parse().ok()).unwrap_or(2000);
    let transport = Transport::new(&node).unwrap();
    let rpc = MoneroDaemon::new(transport.clone()).await.unwrap();
    let tip = rpc.latest_block_number().await.unwrap() as u64;
    let start = tip - count;

    let before = transport.requests();
    let t = Instant::now();
    let blocks = fetch::blocks(&transport, start, tip - 1).await.unwrap();
    let fetched = t.elapsed();
    let txs: usize = blocks.iter().map(|b| b.transactions.len()).sum();

    // Scan them with a random wallet, to see what the maths costs.
    let keys = fob_core::keys::Keys::generate(fob_core::Network::Stagenet).keys;
    let address = monero_wallet::address::MoneroAddress::from_str(
        monero_wallet::address::Network::Stagenet,
        &keys.primary_address(),
    )
    .unwrap();
    let pair = ViewPair::new(address.spend(), Zeroizing::new(monero_wallet::ed25519::Scalar::hash(b"view"))).unwrap();
    let _: Point = pair.spend();
    let mut scanner = Scanner::new(pair);
    let t = Instant::now();
    for block in blocks {
        scanner.scan(block).unwrap();
    }
    let scanned = t.elapsed();
    println!(
        "{node}: {count} blocks ({txs} txs) fetched in {:.1} s ({:.0} blocks/s, {} requests), scanned in {:.2} s",
        fetched.as_secs_f64(),
        count as f64 / fetched.as_secs_f64(),
        transport.requests() - before,
        scanned.as_secs_f64(),
    );
}
