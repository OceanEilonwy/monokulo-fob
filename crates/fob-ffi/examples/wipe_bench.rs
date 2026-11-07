//! What wiping freed memory costs, on work shaped like a catch-up and a save:
//!
//!   cargo run --release -p fob-ffi --example wipe_bench
//!   cargo run --release -p fob-ffi --example wipe_bench --no-default-features
//!
//! The first runs with fob-ffi's wiping allocator, the second without. Each
//! "block" is a blob read into transactions and their outputs, the way
//! parsing allocates, with one point multiplication per output as scanning
//! does; the save is the wallet state to and from JSON.

use std::{hint::black_box, time::Instant};

use curve25519_dalek::{constants::ED25519_BASEPOINT_POINT, scalar::Scalar};

// Linking fob-ffi brings its allocator in, as the app does.
use fob_ffi as _;

fn block(seed: u64) -> usize {
    // A block's bytes, then its transactions, each with its own buffers.
    let blob: Vec<u8> = (0..60_000u64).map(|i| (i ^ seed) as u8).collect();
    let mut outputs = 0;
    for tx in blob.chunks(1_500) {
        let bytes = tx.to_vec();
        let keys: Vec<[u8; 32]> = bytes.chunks_exact(32).take(2).map(|c| c.try_into().unwrap()).collect();
        let mut extra = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            extra.extend_from_slice(key);
            let derivation = Scalar::from_bytes_mod_order(*key) * ED25519_BASEPOINT_POINT;
            black_box(derivation.compress());
            outputs += 1 + i;
        }
        black_box((bytes, extra));
    }
    outputs
}

fn state() -> serde_json::Value {
    let outputs: Vec<_> = (0..2_000)
        .map(|i| serde_json::json!({ "tx": format!("{i:064x}"), "height": 3_000_000 + i, "amount": i * 1_000_000, "key": format!("{:064x}", i * 7), "spent_in": null }))
        .collect();
    serde_json::json!({ "outputs": outputs, "recent_blocks": (0..100).map(|i| format!("{i:064x}")).collect::<Vec<_>>() })
}

fn time(name: &str, runs: u32, mut work: impl FnMut()) {
    work();
    let start = Instant::now();
    for _ in 0..runs {
        work();
    }
    println!("{name:<24} {:>9.3} ms", start.elapsed().as_secs_f64() * 1e3 / f64::from(runs));
}

fn main() {
    println!("wiping freed memory: {}", cfg!(feature = "wipe-freed"));
    let mut seed = 0;
    time("block (parse + scan)", 200, || {
        seed += 1;
        black_box(block(seed));
    });
    let value = state();
    time("save (state to JSON)", 50, || {
        black_box(serde_json::to_string(&value).unwrap());
    });
    let text = serde_json::to_string(&value).unwrap();
    time("open (JSON to state)", 50, || {
        black_box(serde_json::from_str::<serde_json::Value>(&text).unwrap());
    });
}
