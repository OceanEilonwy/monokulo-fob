//! Against a real public stagenet node. Ignored by default (they need the
//! network); run with `cargo test -p fob-core --test stagenet_over_http -- --ignored`.

use std::time::Instant;

use fob_core::{Network, Wallet, network::RESTORE_MARGIN};

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fob-http-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("wallet.fob")
}

#[tokio::test]
#[ignore = "needs a public stagenet node"]
async fn a_new_wallet_is_synced_after_its_first_steps_over_http() {
    let wallet = Wallet::create(&path("new"), [3; 32], Network::Stagenet).unwrap();
    let started = Instant::now();
    while wallet.sync_step().await.unwrap() {}
    assert!(started.elapsed().as_secs() < 60, "took {:?}", started.elapsed());
    assert_eq!(wallet.balance().await.total, 0);
}

#[tokio::test]
#[ignore = "needs a public stagenet node"]
async fn restoring_from_a_seed_scans_a_week_of_blocks_quickly_over_http() {
    // Restoring with today's date scans the last RESTORE_MARGIN blocks.
    let made = Wallet::create(&path("seed-a"), [3; 32], Network::Stagenet).unwrap();
    let (_, words) = made.seed_words().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let from = Network::Stagenet.restore_height_for(now);
    let wallet = Wallet::restore_words(&path("seed-b"), [3; 32], Network::Stagenet, &words, Some(from)).unwrap();
    let first = wallet.status().scanned;
    let started = Instant::now();
    loop {
        let step = Instant::now();
        let more = wallet.sync_step().await.unwrap();
        println!("step: {:?}", step.elapsed());
        if !more {
            break;
        }
    }
    let secs = started.elapsed().as_secs_f64();
    let blocks = wallet.status().tip.max(first) - first;
    println!("{blocks} blocks in {secs:.1} s ({:.0} blocks/s); a week is {RESTORE_MARGIN}", blocks as f64 / secs);
    // A week of stagenet should take well under a minute.
    assert!(secs < 60.0, "took {secs:.1} s");
    assert!(wallet.activity().await.is_empty());
}
