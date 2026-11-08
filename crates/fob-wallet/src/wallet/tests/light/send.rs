//! Sending through a light wallet server: the fee rate and its caps, each
//! input's ring from the server's members, and a payment prepared, signed,
//! submitted and recorded against a fake server, then seen in its pool and
//! mined.

use super::*;
use crate::wallet::light_send::{light_fee_rate, refusal, ring, rings, submitted};
use std::sync::Mutex as StdMutex;

fn unspent(per_byte_fee: u64, fee_mask: u64, fees: Vec<u64>) -> lws::UnspentOuts {
    lws::UnspentOuts { per_byte_fee: lws::Amount(per_byte_fee), fee_mask: lws::Amount(fee_mask), amount: lws::Amount(0), lookahead_fail: None, outputs: vec![], fees }
}

/// The node path's priority from `fees` (Normal, the second), the plain
/// `per_byte_fee` from a server without them, and nothing past the caps.
#[test]
fn the_fee_rate_is_the_server_s_within_the_caps() {
    let normal = light_fee_rate(&unspent(20_000, 10_000, vec![20_000, 80_000, 320_000, 4_000_000])).unwrap();
    assert_eq!(normal, FeeRate::new(80_000, 10_000).unwrap(), "Normal, as the node path pays");
    let older = light_fee_rate(&unspent(20_000, 10_000, vec![])).unwrap();
    assert_eq!(older, FeeRate::new(20_000, 10_000).unwrap(), "monero-lws before `fees`");
    assert_eq!(light_fee_rate(&unspent(20_000, 10_000, vec![20_000, MAX_FEE_PER_WEIGHT + 1])), Err(Problem::FeeTooHigh));
    assert_eq!(light_fee_rate(&unspent(MAX_FEE_PER_WEIGHT + 1, 10_000, vec![])), Err(Problem::FeeTooHigh));
    assert_eq!(light_fee_rate(&unspent(20_000, MAX_FEE_MASK + 1, vec![])), Err(Problem::FeeTooHigh), "a mask that rounds every fee up");
    assert!(light_fee_rate(&unspent(MAX_FEE_PER_WEIGHT, MAX_FEE_MASK, vec![])).is_ok(), "the caps themselves pass");
    assert_eq!(light_fee_rate(&unspent(0, 10_000, vec![])), Err(Problem::Unreadable));
    assert_eq!(light_fee_rate(&unspent(20_000, 0, vec![])), Err(Problem::Unreadable));
    assert_eq!(light_fee_rate(&unspent(20_000, 10_000, vec![20_000])), Err(Problem::Unreadable), "a priority missing");
}

fn point() -> [u8; 32] {
    (&random_scalar() * ED25519_BASEPOINT_TABLE).compress().to_bytes()
}

/// A coin at global index `index`, made up: its ring can be built, though
/// nobody could sign for it.
fn coin(index: u64) -> WalletOutput {
    let bytes = wallet_output_bytes(&WalletOutputParts {
        tx: [3; 32],
        index: 0,
        global_index: index,
        key: point(),
        key_offset: Scalar::from(random_scalar()),
        mask: Scalar::from(random_scalar()),
        amount: XMR,
        timelock: Timelock::None,
        subaddress: None,
        payment_id: None,
    });
    WalletOutput::read(&mut bytes.as_slice()).unwrap()
}

fn member(index: u64) -> lws::RandomOutput {
    lws::RandomOutput { global_index: lws::Amount(index), public_key: lws::Hex(point()), rct: lws::HexBytes(point().to_vec()) }
}

/// `RING_LEN` members at well spread global indices, none of them `coin`'s.
fn members() -> lws::RandomOutputs {
    lws::RandomOutputs { amount: lws::Amount(0), outputs: (0..u64::from(RING_LEN)).map(|i| member(1_000 + i * 7_919)).collect() }
}

/// The ring as monero-oxide checks it: `RING_LEN` members by global
/// index, the real one at the signer's place with its own key and
/// commitment, and the rest from the server's members.
fn holds(built: &OutputWithDecoys, real: &WalletOutput, given: &lws::RandomOutputs) {
    let decoys = built.decoys();
    assert_eq!(decoys.len(), usize::from(RING_LEN));
    let indices = decoys.positions();
    assert!(indices.windows(2).all(|w| w[0] < w[1]), "sorted, each once");
    let signer = usize::from(decoys.signer_index());
    assert_eq!(indices[signer], real.index_on_blockchain());
    assert_eq!(decoys.ring()[signer], [real.key(), real.commitment().commit()]);
    assert_eq!(built.key(), real.key());
    assert_eq!(built.key_offset(), real.key_offset());
    let offered: HashMap<u64, [u8; 32]> = given.outputs.iter().map(|m| (m.global_index.0, m.public_key.0)).collect();
    for (i, (index, [key, _])) in indices.iter().zip(decoys.ring()).enumerate().filter(|(i, _)| *i != signer) {
        assert_eq!(offered.get(index), Some(&key.compress().to_bytes()), "member {i} is the server's");
    }
}

