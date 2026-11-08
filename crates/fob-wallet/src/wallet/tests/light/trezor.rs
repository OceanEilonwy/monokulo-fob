//! A Trezor wallet syncing and sending through a light wallet server: the
//! device gives the key images, the server's spends are held to them, and
//! a payment the device signs goes to `/submit_raw_tx`. Against the fake
//! server of `send.rs` and the software Trezor of `hardware.rs`; no node.

use super::send::{SendServer, send_server, wallet_coin_tx};
use super::*;
use crate::trezor::session::fake::Person;
use crate::wallet::tests::hardware::fake_trezor;

/// A Trezor wallet holding `full`'s keys on the device, syncing through
/// `server`, which lists `coins` (each paid in its own transaction at
/// 2,249,000, global indices from 5,000,000), scanned to 2,249,100. Its
/// node can't be reached: nothing here may need one.
async fn trezor_through(name: &str, server: &SendServer, full: &Keys, coins: &[(u64, u32)]) -> (Wallet, Vec<Paid>) {
    let wallet = Wallet::restore_trezor(&dir(name), [1; 32], Network::Stagenet, &fake_trezor(full), &Person::new(), Some(5)).unwrap();
    wallet.inner.state.lock().await.node = "http://127.0.0.1:9".into();
    let mut listed_coins = vec![];
    let mut paid = vec![];
    for (i, &coin) in coins.iter().enumerate() {
        let (_, hash, p) = transaction(&wallet.inner.keys, Some(coin), &[]);
        let p = p.unwrap();
        listed_coins.push(listed(hash, Some(2_249_000), vec![received(&p, 5_000_000 + i as u64)], vec![]));
        paid.push(p);
    }
    server.lists(listed_coins, 2_249_100);
    wallet.set_light_server(&server.url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    wallet.sync_step().await.unwrap();
    (wallet, paid)
}

/// The device's sync, as Home's "Connect your Trezor" runs it.
async fn device_sync(wallet: &Wallet, full: &Keys) -> u32 {
    wallet.trezor_sync(Arc::new(fake_trezor(full)), Arc::new(Person::new())).await.unwrap()
}

/// What the server lists of the coins, as it keeps listing them.
fn coins_listed(server: &SendServer, paid: &[Paid]) -> Vec<lws::Transaction> {
    paid.iter().map(|p| wallet_coin_tx(p, server)).collect()
}

/// Synced through the server, a Trezor wallet's coins keep their
/// transaction key (the server's `tx_pub_key`, which is `r·D` for a
/// subaddress) and wait for the device. The device's sync gives their key
/// images, which the server's spends are then held to, without a node: a
/// spend in a block marks its coin spent, with its Activity row; one only
/// in the pool marks it tentatively, let go when the pool forgets it; a
/// spend whose key image isn't one of the wallet's marks nothing.
#[tokio::test]
async fn a_trezor_wallet_finds_its_spends_through_the_server() {
    let server = send_server().await;
    let full = Keys::generate(Network::Stagenet).keys;
    let (wallet, paid) = trezor_through("trezor-light-sync", &server, &full, &[(XMR, 0), (XMR / 2, 3), (XMR / 4, 0)]).await;
    {
        let state = wallet.inner.state.lock().await;
        assert_eq!(state.outputs.len(), 3);
        for (output, p) in state.outputs.iter().zip(&paid) {
            assert_eq!(output.tx_key.as_deref(), Some(hex::encode(p.tx_key).as_str()), "the server's transaction key");
            assert_eq!(output.coin, Coin::default(), "no spend key here to make it");
        }
        assert_eq!(state.outputs[1].subaddress, 3);
    }
    assert_eq!(wallet.trezor_unsynced().await, 3, "Home asks for the Trezor");
    assert!(matches!(wallet.prepare(&full.address(9), XMR / 10, None, None).await, Err(FobError::TrezorNeeded)));

    // Spent elsewhere meanwhile, as the server lists it: the first coin in
    // a block, the second in the pool; and a spend of somebody else's.
    let images: Vec<[u8; 32]> = paid.iter().map(|p| image_of(&full, p)).collect();
    let mined = [0x51; 32];
    let pooled = [0x52; 32];
    let mut spends = coins_listed(&server, &paid);
    spends.push(listed(mined, Some(2_249_050), vec![], vec![spend_of(&paid[0], images[0])]));
    spends.push(listed(pooled, None, vec![], vec![spend_of(&paid[1], images[1])]));
    spends.push(listed([0x53; 32], Some(2_249_060), vec![], vec![spend_of(&paid[2], [0x77; 32])]));
    spends[3].fee = Some(lws::Amount(30_000_000));
    server.lists(spends.clone(), 2_249_100);

    assert_eq!(device_sync(&wallet, &full).await, 3);
    assert_eq!(wallet.trezor_unsynced().await, 0);
    let mined_hex = hex::encode(mined);
    {
        let state = wallet.inner.state.lock().await;
        let given: Vec<_> = state.outputs.iter().map(|o| o.coin.key_image().unwrap()).collect();
        assert_eq!(given, images, "the device's key images, the subaddress's too");
        assert_eq!(state.outputs[0].coin, Coin::Spent { ki: KeyImage::Known(images[0]), tx: mined, height: 2_249_050 }, "spent in a block: by that transaction");
        assert_eq!(state.outputs[1].coin, Coin::Held { ki: KeyImage::Known(images[1]), hold: Hold::InPool { tx: pooled } }, "spent in the pool only: held by that transaction");
        assert!(state.outputs[2].coin.is_free(), "somebody else's key image");
        assert_eq!(state.spends.iter().map(|s| (s.tx.as_str(), s.fee)).collect::<Vec<_>>(), vec![(mined_hex.as_str(), 30_000_000)]);
    }
    let rows: Vec<_> = wallet.said_lists(clock_now()).await.days.into_iter().flat_map(|d| d.rows).filter(|r| r.activity.as_ref().is_some_and(|a| a.tx == mined_hex)).collect();
    assert_eq!(rows.len(), 1, "the spend has its Activity row: {rows:?}");

    // The same answer again changes nothing.
    let saved = found(&*wallet.inner.state.lock().await);
    wallet.sync_step().await.unwrap();
    assert_eq!(found(&*wallet.inner.state.lock().await), saved);

    // The pool forgot the second spend: the coin is free again.
    spends.retain(|t| t.hash.0 != pooled);
    server.lists(spends.clone(), 2_249_101);
    wallet.sync_step().await.unwrap();
    assert!(wallet.inner.state.lock().await.outputs[1].coin.is_free());
    // In the pool again, then mined: spent by that transaction for good.
    spends.push(listed(pooled, None, vec![], vec![spend_of(&paid[1], images[1])]));
    server.lists(spends.clone(), 2_249_102);
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.outputs[1].coin.tx(), Some(pooled));
    assert!(!wallet.inner.state.lock().await.outputs[1].coin.is_free());
    spends.pop();
    spends.push(listed(pooled, Some(2_249_102), vec![], vec![spend_of(&paid[1], images[1])]));
    server.lists(spends, 2_249_103);
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.outputs[1].coin, Coin::Spent { ki: KeyImage::Known(images[1]), tx: pooled, height: 2_249_102 });
    assert_eq!(wallet.inner.state.lock().await.spendable(), XMR / 4, "only the third coin is left");
}

