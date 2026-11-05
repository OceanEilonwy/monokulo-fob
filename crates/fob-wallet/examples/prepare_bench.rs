//! The node round trips a payment makes before it can be signed, timed one by one:
//! `cargo run --release -p fob-wallet --example prepare_bench -- [node]`.
//! On a phone: build with `cargo ndk -t arm64-v8a build --profile android --example prepare_bench`.

use std::time::Instant;

use fob_wallet::transport::Transport;
use monero_daemon_rpc::MoneroDaemon;
use monero_wallet::interface::{EvaluateUnlocked, FeePriority, prelude::*};
use rand_core::{OsRng, RngCore};

macro_rules! time {
    ($name:expr, $transport:expr, $runs:expr, $call:expr) => {{
        let mut times = vec![];
        let before = $transport.requests();
        for _ in 0..$runs {
            let start = Instant::now();
            $call.await.unwrap_or_else(|e| panic!("{}: {e:?}", $name));
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{:<34} median {:>6.0} ms  best {:>6.0} ms  ({} request(s) each)",
            $name,
            times[times.len() / 2],
            times[0],
            ($transport.requests() - before) / $runs
        );
    }};
}

#[tokio::main]
async fn main() {
    let node = std::env::args().nth(1).unwrap_or_else(|| "https://node.monerodevs.org:18089".into());
    let transport = Transport::new(&node, false).unwrap();
    let t = Instant::now();
    let before = transport.requests();
    let rpc = MoneroDaemon::new(transport.clone()).await.unwrap();
    println!("{node}\nconnect (first request, TLS){:>10.0} ms  ({} request(s))", t.elapsed().as_secs_f64() * 1000.0, transport.requests() - before);

    let tip = rpc.latest_block_number().await.unwrap();
    time!("get_height", transport, 7, rpc.latest_block_number());
    time!("fee estimate", transport, 7, rpc.fee_rate(FeePriority::Normal, 2_000_000));
    time!("distribution, last 40 blocks", transport, 7, rpc.ringct_output_distribution(tip - 40..=tip));

    let total = *rpc.ringct_output_distribution(tip - 5..=tip).await.unwrap().last().unwrap();
    time!("get_outs, 16 decoys", transport, 7, {
        let indexes: Vec<u64> = (0..16).map(|_| OsRng.next_u64() % (total - 20_000)).collect();
        let rpc = &rpc;
        async move { rpc.unlocked_ringct_outputs(&indexes, EvaluateUnlocked::Normal).await }
    });
}