/// A ring from the server's sixteen members: fifteen of them and the real
/// coin; the member that is the real coin is the one dropped.
#[test]
fn a_ring_is_built_from_the_server_s_members() {
    let real = coin(50_000);
    let given = members();
    holds(&ring(&real, &given, &mut OsRng).unwrap(), &real, &given);

    // The server happened to pick the real coin: that member goes.
    let mut colliding = members();
    colliding.outputs[4] = lws::RandomOutput {
        global_index: lws::Amount(50_000),
        public_key: lws::Hex(real.key().compress().to_bytes()),
        rct: lws::HexBytes(real.commitment().commit().compress().to_bytes().to_vec()),
    };
    let built = ring(&real, &colliding, &mut OsRng).unwrap();
    holds(&built, &real, &colliding);
    let mut in_ring: Vec<u64> = built.decoys().positions();
    in_ring.retain(|i| *i != 50_000);
    let mut rest: Vec<u64> = colliding.outputs.iter().map(|m| m.global_index.0).filter(|i| *i != 50_000).collect();
    rest.sort_unstable();
    assert_eq!(in_ring, rest, "every other member kept");

    // 96 bytes of `rct`, as for old clients: the commitment is the first 32.
    let mut long = members();
    for m in &mut long.outputs {
        m.rct.0.extend_from_slice(&[0; 64]);
    }
    holds(&ring(&real, &long, &mut OsRng).unwrap(), &real, &long);

    // A member with torsion can't be in a ring: dropped, and one is spare.
    let mut torsioned = members();
    let torsion = |bytes: [u8; 32]| (CompressedPoint::from(bytes).decompress().unwrap().into() + curve25519_dalek::constants::EIGHT_TORSION[1]).compress().to_bytes();
    torsioned.outputs[2].public_key = lws::Hex(torsion(torsioned.outputs[2].public_key.0));
    let built = ring(&real, &torsioned, &mut OsRng).unwrap();
    assert!(!built.decoys().positions().contains(&torsioned.outputs[2].global_index.0));
    // Two are one too many.
    torsioned.outputs[3].rct = lws::HexBytes(torsion(<[u8; 32]>::try_from(&torsioned.outputs[3].rct.0[..]).unwrap()).to_vec());
    assert_eq!(ring(&real, &torsioned, &mut OsRng).unwrap_err(), Problem::BadRing);
}

/// What the server gives must be a ring's: refused whole otherwise.
#[test]
fn a_ring_the_server_gets_wrong_is_refused() {
    let real = coin(50_000);
    let bad = |change: &dyn Fn(&mut lws::RandomOutputs)| {
        let mut given = members();
        change(&mut given);
        ring(&real, &given, &mut OsRng).unwrap_err()
    };
    assert_eq!(bad(&|g| { g.outputs.pop(); }), Problem::BadRing, "too few");
    assert_eq!(bad(&|g| g.outputs.push(member(999_999))), Problem::BadRing, "too many");
    assert_eq!(bad(&|g| g.amount = lws::Amount(1)), Problem::BadRing, "not RingCT");
    assert_eq!(bad(&|g| g.outputs[5].global_index = g.outputs[6].global_index), Problem::BadRing, "one named twice");
    assert_eq!(bad(&|g| g.outputs[1].public_key = lws::Hex([0xff; 32])), Problem::BadRing, "a key that isn't a point");
    assert_eq!(bad(&|g| g.outputs[1].rct = lws::HexBytes(vec![0xff; 32])), Problem::BadRing, "a commitment that isn't a point");
    assert_eq!(bad(&|g| g.outputs[1].rct = lws::HexBytes(vec![1; 64])), Problem::BadRing, "an `rct` of neither length");
    assert_eq!(bad(&|g| g.outputs[1].global_index = lws::Amount(50_000)), Problem::BadRing, "the real coin with another key");
    // One answer for several inputs: a ring each, in order, or nothing.
    let reals = [coin(50_000), coin(60_000)];
    let answer = lws::RandomOuts { amount_outs: vec![members(), members()] };
    let built = rings(&reals, &answer, &mut OsRng).unwrap();
    holds(&built[1], &reals[1], &answer.amount_outs[1]);
    assert_eq!(rings(&reals, &lws::RandomOuts { amount_outs: vec![members()] }, &mut OsRng).unwrap_err(), Problem::BadRing);
}

