use super::*;

#[tokio::test]
async fn sync_now_cuts_the_wait_short_and_returns_when_the_round_ends() {
    let wallet = unreachable("sync-now").await;
    wallet.start_sync(Arc::new(Quiet));
    // Let the first round fail; the loop then waits out its backoff.
    let mut rounds = wallet.inner.rounds.subscribe();
    tokio::time::timeout(Duration::from_secs(5), rounds.changed()).await.unwrap().unwrap();
    let started = Instant::now();
    wallet.sync_now().await;
    // Sooner than the 2 s backoff it cut short.
    assert!(started.elapsed() < Duration::from_secs(1), "waited {:?}", started.elapsed());
    assert!(*wallet.inner.rounds.borrow() >= 2);
    assert!(wallet.status().error.is_some());
    wallet.stop_sync();
}

/// A new block announced by the node's publisher ends the loop's wait
/// at once: here a 4 s backoff, cut short well inside a second.
#[tokio::test]
async fn a_block_announced_over_zmq_wakes_the_sync_loop() {
    let wallet = unreachable("zmq-wake").await;
    let publisher = crate::zmq::test_publisher::Publisher::bind("tcp://127.0.0.1:0").await;
    wallet.set_zmq(true, Some(publisher.endpoint())).await.unwrap();
    assert_eq!(wallet.status().zmq, ZmqStatus::Connecting, "on, not syncing yet");
    let mut rounds = wallet.inner.rounds.subscribe();
    wallet.start_sync(Arc::new(Quiet));
    let listening = async {
        while wallet.status().zmq != ZmqStatus::Listening {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), listening).await.expect("never listening");
    assert!(wallet.said_zmq().ends_with(" · listening"), "{}", wallet.said_zmq());
    // The first round, and the one connecting wakes for what was missed.
    while *rounds.borrow_and_update() < 2 {
        tokio::time::timeout(Duration::from_secs(5), rounds.changed()).await.unwrap().unwrap();
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let settled = *rounds.borrow_and_update();
    let started = Instant::now();
    // Sent until the subscription has reached the publisher.
    let woke = async {
        loop {
            publisher.send(&format!("{}:{{}}", crate::zmq::CHAIN_TOPIC));
            if tokio::time::timeout(Duration::from_millis(50), rounds.changed()).await.is_ok() {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(3), woke).await.expect("the announcement never woke the loop");
    assert!(started.elapsed() < Duration::from_secs(1), "woke after {:?}", started.elapsed());
    assert!(*wallet.inner.rounds.borrow() > settled);
    // Pool announcements don't start a round of their own.
    let pooled = *wallet.inner.rounds.borrow();
    publisher.send(&format!("{}:[]", crate::zmq::POOL_TOPIC));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(*wallet.inner.rounds.borrow(), pooled);
    wallet.stop_sync();
    assert_eq!(wallet.status().zmq, ZmqStatus::Connecting, "still on, for when syncing starts again");
    wallet.set_zmq(false, None).await.unwrap();
    assert_eq!(wallet.status().zmq, ZmqStatus::Off);
    assert_eq!(wallet.zmq_endpoint().await, publisher.endpoint(), "kept for turning on again");
}

#[test]
fn a_wallets_gpu_check_runs_when_due_and_no_more() {
    use GpuCheck::{Due, Failed, Passed};
    assert_eq!(gpu_check_due(None, false, "A 1", false), Due, "never checked");
    assert_eq!(gpu_check_due(Some("A 1"), true, "A 1", false), Passed, "kept from before");
    assert_eq!(gpu_check_due(Some("A 1"), true, "A 1", true), Passed);
    assert_eq!(gpu_check_due(Some("A 1"), true, "A 2", false), Due, "a new driver");
    assert_eq!(gpu_check_due(Some("A 1"), false, "A 1", false), Due, "one more try after opening");
    assert_eq!(gpu_check_due(Some("A 1"), false, "A 1", true), Failed, "and only one");
}

#[tokio::test]
#[ignore = "needs a GPU; run with --ignored"]
async fn a_wallet_keeps_its_gpu_check() {
    fob_scanner::set_mode(fob_scanner::Mode::Gpu);
    for _ in 0..1200 {
        if fob_scanner::gpu_ready() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let device = fob_scanner::gpu_identity().expect("the GPU passed its cold-start check");
    let path = dir("gpu-check");
    let wallet = a_used_wallet(&path).await;
    assert!(wallet.gpu_allowed().await);
    assert!(wallet.gpu_allowed().await, "and from what it kept");
    drop(wallet);
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    {
        let mut state = reopened.inner.state.lock().await;
        assert_eq!(state.gpu_checked_on.as_deref(), Some(device.as_str()));
        assert!(state.gpu_check_passed);
        // A failure is tried again once, and a sound GPU then passes.
        state.gpu_check_passed = false;
    }
    assert!(reopened.gpu_check_failed().await);
    assert!(reopened.gpu_allowed().await);
    assert!(!reopened.gpu_check_failed().await);
}

#[tokio::test]
async fn sync_now_without_the_loop_runs_one_round_itself() {
    let wallet = unreachable("sync-now-alone").await;
    let started = Instant::now();
    wallet.sync_now().await;
    assert!(started.elapsed() < SYNC_NOW_WAIT);
}

#[test]
fn a_reorganisation_is_only_followed_as_far_as_it_can_be_checked() {
    assert_eq!(reorg_reach(100, true, 50), ReorgReach::Kept);
    assert_eq!(reorg_reach(100, false, 50), ReorgReach::TooDeep);
    assert_eq!(reorg_reach(50, false, 50), ReorgReach::ToStart, "nothing older to be wrong");
    assert_eq!(reorg_reach(50, true, 50), ReorgReach::Kept);
}

/// `a_used_wallet`'s state, with hashes from block 12 on.
async fn wallet_with_hashes(name: &str, node: &str, oldest_hash: &str) -> (Wallet, serde_json::Value) {
    let wallet = a_used_wallet(&dir(name)).await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.node = node.into();
        state.recent = vec![(12, oldest_hash.into()), (13, "hash".into())];
    }
    let before = saved(&wallet).await;
    (wallet, before)
}

#[tokio::test]
async fn a_reorganisation_within_the_hashes_scans_on_from_the_oldest_still_there() {
    let (url, _) = counting_node(Duration::ZERO).await;
    let (wallet, _) = wallet_with_hashes("reorg-kept", &url, &"aa".repeat(32)).await;
    let (_, rpc, _, _) = wallet.connection().await.unwrap();
    wallet.on_reorg(&rpc).await.unwrap();
    let state = wallet.inner.state.lock().await;
    assert_eq!(state.next_block, 13);
    assert_eq!(state.recent, [(12, "aa".repeat(32))], "the next block must follow it");
    assert_eq!(state.spends.len(), 1, "the spend in block 12 stands");
    assert_eq!(state.sent[0].height, None, "the payment in block 13 is found again");
}

#[tokio::test]
async fn a_reorganisation_past_the_hashes_is_refused_and_changes_nothing() {
    let (url, _) = counting_node(Duration::ZERO).await;
    let (wallet, before) = wallet_with_hashes("reorg-deep", &url, &"bb".repeat(32)).await;
    let (_, rpc, _, _) = wallet.connection().await.unwrap();
    assert!(matches!(wallet.on_reorg(&rpc).await, Err(FobError::ReorgTooDeep)));
    assert_eq!(saved(&wallet).await, before);
}

#[tokio::test]
async fn a_reorganisation_to_the_wallet_s_first_block_scans_it_all_again() {
    let (url, _) = counting_node(Duration::ZERO).await;
    let (wallet, _) = wallet_with_hashes("reorg-start", &url, &"bb".repeat(32)).await;
    wallet.inner.state.lock().await.restore_height = Some(12);
    let (_, rpc, _, _) = wallet.connection().await.unwrap();
    wallet.on_reorg(&rpc).await.unwrap();
    let state = wallet.inner.state.lock().await;
    assert_eq!((state.next_block, state.recent.len()), (12, 0));
}

#[tokio::test]
async fn scanning_again_goes_back_to_the_start_keeping_what_only_the_wallet_knows() {
    let (url, _) = counting_node(Duration::ZERO).await;
    let (wallet, _) = wallet_with_hashes("reorg-again", &url, &"bb".repeat(32)).await;
    wallet.set_status(|s| {
        s.chain_changed = true;
        s.error = Some("the chain changed".into());
    });
    wallet.scan_again().await.unwrap();
    assert!(!wallet.status().chain_changed);
    let state = wallet.inner.state.lock().await;
    assert_eq!(state.next_block, 1, "from the wallet's first block");
    assert!(state.outputs.iter().all(|o| o.height < 1) && state.spends.is_empty(), "found again from the chain");
    assert_eq!((state.sent.len(), state.sent[0].height, state.sent[0].note.as_deref()), (1, None, Some("Rent")));
}

#[test]
fn a_failure_of_this_device_is_told_apart_from_the_node_s() {
    let storage = FobError::storage("disk full");
    assert_eq!(sync_failure(&storage, None, false), ("Couldn't read or save the wallet: disk full".into(), true));
    let offline = FobError::Node("InterfaceError(\"dns error: failed to lookup address information\")".into());
    assert_eq!(sync_failure(&offline, None, false), ("Address not found".into(), false));
}