/// A Trezor payment through the server: prepared from the server's fee and
/// rings (the fee for 48 bytes more, decision 363), signed by the device,
/// handed to `/submit_raw_tx`, recorded with that fee as the software path
/// records it, then seen in the server's pool and mined.
#[tokio::test]
async fn a_trezor_payment_through_the_server_is_signed_submitted_and_recorded() {
    let server = send_server().await;
    let full = Keys::generate(Network::Stagenet).keys;
    let (wallet, paid) = trezor_through("trezor-light-send", &server, &full, &[(XMR, 0)]).await;
    device_sync(&wallet, &full).await;
    let to = Keys::generate(Network::Stagenet).keys.primary_address();
    let prepared = wallet.prepare(&to, XMR / 10, Some("Lunch".into()), None).await.unwrap();
    // Normal's 80,000 a byte from the server's `fees`, the extra weight's included.
    assert!(prepared.fee > 80_000 * 1_000 && prepared.fee < 80_000 * 3_000, "{}", prepared.fee);
    assert_eq!(prepared.fee % 10_000, 0, "rounded to the server's mask");
    assert!(matches!(wallet.send(prepared.id).await, Err(FobError::TrezorNeeded)), "the device signs it, not this phone");
    let device = fake_trezor(&full);
    let hash = wallet.send_with_trezor(prepared.id, Arc::new(device), Arc::new(Person::new())).await.unwrap();

    // What the server was handed: the device's transaction, spending the
    // coin with a ring of sixteen around its place on the chain.
    let handed = server.submitted.lock().unwrap().clone();
    assert_eq!(handed.len(), 1);
    let tx = monero_wallet::transaction::Transaction::<monero_wallet::transaction::NotPruned>::read(&mut handed[0].as_slice()).unwrap();
    assert_eq!(hex::encode(tx.hash()), hash);
    let monero_wallet::transaction::Input::ToKey { key_offsets, key_image, .. } = &tx.prefix().inputs[0] else { panic!("a RingCT input") };
    assert_eq!(key_image.to_bytes(), image_of(&full, &paid[0]), "the coin's own key image, from the device");
    let positions: Vec<u64> = key_offsets.iter().scan(0, |at, o| { *at += o; Some(*at) }).collect();
    assert_eq!(positions.len(), usize::from(RING_LEN));
    assert!(positions.contains(&5_000_000));
    let monero_wallet::transaction::Transaction::V2 { proofs: Some(proofs), .. } = &tx else { panic!("signed") };
    assert_eq!(proofs.base.fee, prepared.fee, "the fee Confirm showed is the one signed");

    {
        let state = wallet.inner.state.lock().await;
        assert!(state.outputs[0].spent_by(&hash));
        let sent = state.sent.iter().find(|s| s.tx == hash).unwrap();
        assert_eq!((sent.amount, sent.fee, sent.to.as_str(), sent.note.as_deref()), (XMR / 10, prepared.fee, to.as_str(), Some("Lunch")));
        assert_eq!(sent.change, XMR - XMR / 10 - prepared.fee);
        assert_eq!((sent.height, sent.signed.as_ref()), (None, None));
    }

    // In the server's pool, then mined.
    let hash_bytes: [u8; 32] = hex::decode(&hash).unwrap().try_into().unwrap();
    server.lists(vec![wallet_coin_tx(&paid[0], &server), listed(hash_bytes, None, vec![], vec![spend_of(&paid[0], image_of(&full, &paid[0]))])], 2_249_101);
    wallet.sync_step().await.unwrap();
    assert!(wallet.inner.pool.lock().unwrap().is_empty(), "its own payment, not money coming in");
    assert!(wallet.inner.state.lock().await.outputs[0].spent_by(&hash), "still held by it, our payment");
    let mut mined = listed(hash_bytes, Some(2_249_102), vec![], vec![spend_of(&paid[0], image_of(&full, &paid[0]))]);
    mined.fee = Some(lws::Amount(prepared.fee));
    server.lists(vec![wallet_coin_tx(&paid[0], &server), mined], 2_249_103);
    wallet.sync_step().await.unwrap();
    let state = wallet.inner.state.lock().await;
    assert_eq!(state.sent[0].height, Some(2_249_102));
    assert_eq!(state.spends.iter().map(|s| (s.tx.as_str(), s.fee)).collect::<Vec<_>>(), vec![(hash.as_str(), prepared.fee)]);
    drop(state);
    let rows: Vec<_> = wallet.said_lists(clock_now()).await.days.into_iter().flat_map(|d| d.rows).filter(|r| r.activity.as_ref().is_some_and(|a| a.tx == hash)).collect();
    assert_eq!(rows.len(), 1, "one row for the payment: {rows:?}");
}

