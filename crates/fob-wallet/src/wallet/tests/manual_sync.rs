use super::*;

#[test]
fn the_node_says_what_it_knows_of_each_payment() {
    let answer = r#"{"status":"OK","txs":[{"tx_hash":"aa","in_pool":false,"block_height":12},{"tx_hash":"bb","in_pool":true,"block_height":0}],"missed_tx":["cc"]}"#;
    assert_eq!(sent_lookup(answer), SentLookup { mined: vec![("aa".into(), 12)], in_pool: vec!["bb".into()], missing: vec!["cc".into()] });
    assert_eq!(sent_lookup("not json"), SentLookup::default(), "no answer says nothing is missing");
    assert_eq!(sent_lookup(r#"{"status":"Failed"}"#), SentLookup::default());
}

/// Before a payment is called not confirmed, the node is asked whether its
/// coins are spent (audit #28).
#[test]
fn a_key_image_answer_is_read_only_when_it_is_whole() {
    assert_eq!(key_images_spent(r#"{"spent_status":[0,1,2],"status":"OK"}"#, 3), Some(vec![false, true, true]));
    assert_eq!(key_images_spent(r#"{"spent_status":[0],"status":"OK"}"#, 2), None, "not for as many");
    assert_eq!(key_images_spent(r#"{"spent_status":[0],"status":"BUSY"}"#, 1), None);
    assert_eq!(key_images_spent("not JSON", 1), None);
}

/// All spent or none is a clear answer, and marks the payment after a day:
/// spent, something else took its coins, and it would otherwise say it's on
/// its way for ever. Anything else waits a day more, so a node that breaks
/// the endpoint doesn't hold the coins for good (review of audit #28, #73).
#[test]
fn a_payment_whose_coins_the_node_cant_say_about_waits_a_day_more() {
    assert_eq!(CoinsSaid::of(Some(&[false, false])), CoinsSaid::Unspent);
    assert_eq!(CoinsSaid::of(Some(&[true, true])), CoinsSaid::Spent);
    assert_eq!(CoinsSaid::of(Some(&[true, false])), CoinsSaid::CantSay, "no one payment does that");
    assert_eq!(CoinsSaid::of(Some(&[])), CoinsSaid::CantSay, "no coins to ask about");
    assert_eq!(CoinsSaid::of(None), CoinsSaid::CantSay);
    let t = 10 * DAY;
    for coins in [CoinsSaid::Unspent, CoinsSaid::Spent] {
        assert!(!still_missing(coins, t, t + NOT_CONFIRMED_AFTER - 1));
        assert!(still_missing(coins, t, t + NOT_CONFIRMED_AFTER), "{coins:?}");
    }
    assert!(!still_missing(CoinsSaid::CantSay, t, t + NOT_CONFIRMED_AFTER));
    assert!(!still_missing(CoinsSaid::CantSay, t, t + NOT_CONFIRMED_UNSURE_AFTER - 1));
    assert!(still_missing(CoinsSaid::CantSay, t, t + NOT_CONFIRMED_UNSURE_AFTER));
}

#[tokio::test]
async fn the_node_is_asked_about_a_quiet_payments_coins() {
    let said = Arc::new(std::sync::Mutex::new(String::new()));
    let answer = said.clone();
    let url = scripted_node(move |text| {
        let spent = answer.lock().unwrap().clone();
        async move { if text.contains("is_key_image_spent") { spent } else { r#"{"jsonrpc":"2.0","id":0,"result":{}}"#.to_owned() } }
    })
    .await;
    let wallet = a_used_wallet(&dir("quiet-coins")).await;
    let tx = &tx_hex("out");
    let t = now();
    {
        let mut state = wallet.inner.state.lock().await;
        state.node = url;
        state.sent[0].tx = tx.clone();
        state.sent[0].height = None;
        state.sent[0].time = t - NOT_CONFIRMED_AFTER;
        state.outputs.truncate(1);
        state.outputs[0] = state.outputs[0].clone().with_coin(held_by_ours("out"));
    }
    let rpc = wallet.rpc().await.unwrap();
    let missing = SentLookup { missing: vec![tx.clone()], ..Default::default() };
    let cases = [
        (r#"{"spent_status":[0],"status":"OK"}"#, true, "unspent: it may not have gone through"),
        (r#"{"spent_status":[1],"status":"OK"}"#, true, "spent by something else: it never will"),
        (r#"{"spent_status":[0],"status":"BUSY"}"#, false, "can't say: asked again next time"),
        ("not JSON", false, "can't say"),
    ];
    for (answer, kept, why) in cases {
        *said.lock().unwrap() = answer.into();
        assert_eq!(wallet.check_missing(&rpc, missing.clone()).await.missing.contains(&tx.to_owned()), kept, "{why}");
    }

    // A day more, and a node that never answers doesn't hold the coins.
    wallet.inner.state.lock().await.sent[0].time = t - NOT_CONFIRMED_UNSURE_AFTER;
    assert!(wallet.check_missing(&rpc, missing.clone()).await.missing.contains(&tx.to_owned()));

    // No coins known to ask about, as while a rescan finds them again.
    {
        let mut state = wallet.inner.state.lock().await;
        state.sent[0].time = t - NOT_CONFIRMED_AFTER;
        let no_image = Coin::Held { ki: KeyImage::Unknown, hold: Hold::OurPayment { tx: tx_hash("out") } };
        state.outputs[0] = state.outputs[0].clone().with_coin(no_image);
    }
    *said.lock().unwrap() = r#"{"spent_status":[],"status":"OK"}"#.into();
    assert!(!wallet.check_missing(&rpc, missing.clone()).await.missing.contains(&tx.to_owned()));
    wallet.inner.state.lock().await.sent[0].time = t - NOT_CONFIRMED_UNSURE_AFTER;
    assert!(wallet.check_missing(&rpc, missing.clone()).await.missing.contains(&tx.to_owned()));

    // Spent elsewhere: marked, and its coins stay spent.
    {
        let mut state = wallet.inner.state.lock().await;
        state.sent[0].time = t - NOT_CONFIRMED_AFTER;
        state.outputs[0] = state.outputs[0].clone().with_coin(held_by_ours("out"));
    }
    *said.lock().unwrap() = r#"{"spent_status":[1],"status":"OK"}"#.into();
    let lookup = wallet.check_missing(&rpc, missing).await;
    wallet.note_missing(&lookup).await.unwrap();
    let state = wallet.inner.state.lock().await;
    assert!(state.sent[0].not_confirmed);
    assert!(state.outputs[0].spent_by(tx));
}

#[tokio::test]
async fn a_payment_the_node_never_heard_of_waits_a_day_then_its_coins_can_be_released() {
    let wallet = a_used_wallet(&dir("dropped")).await;
    let t = now();
    let tx = &tx_hex("out");
    let coin = {
        let mut state = wallet.inner.state.lock().await;
        state.sent[0].tx = tx.clone();
        state.sent[0].height = None;
        state.sent[0].time = t - NOT_CONFIRMED_AFTER + 60;
        state.sent[0].change = 7;
        state.outputs.truncate(1);
        state.outputs[0].amount = 100;
        state.outputs[0] = state.outputs[0].clone().with_coin(held_by_ours("out"));
        state.outputs[0].amount
    };
    let missing = SentLookup { missing: vec![tx.clone()], ..Default::default() };
    assert!(wallet.release_payment(tx).await.is_err(), "not until it's marked");
    wallet.note_missing(&missing).await.unwrap();
    assert!(!wallet.inner.state.lock().await.sent[0].not_confirmed, "under a day: it may yet arrive");

    wallet.inner.state.lock().await.sent[0].time = t - NOT_CONFIRMED_AFTER;
    wallet.note_missing(&missing).await.unwrap();
    {
        let state = wallet.inner.state.lock().await;
        assert!(state.sent[0].not_confirmed);
        assert!(state.outputs[0].spent_by(tx), "nothing's released by itself");
    }
    assert_eq!(wallet.activity().await.iter().find(|a| a.tx == *tx).map(|a| a.not_confirmed), Some(true));

    // Heard of again: back to waiting.
    wallet.note_missing(&SentLookup { in_pool: vec![tx.clone()], ..Default::default() }).await.unwrap();
    assert!(!wallet.inner.state.lock().await.sent[0].not_confirmed);
    wallet.note_missing(&missing).await.unwrap();

    let before = wallet.balance().await.total;
    wallet.release_payment(tx).await.unwrap();
    {
        let state = wallet.inner.state.lock().await;
        assert!(state.sent[0].released && !state.sent[0].not_confirmed);
        assert!(state.outputs[0].coin.is_free());
    }
    assert_eq!(wallet.balance().await.total, before + coin - 7, "the coin back, the change it was to bring gone");
    assert!(wallet.release_payment(tx).await.is_err(), "once");
    let row = wallet.activity().await.iter().find(|a| a.tx == *tx).cloned().unwrap();
    assert!(row.released);
    // Not asked about any more.
    assert!(Wallet::unsettled_sent(&*wallet.inner.state.lock().await).iter().all(|(hash, _)| hash != tx));
}

#[test]
fn a_refusal_is_told_apart_only_with_manual_sync_on() {
    use monero_wallet::interface::PublishTransactionError as E;
    let refused = |e, manual| match publish_failure(e, manual) {
        Unpublished::Refused(e) => e,
        Unpublished::Unsure => panic!("a refusal is an answer"),
    };
    assert_eq!(refused(E::TransactionRejected("double spend".into()), true).to_string(), "your node refused this payment: double spend");
    assert_eq!(refused(E::TransactionRejected(" ".into()), true).to_string(), "your node refused this payment", "no reason, no colon");
    assert!(matches!(refused(E::TransactionRejected("double spend".into()), false), FobError::Node(_)));
}

/// Only the node's own refusal, or a request that never reached it, says a
/// payment didn't go: a timeout or a dropped line may come after the node
/// took it (audit #24).
#[test]
fn a_publish_with_no_answer_is_not_taken_as_refused() {
    use monero_wallet::interface::{InterfaceError as I, PublishTransactionError as E};
    let never = E::InterfaceError(I::InterfaceError("not connected: error sending request: tcp connect error: Connection refused".into()));
    assert!(matches!(publish_failure(never, false), Unpublished::Refused(FobError::Node(_))));
    let unauthorised = E::InterfaceError(I::InterfaceError("node answered 401 Unauthorized".into()));
    assert!(matches!(publish_failure(unauthorised, true), Unpublished::Refused(_)));
    for unsure in ["error decoding response body: operation timed out", "connection closed before message completed", "node answered 502 Bad Gateway"] {
        assert!(matches!(publish_failure(E::InterfaceError(I::InterfaceError(unsure.into())), false), Unpublished::Unsure), "{unsure}");
    }
    assert!(matches!(publish_failure(E::InterfaceError(I::InvalidInterface("not JSON".into())), false), Unpublished::Unsure));
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
    wallet.ask_to_scan(ScanTitle::WholeWallet, 0).await;
    assert!(!wallet.waiting_to_be_asked().await);
    assert!(!wallet.status().synced, "not up to date while it scans");
    assert_eq!(wallet.status().scan.unwrap().title, ScanTitle::WholeWallet);
    wallet.scan_finished().await;
    assert!(wallet.waiting_to_be_asked().await);
    assert!(wallet.status().scan.is_none());
    // Turning it on stops one under way.
    wallet.ask_to_scan(ScanTitle::WholeWallet, 0).await;
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
    state.outputs[0] = state.outputs[0].clone().with_coin(Coin::Free { ki: KeyImage::Known([0xaa; 32]) });
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
    let said = fob_words::manual::pull(&words);
    assert_eq!(said.title, "Nothing new");
    assert_eq!(said.line, "Sync to look for new payments.");
    {
        let state = wallet.inner.state.lock().await;
        assert_eq!(state.tip, 3_000_000, "coins waiting can now count down");
        assert_eq!(state.pull_mark_hash.as_deref(), Some("a".repeat(64).as_str()));
        assert_eq!(state.next_block, 0, "Last scan stays");
    }
    // Same hash again: nothing to say. Another hash at that height: the chain changed.
    assert_eq!(fob_words::manual::pull(&wallet.pull_refresh().await.unwrap().unwrap()).title, "Nothing new");
    wallet.inner.state.lock().await.pull_mark_hash = Some("b".repeat(64));
    assert_eq!(fob_words::manual::pull(&wallet.pull_refresh().await.unwrap().unwrap()).title, "The chain changed");
    // Disabled does nothing at all.
    wallet.set_pull_mode(PullMode::Disabled).await.unwrap();
    assert_eq!(wallet.pull_refresh().await.unwrap(), None);
}

/// A varint as epee writes it.
fn epee_len(n: usize, out: &mut Vec<u8>) {
    match n {
        0..64 => out.push((n << 2) as u8),
        64..16_384 => out.extend((((n << 2) | 1) as u16).to_le_bytes()),
        _ => out.extend((((n << 2) | 2) as u32).to_le_bytes()),
    }
}

fn epee_key(name: &str, kind: u8, out: &mut Vec<u8>) {
    out.push(name.len() as u8);
    out.extend(name.as_bytes());
    out.push(kind);
}

/// A `get_blocks.bin` answer with `blocks`, each with only its miner transaction,
/// which has one output.
fn blocks_answer(blocks: &[Vec<u8>]) -> Vec<u8> {
    const STRING: u8 = 10;
    const OBJECTS: u8 = 12 | 0x80;
    const NUMBERS: u8 = 5 | 0x80;
    let mut out = b"\x01\x11\x01\x01\x01\x01\x02\x01\x01".to_vec();
    epee_len(3, &mut out);
    epee_key("status", STRING, &mut out);
    epee_len(2, &mut out);
    out.extend(b"OK");
    epee_key("blocks", OBJECTS, &mut out);
    epee_len(blocks.len(), &mut out);
    for block in blocks {
        epee_len(1, &mut out);
        epee_key("block", STRING, &mut out);
        epee_len(block.len(), &mut out);
        out.extend(block);
    }
    epee_key("output_indices", OBJECTS, &mut out);
    epee_len(blocks.len(), &mut out);
    for _ in blocks {
        epee_len(1, &mut out);
        epee_key("indices", OBJECTS, &mut out);
        epee_len(1, &mut out);
        epee_len(1, &mut out);
        epee_key("indices", NUMBERS, &mut out);
        epee_len(1, &mut out);
        out.extend(0u64.to_le_bytes());
    }
    out
}

/// A number in an epee request, after its name and type.
fn asked_for(request: &[u8], name: &[u8]) -> Option<u64> {
    let at = request.windows(name.len()).position(|w| w == name)? + name.len() + 1;
    Some(u64::from_le_bytes(request.get(at..at + 8)?.try_into().ok()?))
}

/// A node on 127.0.0.1 with `chain`, whose first block is at height 1, taking `later`
/// to answer for anything after the first. Gives its address and the height each
/// request for blocks started at.
async fn node_with_blocks(chain: Vec<Vec<u8>>, later: Duration) -> (String, Arc<std::sync::Mutex<Vec<u64>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let asked = Arc::new(std::sync::Mutex::new(vec![]));
    let seen = asked.clone();
    let chain = Arc::new(chain);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let (seen, chain) = (seen.clone(), chain.clone());
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
                let body = match (asked_for(&request, b"start_height"), asked_for(&request, b"max_block_count")) {
                    (Some(start), Some(count)) => {
                        seen.lock().unwrap().push(start);
                        if start > 1 {
                            tokio::time::sleep(later).await;
                        }
                        let from = (start as usize - 1).min(chain.len());
                        blocks_answer(&chain[from..(from + count as usize).min(chain.len())])
                    }
                    // Connecting asks something of json_rpc; any answer will do.
                    _ => br#"{"jsonrpc":"2.0","id":0,"result":{}}"#.to_vec(),
                };
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(&body).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (url, asked)
}

/// A chain of `count` blocks from height 1, each following the last but for
/// `broken_at`. Each pays a stranger, but for those in `pays`: heights, with the
/// subaddress of `wallet` each pays.
fn chain_of(count: u64, broken_at: Option<u64>, wallet: &Wallet, pays: &[(u64, u32)]) -> Vec<Vec<u8>> {
    use curve25519_dalek::{Scalar, constants::ED25519_BASEPOINT_TABLE as G, edwards::EdwardsPoint};
    use monero_wallet::{
        address::{Network as Addresses, SubaddressIndex},
        block::{Block, BlockHeader},
        ed25519::CompressedPoint,
        transaction::{Input, Output, Timelock, Transaction, TransactionPrefix},
    };
    use sha3::{Digest, Keccak256};
    let pair = wallet.inner.keys.view_pair();
    let view = Scalar::from_bytes_mod_order(*wallet.inner.keys.view_bytes());
    let mut previous = [0; 32];
    let mut chain = vec![];
    for height in 1..=count {
        let r = Scalar::from(height + 1_000);
        let key = &r * G;
        let derivation = (view * key).mul_by_cofactor().compress().to_bytes();
        let shared = Scalar::from_bytes_mod_order(Keccak256::digest([&derivation[..], &[0]].concat()).into());
        let tag = Keccak256::digest([&b"view_tag"[..], &derivation, &[0]].concat())[0];
        let to: EdwardsPoint = match pays.iter().find(|(at, _)| *at == height) {
            Some((_, subaddress)) => pair.subaddress(Addresses::Mainnet, SubaddressIndex::new(0, *subaddress).unwrap()).spend().into(),
            None => &Scalar::from(height) * G,
        };
        let output = Output { amount: Some(1_000), key: CompressedPoint::from((&shared * G + to).compress().to_bytes()), view_tag: Some(tag) };
        let extra = [&[1u8][..], key.compress().as_bytes()].concat();
        let prefix = TransactionPrefix { additional_timelock: Timelock::None, inputs: vec![Input::Gen(height as usize)], outputs: vec![output], extra };
        let follows = if broken_at == Some(height) { [9; 32] } else { previous };
        let header = BlockHeader { hardfork_version: 16, hardfork_signal: 16, timestamp: height, previous: follows, nonce: 0 };
        let block = Block::new(header, Transaction::V2 { prefix, proofs: None }, vec![]).unwrap();
        previous = block.hash();
        chain.push(block.serialize());
    }
    chain
}

async fn scanning_from(name: &str, url: String) -> Wallet {
    let wallet = Wallet::create(&dir(name), [1; 32], Network::Stagenet).unwrap();
    wallet.inner.state.lock().await.node = url;
    wallet
}

#[tokio::test]
async fn a_scan_fetches_each_block_once() {
    let scratch = Wallet::create(&dir("scan-once-chain"), [1; 32], Network::Stagenet).unwrap();
    let (url, asked) = node_with_blocks(chain_of(200, None, &scratch, &[]), Duration::ZERO).await;
    let wallet = scanning_from("scan-once", url).await;
    wallet.scan_blocks(1, 200).await.unwrap();
    let mut starts = asked.lock().unwrap().clone();
    assert_eq!(starts[0], 1);
    starts.sort_unstable();
    let before = starts.len();
    starts.dedup();
    assert_eq!(starts.len(), before, "no height was asked for twice: {starts:?}");
    assert!(starts.len() > 1, "and it took several requests: {starts:?}");
}

#[tokio::test]
async fn an_address_coming_into_use_scans_on_from_there_watching_for_more() {
    let wallet = Wallet::create(&dir("scan-further"), [1; 32], Network::Stagenet).unwrap();
    // Block 5 pays an address being watched, which moves watching on to the one block 10 pays.
    let further = LOOKAHEAD + 100;
    let chain = chain_of(60, None, &wallet, &[(5, 150), (10, further)]);
    let (url, asked) = node_with_blocks(chain, Duration::ZERO).await;
    wallet.inner.state.lock().await.node = url;
    wallet.scan_blocks(1, 60).await.unwrap();
    let paid: Vec<u32> = wallet.inner.state.lock().await.outputs.iter().map(|o| o.subaddress).collect();
    assert_eq!(paid, [150, further]);
    assert!(asked.lock().unwrap().contains(&6), "it started again from the block after the first payment");
}

#[tokio::test]
async fn a_chain_that_changes_between_batches_fails_the_scan() {
    let scratch = Wallet::create(&dir("scan-changed-chain"), [1; 32], Network::Stagenet).unwrap();
    // The first requests are 20 blocks each, and the second is late, so the first batch
    // is scanned alone and this is the first block of the next.
    let (url, _) = node_with_blocks(chain_of(60, Some(21), &scratch, &[]), Duration::from_millis(200)).await;
    let wallet = scanning_from("scan-changed", url).await;
    let failed = wallet.scan_blocks(1, 60).await.unwrap_err();
    assert!(format!("{failed:?}").contains("don't follow on"), "{failed:?}");
}
