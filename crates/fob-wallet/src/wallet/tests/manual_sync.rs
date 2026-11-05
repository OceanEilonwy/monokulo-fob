use super::*;

#[test]
fn the_node_says_what_it_knows_of_each_payment() {
    let answer = r#"{"status":"OK","txs":[{"tx_hash":"aa","in_pool":false,"block_height":12},{"tx_hash":"bb","in_pool":true,"block_height":0}],"missed_tx":["cc"]}"#;
    assert_eq!(sent_lookup(answer), SentLookup { mined: vec![("aa".into(), 12)], in_pool: vec!["bb".into()], missing: vec!["cc".into()] });
    assert_eq!(sent_lookup("not json"), SentLookup::default(), "no answer says nothing is missing");
    assert_eq!(sent_lookup(r#"{"status":"Failed"}"#), SentLookup::default());
}

#[tokio::test]
async fn a_payment_the_node_never_heard_of_waits_a_day_then_its_coins_can_be_released() {
    let wallet = a_used_wallet(&dir("dropped")).await;
    let t = now();
    let tx = "out";
    let coin = {
        let mut state = wallet.inner.state.lock().await;
        state.sent[0].height = None;
        state.sent[0].time = t - NOT_CONFIRMED_AFTER + 60;
        state.sent[0].change = 7;
        state.outputs.truncate(1);
        state.outputs[0].amount = 100;
        state.outputs[0].spent_in = Some(tx.into());
        state.outputs[0].amount
    };
    let missing = SentLookup { missing: vec![tx.into()], ..Default::default() };
    assert!(wallet.release_payment(tx).await.is_err(), "not until it's marked");
    wallet.note_missing(&missing).await.unwrap();
    assert!(!wallet.inner.state.lock().await.sent[0].not_confirmed, "under a day: it may yet arrive");

    wallet.inner.state.lock().await.sent[0].time = t - NOT_CONFIRMED_AFTER;
    wallet.note_missing(&missing).await.unwrap();
    {
        let state = wallet.inner.state.lock().await;
        assert!(state.sent[0].not_confirmed);
        assert_eq!(state.outputs[0].spent_in.as_deref(), Some(tx), "nothing's released by itself");
    }
    assert_eq!(wallet.activity().await.iter().find(|a| a.tx == tx).map(|a| a.not_confirmed), Some(true));

    // Heard of again: back to waiting.
    wallet.note_missing(&SentLookup { in_pool: vec![tx.into()], ..Default::default() }).await.unwrap();
    assert!(!wallet.inner.state.lock().await.sent[0].not_confirmed);
    wallet.note_missing(&missing).await.unwrap();

    let before = wallet.balance().await.total;
    wallet.release_payment(tx).await.unwrap();
    {
        let state = wallet.inner.state.lock().await;
        assert!(state.sent[0].released && !state.sent[0].not_confirmed);
        assert_eq!(state.outputs[0].spent_in, None);
    }
    assert_eq!(wallet.balance().await.total, before + coin - 7, "the coin back, the change it was to bring gone");
    assert!(wallet.release_payment(tx).await.is_err(), "once");
    let row = wallet.activity().await.into_iter().find(|a| a.tx == tx).unwrap();
    assert!(row.released);
    // Not asked about any more.
    assert!(Wallet::unsettled_sent(&*wallet.inner.state.lock().await).iter().all(|(hash, _)| hash != tx));
}

#[test]
fn a_refusal_is_told_apart_only_with_manual_sync_on() {
    use monero_wallet::interface::PublishTransactionError as E;
    assert_eq!(publish_failure(E::TransactionRejected("double spend".into()), true).to_string(), "your node refused this payment: double spend");
    assert_eq!(publish_failure(E::TransactionRejected(" ".into()), true).to_string(), "your node refused this payment", "no reason, no colon");
    assert!(matches!(publish_failure(E::TransactionRejected("double spend".into()), false), FobError::Node(_)));
}

#[tokio::test]
async fn manual_sync_settings_are_kept_and_kept_within_their_limits() {
    let path = dir("manual-settings");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let settings = wallet.manual_settings().await;
    assert_eq!(settings, ManualSettings { on: false, pull_mode: PullMode::Auto, incoming_scan_blocks: 5, settled_confirmations: 10 });
    assert!(wallet.set_settled_confirmations(9).await.is_err());
    assert!(wallet.set_settled_confirmations(721).await.is_err());
    assert!(wallet.set_incoming_scan_blocks(0).await.is_err());
    wallet.set_manual_sync(true).await.unwrap();
    wallet.set_pull_mode(PullMode::Send).await.unwrap();
    wallet.set_incoming_scan_blocks(20).await.unwrap();
    wallet.set_settled_confirmations(720).await.unwrap();
    drop(wallet);
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(reopened.manual_settings().await, ManualSettings { on: true, pull_mode: PullMode::Send, incoming_scan_blocks: 20, settled_confirmations: 720 });
    assert!(reopened.status().synced, "taken as up to date");
    assert!(reopened.status().manual_sync);
}

#[tokio::test]
async fn with_manual_sync_on_nothing_scans_until_asked() {
    let wallet = Wallet::create(&dir("manual-idle"), [1; 32], Network::Stagenet).unwrap();
    assert!(!wallet.waiting_to_be_asked().await, "off: the loop scans");
    wallet.set_manual_sync(true).await.unwrap();
    assert!(wallet.waiting_to_be_asked().await);
    wallet.ask_to_scan("Scanning", 0).await;
    assert!(!wallet.waiting_to_be_asked().await);
    assert!(!wallet.status().synced, "not up to date while it scans");
    assert_eq!(wallet.status().scan.unwrap().title, "Scanning");
    wallet.scan_finished().await;
    assert!(wallet.waiting_to_be_asked().await);
    assert!(wallet.status().scan.is_none());
    // Turning it on stops one under way.
    wallet.ask_to_scan("Scanning", 0).await;
    wallet.set_manual_sync(true).await.unwrap();
    assert!(wallet.waiting_to_be_asked().await);
}

#[tokio::test]
async fn manual_sync_spends_at_settled_confirmations_and_says_what_is_waiting() {
    let wallet = a_used_wallet(&dir("manual-settled")).await;
    let mut state = wallet.inner.state.lock().await;
    state.outputs.truncate(1);
    state.outputs[0].coinbase = false;
    state.outputs[0].unlock_block = 0;
    state.outputs[0].spent_in = None;
    state.outputs[0].height = 100;
    state.tip = 115;
    let amount = state.outputs[0].amount;
    assert_eq!(state.spendable(), amount, "15 confirmations are enough without it");
    state.manual_sync = true;
    state.settled_confirmations = 20;
    assert_eq!(state.spendable(), 0, "not until it's settled");
    state.tip = 120;
    assert_eq!(state.spendable(), amount);
}

#[tokio::test]
async fn scanning_from_last_scan_replaces_what_pulls_found_past_it() {
    let wallet = a_used_wallet(&dir("manual-from-last")).await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.manual_sync = true;
        let mut found = state.outputs[0].clone();
        found.height = 500;
        found.key = "found".into();
        state.outputs.push(found);
        state.tip = 600;
    }
    assert_eq!(wallet.start_scan(ScanRequest::FromLast).await.unwrap(), None);
    let state = wallet.inner.state.lock().await;
    assert!(state.outputs.iter().all(|o| o.key != "found"), "found again if it's still on the chain");
    assert_eq!(state.next_block, 14);
    assert_eq!(state.recent.last(), Some(&(13, "hash".to_owned())), "the next block must follow it");
    drop(state);
    assert!(!wallet.waiting_to_be_asked().await);
}

#[tokio::test]
async fn a_pull_in_send_mode_checks_the_chain_and_says_when_it_changed() {
    let (url, _) = counting_node(Duration::ZERO).await;
    let wallet = Wallet::create(&dir("manual-pull"), [1; 32], Network::Stagenet).unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.node = url;
        state.manual_sync = true;
        state.pull_mode = PullMode::Send;
    }
    let words = wallet.pull_refresh().await.unwrap().unwrap();
    assert_eq!(words.title, "Nothing new");
    assert_eq!(words.line, "Sync to look for new payments.");
    {
        let state = wallet.inner.state.lock().await;
        assert_eq!(state.tip, 3_000_000, "coins waiting can now count down");
        assert_eq!(state.pull_mark_hash.as_deref(), Some("a".repeat(64).as_str()));
        assert_eq!(state.next_block, 0, "Last scan stays");
    }
    // Same hash again: nothing to say. Another hash at that height: the chain changed.
    assert_eq!(wallet.pull_refresh().await.unwrap().unwrap().title, "Nothing new");
    wallet.inner.state.lock().await.pull_mark_hash = Some("b".repeat(64));
    assert_eq!(wallet.pull_refresh().await.unwrap().unwrap().title, "The chain changed");
    // Disabled does nothing at all.
    wallet.set_pull_mode(PullMode::Disabled).await.unwrap();
    assert_eq!(wallet.pull_refresh().await.unwrap(), None);
}