/// The server's node turning a Trezor payment down: its reason said,
/// nothing recorded, the coin still free for the next try.
#[tokio::test]
async fn a_trezor_payment_the_server_refuses_is_not_recorded() {
    let server = send_server().await;
    let full = Keys::generate(Network::Stagenet).keys;
    let (wallet, _) = trezor_through("trezor-light-refused", &server, &full, &[(XMR, 0)]).await;
    device_sync(&wallet, &full).await;
    *server.submit.lock().unwrap() = (422, r#"{"status":"Failed","reason":"double spend","not_relayed":true}"#.into());
    let to = Keys::generate(Network::Stagenet).keys.primary_address();
    let prepared = wallet.prepare(&to, XMR / 10, None, None).await.unwrap();
    let refused = wallet.send_with_trezor(prepared.id, Arc::new(fake_trezor(&full)), Arc::new(Person::new())).await.unwrap_err();
    assert_eq!(refused.to_string(), "The light server's node turned the payment down: double spend");
    assert_eq!(server.submitted.lock().unwrap().len(), 1, "it was handed over");
    let state = wallet.inner.state.lock().await;
    assert!(state.sent.is_empty());
    assert!(state.outputs[0].coin.is_free());
    drop(state);
    // Prepared again, it goes.
    *server.submit.lock().unwrap() = (200, r#"{"status":"OK"}"#.into());
    let prepared = wallet.prepare(&to, XMR / 10, None, None).await.unwrap();
    let hash = wallet.send_with_trezor(prepared.id, Arc::new(fake_trezor(&full)), Arc::new(Person::new())).await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.sent[0].tx, hash);
}