/// The server's answer to a submission: taken, held without being passed
/// on, turned down in its words, or no answer.
#[test]
fn a_submission_is_taken_refused_or_unsure() {
    let said = |status: &str, reason: Option<&str>, not_relayed: Option<bool>| Ok(lws::SubmitResponse { status: status.into(), reason: reason.map(str::to_owned), not_relayed });
    assert!(submitted(said("OK", None, Some(false))).is_ok());
    assert!(submitted(said("OK", None, None)).is_ok());
    assert!(matches!(submitted(said("OK", Some("Not relayed"), Some(true))), Err(Unpublished::Unsure)), "its node holds it, the network may not");
    assert!(matches!(submitted(Err(Problem::Unavailable)), Err(Unpublished::Refused(_))), "503: its node wasn't reached, so it wasn't sent");
    match submitted(said("Failed", Some("double spend"), Some(true))) {
        Err(Unpublished::Refused(e)) => assert_eq!(e.to_string(), "The light server's node turned the payment down: double spend"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(submitted(Err(Problem::Unreachable("timed out".into()))), Err(Unpublished::Unsure)));
    assert!(matches!(submitted(Err(Problem::Unreachable("not connected: connection refused".into()))), Err(Unpublished::Refused(_))), "never reached");
    assert!(matches!(submitted(Err(Problem::BadRequest("an empty transaction".into()))), Err(Unpublished::Refused(_))));
    assert_eq!(refusal("double\nspend\u{7}  and\tmore"), "double spend and more");
    assert_eq!(refusal(&"x".repeat(300)).chars().count(), 121);
}

/// A fake light wallet server for sending: `/get_address_txs` answers
/// whatever `txs` holds then, `/get_unspent_outs` the fee in `fee`,
/// `/get_random_outs` sixteen made-up members per ring asked, and
/// `/submit_raw_tx` as `submit` says, keeping each transaction handed in.
struct SendServer {
    url: String,
    txs: Arc<StdMutex<String>>,
    fee: Arc<StdMutex<String>>,
    submit: Arc<StdMutex<(u16, String)>>,
    submitted: Arc<StdMutex<Vec<Vec<u8>>>>,
}

async fn send_server() -> SendServer {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let server = SendServer {
        url,
        txs: Arc::new(StdMutex::new(serde_json::to_string(&answer(vec![], 2_249_100)).unwrap())),
        fee: Arc::new(StdMutex::new(r#"{"per_byte_fee":20000,"fee_mask":10000,"amount":"0","fees":[20000,80000,320000,4000000]}"#.into())),
        submit: Arc::new(StdMutex::new((200, r#"{"status":"OK","not_relayed":false}"#.into()))),
        submitted: Arc::default(),
    };
    let (txs, fee, submit, submitted) = (server.txs.clone(), server.fee.clone(), server.submit.clone(), server.submitted.clone());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let (txs, fee, submit, submitted) = (txs.clone(), fee.clone(), submit.clone(), submitted.clone());
            tokio::spawn(async move {
                let mut request = vec![];
                let mut buf = [0; 65536];
                let (head, body) = loop {
                    let Ok(n) = stream.read(&mut buf).await else { return };
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break (head, request[end + 4..end + 4 + len].to_vec());
                    }
                };
                let route = head.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_owned();
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                let (status, text) = match route.as_str() {
                    "get_version" => (200, VERSION.to_owned()),
                    "login" => (200, r#"{"new_address":false,"start_height":1}"#.to_owned()),
                    "upsert_subaddrs" => (200, r#"{"new_subaddrs":[]}"#.to_owned()),
                    "get_address_txs" => (200, txs.lock().unwrap().clone()),
                    "get_unspent_outs" => (200, fee.lock().unwrap().clone()),
                    "get_random_outs" => {
                        assert_eq!(body["count"], u64::from(RING_LEN), "a ring's worth each");
                        let asked = body["amounts"].as_array().unwrap();
                        assert!(asked.iter().all(|a| a == "0"), "RingCT's amount, as a string");
                        // As monero-lws sends them: `rct` the commitment and 64 zero bytes.
                        let rings = (0..asked.len())
                            .map(|_| {
                                let mut ring = members();
                                ring.outputs.iter_mut().for_each(|m| m.rct.0.extend_from_slice(&[0; 64]));
                                ring
                            })
                            .collect();
                        (200, serde_json::to_string(&lws::RandomOuts { amount_outs: rings }).unwrap())
                    }
                    "submit_raw_tx" => {
                        submitted.lock().unwrap().push(hex::decode(body["tx"].as_str().unwrap()).unwrap());
                        submit.lock().unwrap().clone()
                    }
                    _ => (404, String::new()),
                };
                let response = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    server
}

impl SendServer {
    /// `/get_address_txs` lists `transactions` from now on, scanned to `scanned`.
    fn lists(&self, transactions: Vec<lws::Transaction>, scanned: u64) {
        *self.txs.lock().unwrap() = serde_json::to_string(&answer(transactions, scanned)).unwrap();
    }
}

/// A wallet syncing through `server`, holding one coin of 1 XMR paid to it
/// at 2,249,000 (global index 5,000,000), ready to spend at 2,249,100.
async fn light_wallet_with_a_coin(name: &str, server: &SendServer) -> (Wallet, Paid) {
    let wallet = Wallet::create(&dir(name), [1; 32], Network::Stagenet).unwrap();
    let (_, hash, paid) = transaction(&wallet.inner.keys, Some((XMR, 0)), &[]);
    let paid = paid.unwrap();
    server.lists(vec![listed(hash, Some(2_249_000), vec![received(&paid, 5_000_000)], vec![])], 2_249_100);
    wallet.set_light_server(&server.url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.spendable(), XMR, "found, and ready");
    (wallet, paid)
}

/// A payment through the server: its fee from `/get_unspent_outs`, its
/// ring from `/get_random_outs`, signed here and handed to
/// `/submit_raw_tx`; recorded as the node path records it (the coin spent,
/// the payment with its fee and change); on its way while the server
/// lists it in its pool; then mined, with its height and the spend's fee.
#[tokio::test]
async fn a_payment_through_the_server_is_built_submitted_and_recorded() {
    let server = send_server().await;
    let (wallet, paid) = light_wallet_with_a_coin("light-send", &server).await;
    let keys = &wallet.inner.keys;
    assert!(wallet.send_notice().await.is_none(), "Send is open through a server");
    let to = keys.address(9);
    let prepared = wallet.prepare(&to, XMR / 10, Some("Lunch".into()), None).await.unwrap();
    assert!(prepared.fee > 0);
    // Normal's 80,000 a byte from the server's `fees`, for a transaction of 1–2 kB.
    assert!(prepared.fee > 80_000 * 1_000 && prepared.fee < 80_000 * 3_000, "{}", prepared.fee);
    assert_eq!(*wallet.inner.fee_rate.lock().unwrap(), Some((80_000, 10_000)), "kept for Split up");
    let hash = wallet.send(prepared.id).await.unwrap();

    // What the server was handed: the transaction, spending the coin with
    // a ring of sixteen around its global index.
    let handed = server.submitted.lock().unwrap().clone();
    assert_eq!(handed.len(), 1);
    let tx = monero_wallet::transaction::Transaction::<monero_wallet::transaction::NotPruned>::read(&mut handed[0].as_slice()).unwrap();
    assert_eq!(hex::encode(tx.hash()), hash);
    let monero_wallet::transaction::Input::ToKey { key_offsets, key_image, .. } = &tx.prefix().inputs[0] else { panic!("a RingCT input") };
    assert_eq!(key_image.to_bytes(), image_of(keys, &paid), "the coin's own key image");
    assert_eq!(key_offsets.len(), usize::from(RING_LEN));
    let positions: Vec<u64> = key_offsets.iter().scan(0, |at, o| { *at += o; Some(*at) }).collect();
    assert!(positions.contains(&5_000_000), "its own place on the chain is in the ring");

    // Recorded as the node path records a payment.
    {
        let state = wallet.inner.state.lock().await;
        assert_eq!(state.outputs[0].spent_in.as_deref(), Some(hash.as_str()));
        let sent = state.sent.iter().find(|s| s.tx == hash).unwrap();
        assert_eq!((sent.amount, sent.fee, sent.to.as_str(), sent.note.as_deref()), (XMR / 10, prepared.fee, to.as_str(), Some("Lunch")));
        assert_eq!(sent.change, XMR - XMR / 10 - prepared.fee);
        assert_eq!((sent.height, sent.signed.as_ref()), (None, None));
    }

    // In the server's pool: this wallet's own payment, not money coming in.
    server.lists(
        vec![listed(hex::decode(&hash).unwrap().try_into().unwrap(), None, vec![], vec![]), wallet_coin_tx(&paid, &server)],
        2_249_101,
    );
    wallet.sync_step().await.unwrap();
    assert!(wallet.inner.pool.lock().unwrap().is_empty());
    assert_eq!(wallet.inner.state.lock().await.outputs[0].spent_in.as_deref(), Some(hash.as_str()), "still spent");

    // Mined: the payment gets its height, the spend its fee.
    let mut mined = listed(hex::decode(&hash).unwrap().try_into().unwrap(), Some(2_249_102), vec![], vec![spend_of(&paid, image_of(keys, &paid))]);
    mined.fee = Some(lws::Amount(prepared.fee));
    server.lists(vec![wallet_coin_tx(&paid, &server), mined], 2_249_103);
    wallet.sync_step().await.unwrap();
    let state = wallet.inner.state.lock().await;
    assert_eq!(state.sent[0].height, Some(2_249_102));
    assert_eq!(state.spends.iter().map(|s| (s.tx.as_str(), s.fee)).collect::<Vec<_>>(), vec![(hash.as_str(), prepared.fee)]);
    drop(state);
    let rows: Vec<_> = wallet.said_lists(clock_now()).await.days.into_iter().flat_map(|d| d.rows).filter(|r| r.activity.as_ref().is_some_and(|a| a.tx == hash)).collect();
    assert_eq!(rows.len(), 1, "one row for the payment: {rows:?}");
}

/// The transaction that paid the coin, as the server keeps listing it.
fn wallet_coin_tx(paid: &Paid, server: &SendServer) -> lws::Transaction {
    let first: lws::AddressTxs = serde_json::from_str(&server.txs.lock().unwrap()).unwrap();
    first.transactions.into_iter().find(|t| t.receives.iter().any(|r| r.public_key.0 == paid.key)).unwrap()
}

/// The server's node turning the payment down: its reason said, nothing
/// recorded, the coin still free. A fee past the cap: nothing built.
#[tokio::test]
async fn a_payment_the_server_refuses_is_not_recorded() {
    let server = send_server().await;
    let (wallet, _) = light_wallet_with_a_coin("light-send-refused", &server).await;
    *server.submit.lock().unwrap() = (422, r#"{"status":"Failed","reason":"double spend","not_relayed":true}"#.into());
    let prepared = wallet.prepare(&wallet.inner.keys.address(9), XMR / 10, None, None).await.unwrap();
    let refused = wallet.send(prepared.id).await.unwrap_err();
    assert_eq!(refused.to_string(), "The light server's node turned the payment down: double spend");
    let state = wallet.inner.state.lock().await;
    assert!(state.sent.is_empty());
    assert!(state.outputs[0].spent_in.is_none());
    drop(state);

    *server.fee.lock().unwrap() = r#"{"per_byte_fee":20000,"fee_mask":10000,"amount":"0","fees":[20000,8000000,32000000,400000000]}"#.into();
    let too_dear = wallet.prepare(&wallet.inner.keys.address(9), XMR / 10, None, None).await.unwrap_err();
    assert_eq!(too_dear, FobError::Light(Problem::FeeTooHigh));
    assert_eq!(too_dear.to_string(), "The light server asked a network fee far above the usual, so nothing was sent");
}

/// A server whose node kept the payment without passing it on (or that
/// gave no answer) may have it out yet: the payment is recorded with the
/// signed transaction kept, and offered again at the next poll until the
/// server takes it.
#[tokio::test]
async fn a_payment_the_server_gave_no_answer_for_is_offered_again() {
    let server = send_server().await;
    let (wallet, _) = light_wallet_with_a_coin("light-send-unsure", &server).await;
    *server.submit.lock().unwrap() = (200, r#"{"status":"OK","reason":"Not relayed","not_relayed":true}"#.into());
    let prepared = wallet.prepare(&wallet.inner.keys.address(9), XMR / 10, None, None).await.unwrap();
    let hash = wallet.send(prepared.id).await.unwrap();
    assert!(wallet.inner.state.lock().await.sent[0].signed.is_some(), "kept to offer again");
    *server.submit.lock().unwrap() = (200, r#"{"status":"OK"}"#.into());
    wallet.sync_step().await.unwrap();
    let handed = server.submitted.lock().unwrap().clone();
    assert_eq!(handed.len(), 2);
    assert_eq!(handed[0], handed[1], "the same transaction again");
    let state = wallet.inner.state.lock().await;
    assert_eq!((state.sent[0].tx.as_str(), state.sent[0].signed.as_ref()), (hash.as_str(), None), "taken: nothing more to offer");
}

/// The server can list the spend before Fob has recorded the payment (a
/// poll between the submission and the record): both are kept as one,
/// and the polls after change nothing.
#[tokio::test]
async fn a_spend_the_server_lists_before_the_record_is_one_payment() {
    let server = send_server().await;
    let (wallet, paid) = light_wallet_with_a_coin("light-send-early", &server).await;
    let keys = &wallet.inner.keys;
    let prepared = wallet.prepare(&keys.address(9), XMR / 10, None, None).await.unwrap();
    let PreparedTx { signable, out, .. } = wallet.inner.prepared.lock().await.take(prepared.id).unwrap();
    let tx = signable.sign(&mut OsRng, &keys.spend().unwrap()).unwrap();
    let hash = hex::encode(tx.hash());
    let mut mined = listed(tx.hash(), Some(2_249_102), vec![], vec![spend_of(&paid, image_of(keys, &paid))]);
    mined.fee = Some(lws::Amount(prepared.fee));
    server.lists(vec![wallet_coin_tx(&paid, &server), mined], 2_249_103);
    let polling = wallet.clone();
    let publish = async move {
        // Taken, and mined, and seen by a poll before it's recorded.
        polling.sync_step().await.map_err(Unpublished::Refused)?;
        Ok(())
    };
    wallet.publish_and_record(hash.clone(), prepared.fee, out, hex::encode(tx.serialize()), publish).await.unwrap();
    wallet.sync_step().await.unwrap();
    let saved = found(&*wallet.inner.state.lock().await);
    let sent_saved = serde_json::to_value(&wallet.inner.state.lock().await.sent).unwrap();
    {
        let state = wallet.inner.state.lock().await;
        assert_eq!(state.sent.len(), 1);
        assert_eq!(state.sent[0].height, Some(2_249_102));
        assert_eq!(state.spends.len(), 1);
        assert_eq!(state.outputs[0].spent_in.as_deref(), Some(hash.as_str()));
    }
    wallet.sync_step().await.unwrap();
    assert_eq!(found(&*wallet.inner.state.lock().await), saved, "the next poll changes nothing");
    assert_eq!(serde_json::to_value(&wallet.inner.state.lock().await.sent).unwrap(), sent_saved);
    let rows: Vec<_> = wallet.said_lists(clock_now()).await.days.into_iter().flat_map(|d| d.rows).filter(|r| r.activity.as_ref().is_some_and(|a| a.tx == hash)).collect();
    assert_eq!(rows.len(), 1);
}

/// What `prepare` asks of the server for a payment of several coins: one
/// `/get_random_outs` for all of them.
#[tokio::test]
async fn several_coins_take_one_ring_each() {
    let server = send_server().await;
    let wallet = Wallet::create(&dir("light-send-several"), [1; 32], Network::Stagenet).unwrap();
    let keys = &wallet.inner.keys;
    let coins: Vec<_> = (0..3).map(|i| {
        let (_, hash, paid) = transaction(keys, Some((XMR / 4, 0)), &[]);
        let paid = paid.unwrap();
        (listed(hash, Some(2_249_000), vec![received(&paid, 5_000_000 + i)], vec![]), paid)
    }).collect();
    server.lists(coins.iter().map(|c| c.0.clone()).collect(), 2_249_100);
    wallet.set_light_server(&server.url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    wallet.sync_step().await.unwrap();
    let prepared = wallet.prepare(&keys.address(9), XMR / 2 + XMR / 10, None, None).await.unwrap();
    wallet.send(prepared.id).await.unwrap();
    let handed = server.submitted.lock().unwrap().clone();
    let tx = monero_wallet::transaction::Transaction::<monero_wallet::transaction::NotPruned>::read(&mut handed[0].as_slice()).unwrap();
    assert_eq!(tx.prefix().inputs.len(), 3);
    let images: HashSet<[u8; 32]> = tx.prefix().inputs.iter().map(|i| match i {
        monero_wallet::transaction::Input::ToKey { key_image, .. } => key_image.to_bytes(),
        _ => panic!("a RingCT input"),
    }).collect();
    assert_eq!(images, coins.iter().map(|c| image_of(keys, &c.1)).collect());
}

/// A chain as monero-oxide's decoy picker sees it: 2,001 blocks of 100
/// outputs each, every output unlocked, each with a key and commitment of
/// its own, `real`'s where it is.
struct Picker {
    real: (u64, [monero_wallet::ed25519::Point; 2]),
}

impl Picker {
    fn at(&self, index: u64) -> [monero_wallet::ed25519::Point; 2] {
        if index == self.real.0 {
            return self.real.1;
        }
        let point = |tag: u8| {
            let mut bytes = [tag; 32];
            bytes[..8].copy_from_slice(&index.to_le_bytes());
            monero_wallet::ed25519::Point::biased_hash(bytes)
        };
        [point(1), point(2)]
    }
}

impl monero_wallet::interface::ProvidesBlockchainMeta for Picker {
    fn latest_block_number(&self) -> impl Send + std::future::Future<Output = std::result::Result<usize, monero_wallet::interface::InterfaceError>> {
        async { Ok(2_000) }
    }
}

impl monero_wallet::interface::ProvidesUnvalidatedDecoys for Picker {
    fn ringct_output_distribution(
        &self,
        range: impl Send + std::ops::RangeBounds<usize>,
    ) -> impl Send + std::future::Future<Output = std::result::Result<Vec<u64>, monero_wallet::interface::InterfaceError>> {
        let end = match range.end_bound() {
            std::ops::Bound::Included(e) => *e + 1,
            std::ops::Bound::Excluded(e) => *e,
            std::ops::Bound::Unbounded => 2_001,
        };
        async move { Ok((0..end as u64).map(|b| (b + 1) * 100).collect()) }
    }

    fn unlocked_ringct_outputs(
        &self,
        indexes: &[u64],
        _: monero_wallet::interface::EvaluateUnlocked,
    ) -> impl Send + std::future::Future<Output = std::result::Result<Vec<Option<[monero_wallet::ed25519::Point; 2]>>, monero_wallet::interface::TransactionsError>> {
        let outputs = indexes.iter().map(|i| Some(self.at(*i))).collect();
        async move { Ok(outputs) }
    }
}

/// The light path's ring is monero-oxide's, byte for byte: given the
/// decoys its own picker chose (and the real coin as the spare member,
/// which is the one dropped), `ring` makes the `OutputWithDecoys` that
/// `OutputWithDecoys::new` made. So a transaction built from either is
/// built the same way from there on.
#[tokio::test]
async fn the_light_path_s_ring_is_the_node_path_s() {
    for _ in 0..5 {
        let real = coin(150_000 + u64::from(OsRng.next_u32() % 40_000));
        let picker = Picker { real: (real.index_on_blockchain(), [real.key(), real.commitment().commit()]) };
        let theirs = OutputWithDecoys::new(&mut OsRng, &picker, RING_LEN, 2_000, real.clone()).await.unwrap();
        let positions = theirs.decoys().positions();
        let given = lws::RandomOutputs {
            amount: lws::Amount(0),
            outputs: positions
                .iter()
                .map(|i| {
                    let [key, commitment] = picker.at(*i);
                    lws::RandomOutput { global_index: lws::Amount(*i), public_key: lws::Hex(key.compress().to_bytes()), rct: lws::HexBytes(commitment.compress().to_bytes().to_vec()) }
                })
                .collect(),
        };
        let ours = ring(&real, &given, &mut OsRng).unwrap();
        assert_eq!(ours.serialize(), theirs.serialize());
    }
}

/// The wallet in `LIVE_SPEND`/`LIVE_VIEW` (a stagenet test wallet with
/// money ready, already known to the server at `LWS_URL`), synced through
/// the server from `LIVE_FROM`, its proofs checked with `LIVE_NODE` (Fob's
/// default otherwise), pays 0.01 XMR to its own subaddress 1 through the
/// server; then polls until the server lists the payment in its pool, and
/// until it's mined, for `LIVE_WAIT` seconds at most (default 1,800).
#[tokio::test]
#[ignore = "needs monokulo-lws on stagenet at LWS_URL, a funded test wallet's keys, and the network"]
async fn live_a_payment_from_light_mode() {
    let url = std::env::var("LWS_URL").unwrap_or_else(|_| "http://127.0.0.1:18443".into());
    let spend = std::env::var("LIVE_SPEND").expect("LIVE_SPEND");
    let view = std::env::var("LIVE_VIEW").expect("LIVE_VIEW");
    let from: u64 = std::env::var("LIVE_FROM").ok().and_then(|f| f.parse().ok()).expect("LIVE_FROM");
    let wait: u64 = std::env::var("LIVE_WAIT").ok().and_then(|w| w.parse().ok()).unwrap_or(1_800);
    let keys = Keys::from_spend_key(Network::Stagenet, &spend, Some(&view)).unwrap();
    let wallet = Wallet::new(&dir("light-live-send"), [1; 32], keys, Some(from)).unwrap();
    if let Ok(node) = std::env::var("LIVE_NODE") {
        wallet.inner.state.lock().await.node = node;
    }
    println!("address {}", wallet.primary_address());
    wallet.set_light_server(&url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    for poll in 1..=12 {
        let result = wallet.sync_step().await;
        live_report(&wallet, &format!("poll {poll}: {result:?}")).await;
        if result.is_ok() && wallet.status().synced {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    assert!(wallet.status().synced);
    println!("balance before {:?}", wallet.balance().await);

    let to = wallet.inner.keys.address(1);
    let prepared = wallet.prepare(&to, XMR / 100, Some("Light mode's first payment".into()), None).await.unwrap();
    println!("prepared: 0.01 XMR to {to}, fee {} XMR, fee rate {:?}", fob_monero::amount::format(prepared.fee), *wallet.inner.fee_rate.lock().unwrap());
    let hash = wallet.send(prepared.id).await.unwrap();
    println!("submitted through the server: {hash}");
    live_report(&wallet, "sent").await;

    let client = wallet.light_client().await.unwrap();
    let login = wallet.light_login();
    let started = Instant::now();
    let (mut pooled, mut mined) = (false, None);
    while started.elapsed() < Duration::from_secs(wait) && mined.is_none() {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if !pooled
            && let Ok(listed) = client.get_address_txs(&login).await
            && listed.transactions.iter().any(|t| t.mempool && hex::encode(t.hash.0) == hash)
        {
            pooled = true;
            println!("[{} s] the server lists it in its pool", started.elapsed().as_secs());
        }
        let result = wallet.sync_step().await;
        mined = wallet.inner.state.lock().await.sent.iter().find(|s| s.tx == hash).and_then(|s| s.height);
        if mined.is_some() || started.elapsed().as_secs() % 120 < 10 {
            live_report(&wallet, &format!("[{} s] {result:?}", started.elapsed().as_secs())).await;
        }
    }
    assert!(pooled, "the server listed the payment in its pool");
    let height = mined.expect("mined within LIVE_WAIT");
    println!("mined at {height}");
    // One more poll, so the change is seen as a coin.
    wallet.sync_step().await.unwrap();
    live_report(&wallet, "after").await;
    println!("balance after {:?}", wallet.balance().await);
    println!("light server screen: {:?}", wallet.light_standing().await);
    let state = wallet.inner.state.lock().await;
    let sent = state.sent.iter().find(|s| s.tx == hash).unwrap();
    assert_eq!((sent.amount, sent.fee), (XMR / 100, prepared.fee));
    assert!(state.spends.iter().any(|s| s.tx == hash && s.fee == prepared.fee), "the server's spend with its fee");
    drop(state);
    for row in wallet.said_lists(clock_now()).await.days.into_iter().flat_map(|d| d.rows).filter(|r| r.activity.as_ref().is_some_and(|a| a.tx == hash)) {
        println!("activity: {row:?}");
    }
}

/// A payment of this wallet's the server has neither mined nor in its pool
/// a day after it was sent is marked not confirmed, as the node path marks
/// one its node never heard of; one in the pool isn't; mined after all, it
/// is confirmed again.
#[test]
fn a_payment_the_server_never_saw_is_marked_not_confirmed() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let day = 24 * 3600;
    state.sent.push(Sent { time: 1_000, ..sent_payment(&hex::encode([7; 32])) });
    state.sent.push(Sent { time: 1_000, ..sent_payment(&hex::encode([8; 32])) });
    reconcile(keys, &mut state, &answer(vec![], 200), &HashMap::new(), 1_000 + day - 1).unwrap();
    assert!(state.sent.iter().all(|s| !s.not_confirmed), "not yet a day");
    let pooled = listed([8; 32], None, vec![], vec![]);
    let done = reconcile(keys, &mut state, &answer(vec![pooled], 200), &HashMap::new(), 1_000 + day).unwrap();
    assert!(done.changed);
    assert_eq!(state.sent.iter().map(|s| s.not_confirmed).collect::<Vec<_>>(), vec![true, false], "the one in the pool is on its way");
    let mined = listed([7; 32], Some(150), vec![], vec![]);
    reconcile(keys, &mut state, &answer(vec![mined], 200), &HashMap::new(), 1_000 + 2 * day).unwrap();
    assert_eq!((state.sent[0].not_confirmed, state.sent[0].height), (false, Some(150)), "mined after all");
}
