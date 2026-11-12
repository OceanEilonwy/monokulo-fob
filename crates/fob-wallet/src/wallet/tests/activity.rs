//! Activity built once per change: the same rows as before, and how long
//! it takes over a long history.

use super::*;

/// A transaction hash in hex, from a kind and a number.
fn hash_of(kind: u8, n: u32) -> String {
    let mut hash = [0u8; 32];
    hash[0] = kind;
    hash[1..5].copy_from_slice(&n.to_le_bytes());
    hash[31] = 0x5a;
    hex::encode(hash)
}

/// A wallet with a long history: `contacts` contacts (each with a photo),
/// `requests` requests (most for a contact), and about `rows` payments:
/// payments in (some to requests, some in two pieces), payments out (a
/// third with a receipt, most to contacts, every tenth a split) and
/// spends a restore found on the chain. A few are still in the pool.
async fn long_history(path: &Path, rows: u32, contacts: u32, requests: u32) -> Wallet {
    let wallet = Wallet::create(path, [1; 32], Network::Stagenet).unwrap();
    let others = Keys::generate(Network::Stagenet).keys;
    let photo = fob_codes::names::test_webp(6_000, 96);
    let mut ids = vec![];
    for i in 0..contacts {
        let draft = contacts::Draft { name: format!("Person {i}"), note: Some(format!("Note {i}")), photo: Some(photo.clone()), send_address: Some(others.address(i + 1)), also_paid_at: vec![] };
        ids.push(wallet.add_contact(draft).await.unwrap().id);
    }
    let receipt = r##"{"orf_version":"0.1.0","receipt":{"receipt_id":"#1042","issue_timestamp":"2026-10-31T09:12:00Z","receipt_type":"sale"},"merchant":{"legal_name":"Corner Café Ltd","display_name":"Corner Café","country_code":"US"},"line_items":[{"description":"Flat white","quantity":2,"unit_price":4.5,"line_total":9.0},{"description":"Almond croissant","quantity":1,"unit_price":4.5,"line_total":4.5}],"tax":{"tax_lines":[{"rate":0.1,"tax_amount":1.23}]},"total":13.5,"currency":"USD"}"##;
    let mut state = wallet.inner.state.lock().await;
    for r in 0..requests {
        let subaddress = 1_000 + r;
        let contact = (r % 4 != 0).then(|| ids[(r as usize) % ids.len().max(1)]).filter(|_| !ids.is_empty());
        let draft = requests::Draft { subaddress, address: wallet.inner.keys.address(subaddress), contact, note: Some(format!("Request {r}")), asked: None, name: None };
        state.requests.add(draft, 1_700_000_000 + u64::from(r)).unwrap();
    }
    let base = 1_700_000_000u64;
    let (received, sent, restored) = (rows * 6 / 10, rows * 3 / 10, rows / 10);
    for i in 0..received {
        let tx = hash_of(1, i);
        let subaddress = if i % 7 == 0 && requests > 0 { 1_000 + i % requests } else { 1 + i % 50 };
        let height = 100_000 + u64::from(i);
        let time = base + u64::from(i) * 600;
        let spent = |n: u32| Coin::Spent { ki: KeyImage::Known([1; 32]), tx: tx_hash_bytes(&hash_of(if n.is_multiple_of(3) { 3 } else { 2 }, n / 2)), height: height + 50 };
        let mut output = Owned { subaddress, height, ..received_output(&tx, time, XMR / 100 + u64::from(i)) };
        // Every other coin was spent since, by a payment out or a spend found.
        if i % 2 == 0 {
            output = output.with_coin(spent(i));
        }
        state.outputs.push(output);
        if i % 5 == 0 {
            state.outputs.push(Owned { subaddress: subaddress + 1, height, key: format!("{tx}-2"), output: "2".into(), ..received_output(&tx, time, XMR / 1_000) });
        }
    }
    for i in 0..sent {
        let tx = hash_of(2, i);
        let to = if ids.is_empty() || i % 5 == 0 { others.address(10_000 + i % 40) } else { others.address(1 + i % contacts) };
        let height = 100_100 + u64::from(i) * 2;
        state.sent.push(Sent {
            not_confirmed: false,
            released: false,
            tx: tx.clone(),
            to,
            amount: XMR / 50,
            fee: 30_000,
            change: XMR / 200,
            time: base + 300 + u64::from(i) * 1_200,
            height: Some(height),
            receipt_view: state::receipt_view((i % 3 == 0).then_some(receipt)),
            receipt: (i % 3 == 0).then(|| receipt.to_owned()),
            note: (i % 4 == 0).then(|| "Lunch".to_owned()),
            pieces: (i % 10 == 9).then_some(4),
            back: vec![],
            recipient_name: None,
            signed: None,
        });
        // The change came back.
        state.outputs.push(Owned { subaddress: 0, height, key: format!("{tx}-c"), ..received_output(&tx, base + 300 + u64::from(i) * 1_200, XMR / 200) });
    }
    for i in 0..restored {
        let tx = hash_of(3, i);
        state.spends.push(ChainSpend { tx, height: 100_200 + u64::from(i) * 3, time: base + 900 + u64::from(i) * 1_800, fee: 25_000 });
    }
    state.tip = 100_000 + u64::from(rows) * 3;
    state.next_block = state.tip;
    state.prices.currency = Some("USD".into());
    for i in (0..received).step_by(3) {
        state.price_records.entry(hash_of(1, i)).or_default().push(fiat::Record { currency: "USD".into(), price: 160_000_000, provider: "Kraken".into(), at: base, daily: true });
    }
    wallet.save_state(&state).unwrap();
    drop(state);
    let mut pool = wallet.inner.pool.lock().expect("pool lock");
    for i in 0..5 {
        pool.insert(hash_of(4, i), Pooled { seen: base + u64::from(rows) * 1_000, parts: vec![(1_000 + i % requests.max(1), XMR / 3)] });
    }
    drop(pool);
    wallet
}

fn tx_hash_bytes(hex: &str) -> TxHash {
    hash_bytes(hex).expect("a hash")
}

fn received_output(tx: &str, time: u64, amount: u64) -> Owned {
    received(tx, time, amount)
}

/// The time each of `runs` calls of `f` takes, at best and on average.
async fn timed<F: std::future::Future>(runs: u32, mut f: impl FnMut() -> F) -> (Duration, Duration) {
    let mut best = Duration::MAX;
    let mut total = Duration::ZERO;
    for _ in 0..runs {
        let started = Instant::now();
        std::hint::black_box(f().await);
        let took = started.elapsed();
        best = best.min(took);
        total += took;
    }
    (best, total / runs)
}

/// About 5,000 payments, 200 contacts and 100 requests: how long the
/// activity and the screens' lists take to build. Run with
/// `cargo test --release -p fob-wallet activity_over_a_long_history -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "a measurement, not a check"]
async fn activity_over_a_long_history() {
    let path = dir("long-history");
    crate::wallet::funds::MEASURING.with(|m| m.set(true));
    let wallet = long_history(&path, 5_000, 200, 100).await;
    let clock = clock_now();
    let rows = wallet.activity().await.len();
    let (best, mean) = timed(20, || wallet.activity()).await;
    println!("activity(): {rows} rows, best {best:?}, mean {mean:?}");
    // Built afresh, as after any change.
    let (best, mean) = timed(20, || async {
        let state = wallet.inner.state.lock().await;
        let pool = wallet.inner.pool.lock().expect("pool lock");
        crate::wallet::funds::build(&state, &pool).len()
    })
    .await;
    println!("activity built afresh: best {best:?}, mean {mean:?}");
    let (best, mean) = timed(20, || wallet.lists(clock)).await;
    println!("lists(): best {best:?}, mean {mean:?}");
    let (best, mean) = timed(20, || wallet.contacts("", clock)).await;
    println!("contacts(): best {best:?}, mean {mean:?}");
    // A new block: only the confirmations change.
    let (best, mean) = timed(20, || async {
        wallet.inner.state.lock().await.tip += 1;
        wallet.lists(clock).await
    })
    .await;
    println!("lists() after a block: best {best:?}, mean {mean:?}");
}

/// Activity as Fob built it before decision 412, word for word but for
/// taking the state and the pool and reading each receipt here: the oracle
/// the one-pass build is held to.
fn old_activity(state: &State, pool: &HashMap<String, Pooled>) -> Vec<Activity> {
    let mut rows = vec![];
    let mut outgoing: HashSet<&str> = HashSet::new();
    for sent in &state.sent {
        outgoing.insert(&sent.tx);
        let receipt = sent.receipt.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok());
        rows.push(Activity {
            not_confirmed: sent.not_confirmed,
            released: sent.released,
            tx: sent.tx.clone(),
            direction: Direction::Out,
            // A split's pieces are still this wallet's: only the fee left.
            amount: if sent.pieces.is_some() { sent.fee } else { sent.amount },
            fee: Some(sent.fee),
            to: Some(sent.to.clone()),
            height: sent.height,
            time: sent.time,
            confirmations: sent.height.map(|h| confirmations(state, h)).unwrap_or(0),
            needed: UNLOCK,
            receipt: receipt.as_ref().map(|r| Arc::new(ReceiptView::from(r))),
            price: None,
            subaddress: None,
            pieces: vec![],
            note: None,
            split: None,
            to_self: false,
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        });
    }
    // Spends found on the chain that this wallet didn't send (a restored
    // wallet): what left is the spent inputs less change and fee. A fee
    // of 0 is one a light wallet server didn't give: not known, so the
    // row claims none and the amount includes it.
    for spend in &state.spends {
        if !outgoing.insert(&spend.tx) {
            continue;
        }
        let spent = amount::total(state.outputs.iter().filter(|o| o.spent_by(&spend.tx)).map(|o| o.amount));
        let change = amount::total(state.outputs.iter().filter(|o| o.tx == spend.tx).map(|o| o.amount));
        rows.push(Activity {
            not_confirmed: false,
            released: false,
            tx: spend.tx.clone(),
            direction: Direction::Out,
            amount: spent.saturating_sub(change).saturating_sub(spend.fee),
            fee: (spend.fee > 0).then_some(spend.fee),
            to: None,
            height: Some(spend.height),
            time: spend.time,
            confirmations: confirmations(state, spend.height),
            needed: UNLOCK,
            receipt: None,
            price: None,
            subaddress: None,
            pieces: vec![],
            note: None,
            split: None,
            to_self: false,
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        });
    }
    let mut incoming: HashMap<&str, Activity> = HashMap::new();
    for output in state.outputs.iter().filter(|o| !outgoing.contains(o.tx.as_str())) {
        let row = incoming.entry(&output.tx).or_insert_with(|| Activity {
            not_confirmed: false,
            released: false,
            tx: output.tx.clone(),
            direction: Direction::In,
            amount: 0,
            fee: None,
            to: None,
            height: Some(output.height),
            time: output.time,
            confirmations: confirmations(state, output.height),
            needed: UNLOCK,
            receipt: None,
            price: None,
            subaddress: None,
            pieces: vec![],
            note: None,
            split: None,
            to_self: false,
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        });
        row.amount = row.amount.saturating_add(output.amount);
        add_part(&mut row.parts, output.subaddress, output.amount);
    }
    rows.extend(incoming.into_values());
    for (tx, pooled) in pool.iter() {
        if !rows.iter().any(|r| r.tx == *tx) {
            rows.push(Activity {
                not_confirmed: false,
                released: false,
                tx: tx.clone(),
                direction: Direction::In,
                amount: pooled.amount(),
                fee: None,
                to: None,
                height: None,
                time: pooled.seen,
                confirmations: 0,
                needed: UNLOCK,
                receipt: None,
                price: None,
                subaddress: pooled.parts.first().map(|p| p.0),
                pieces: vec![],
                note: None,
                split: None,
                to_self: false,
                contact: None,
                contact_id: None,
                shared: None,
                parts: pooled.parts.clone(),
            });
        }
    }
    for row in &mut rows {
        if let Some(code) = state.prices.code() {
            row.price = state.price_records.get(&row.tx).and_then(|r| r.iter().find(|r| r.currency == code)).cloned();
        }
        match row.direction {
            Direction::In => {
                let outputs: Vec<&Owned> = state.outputs.iter().filter(|o| o.tx == row.tx).collect();
                row.pieces = outputs.iter().map(|o| o.amount).collect();
                row.needed = outputs.iter().map(|o| state.needed(o)).max().unwrap_or(UNLOCK);
                row.subaddress = row.subaddress.or(outputs.first().map(|o| o.subaddress));
                // A request's address: its note, and who it was for.
                match row.subaddress.and_then(|s| state.requests.at(s)) {
                    Some(request) => {
                        row.note = request.note.clone();
                        row.contact_id = state.requests.payer(request.subaddress, &row.tx, row.time);
                        row.contact = row.contact_id.and_then(|id| state.contacts.get(id)).map(|c| c.name.clone());
                        row.contact_id = row.contact.as_ref().and(row.contact_id);
                    }
                    None => row.note = row.subaddress.and_then(|s| state.notes.get(&s)).cloned(),
                }
                // A share's address: "Via your profile".
                row.shared = row.subaddress.and_then(|s| state.profile.at(s)).map(|s| s.created);
            }
            Direction::Out => {
                row.pieces = state.outputs.iter().filter(|o| o.spent_by(&row.tx)).map(|o| o.amount).collect();
                let sent = state.sent.iter().find(|s| s.tx == row.tx);
                row.note = sent.and_then(|s| s.note.clone());
                row.split = sent.and_then(|s| s.pieces);
                // Sent to one of its own addresses: more came back than the change, in a block
                // or still in the pool. What came back is still this wallet's; only the fee left.
                if let Some(sent) = sent.filter(|s| s.pieces.is_none()) {
                    let mined = amount::total(state.outputs.iter().filter(|o| o.tx == row.tx).map(|o| o.amount));
                    let pooled = pool.get(&row.tx).map_or(0, Pooled::amount);
                    if mined.max(pooled) > sent.change {
                        row.to_self = true;
                        row.amount = sent.fee;
                    }
                }
                if let Some(owner) = row.to.as_deref().and_then(|to| state.contacts.owner(to)).filter(|_| row.split.is_none()) {
                    row.contact = Some(owner.name.clone());
                    row.contact_id = Some(owner.id);
                }
            }
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse((r.height.is_none(), r.height, r.time)));
    rows
}

/// `rows` in one order whatever order rows of the same block and time came
/// in: the old build took payments in and the pool from a map, in no order.
fn canonical(mut rows: Vec<Activity>) -> Vec<Activity> {
    rows.sort_by(|a, b| {
        let key = |r: &Activity| (std::cmp::Reverse((r.height.is_none(), r.height, r.time)), r.tx.clone(), r.direction == Direction::Out);
        key(a).cmp(&key(b))
    });
    rows
}

/// The old build and the new give the same rows for what `wallet` holds
/// now, and `activity()` gives them too.
async fn same_as_before(wallet: &Wallet, what: &str) {
    let given = wallet.activity().await.to_vec();
    let state = wallet.inner.state.lock().await;
    let pool = wallet.inner.pool.lock().expect("pool lock");
    let old = canonical(old_activity(&state, &pool));
    let new = canonical(crate::wallet::funds::build(&state, &pool));
    assert_eq!(old.len(), new.len(), "{what}: as many rows");
    for (o, n) in old.iter().zip(&new) {
        assert_eq!(o, n, "{what}: row {}", o.tx);
    }
    assert_eq!(canonical(given), new, "{what}: activity() gives them");
}

/// Every kind of row, in one wallet: received in one piece and several, to
/// requests (live, deleted after it came, deleted before, one transaction
/// paying two), to a share and to a noted address, a mined reward and a
/// coin locked to a time; sent with and without receipts, to contacts,
/// splits, to this wallet's own address (mined, and in the pool), not
/// confirmed, released and not yet mined; spends a restore found, with a
/// fee and without (a light wallet server's), one twice; payments in the
/// pool, one of them mined already. Then the same after a rewind, in light
/// mode, with Manual Sync's settled confirmations, and with prices off.
#[tokio::test]
async fn activity_is_the_same_as_before_for_every_kind_of_row() {
    let path = dir("activity-same");
    let wallet = long_history(&path, 300, 12, 10).await;
    let own = wallet.inner.keys.address(7);
    let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), send_address: Some(Keys::generate(Network::Stagenet).keys.address(3)), ..Default::default() }).await.unwrap();
    let mum = wallet.add_contact(contacts::Draft { name: "Mum".into(), ..Default::default() }).await.unwrap();
    wallet.remember_receive_note(40, "Coffee beans").await.unwrap();
    let t = 1_800_000_000;
    {
        let mut state = wallet.inner.state.lock().await;
        let at = |n: u32| 2_000 + n;
        let address = |n: u32| wallet.inner.keys.address(at(n));
        for (n, contact) in [(0, Some(sam.id)), (1, Some(mum.id)), (2, Some(sam.id)), (3, None)] {
            state.requests.add(requests::Draft { subaddress: at(n), address: address(n), contact, note: Some(format!("Ask {n}")), ..Default::default() }, t).unwrap();
        }
        state.profile.share(at(4), address(4), profile::ShareKind::CodeShown, t - 50).unwrap();
        let paid = |tx: &str, n: u32, amount: u64, height: u64| Owned { subaddress: at(n), height, key: format!("{tx}-{n}"), output: n.to_string(), ..received(tx, t + height, amount) };
        let both = tx_hex("both");
        state.outputs.push(paid(&both, 0, XMR, 200_000));
        state.outputs.push(paid(&both, 2, XMR / 2, 200_000));
        state.outputs.push(paid(&tx_hex("to-mum"), 1, XMR, 200_001));
        state.outputs.push(paid(&tx_hex("late"), 3, XMR, 200_002));
        state.outputs.push(paid(&tx_hex("shared"), 4, XMR, 200_003));
        state.outputs.push(Owned { subaddress: 40, ..paid(&tx_hex("noted"), 5, XMR, 200_004) });
        state.outputs.push(Owned { coinbase: true, unlock_block: 200_065, ..paid(&tx_hex("reward"), 6, XMR, 200_005) });
        state.outputs.push(Owned { unlock_block: state::TIME_LOCK_FROM + t + 9_000_000, ..paid(&tx_hex("locked"), 7, XMR, 200_006) });
        // Mum's request deleted with what had come to it; the last one by time.
        let requests: Vec<u64> = state.requests.all().iter().filter(|r| r.subaddress >= at(1)).map(|r| r.id).collect();
        let rows = old_activity(&state, &HashMap::new());
        state.requests.delete(requests[0], t + 300_000, &rows);
        let mut book = state.requests.clone();
        book.delete(requests[2], t, &[]);
        state.requests = book;
        let sent = |tx: &str, to: &str, change: u64, height: Option<u64>| Sent {
            not_confirmed: false,
            released: false,
            tx: tx_hex(tx),
            to: to.into(),
            amount: XMR / 3,
            fee: 40_000,
            change,
            time: t + 500,
            height,
            receipt: None,
            receipt_view: None,
            note: None,
            pieces: None,
            back: vec![],
            recipient_name: None,
            signed: None,
        };
        let sam_address = sam.send_address.clone().unwrap();
        state.sent.push(sent("self-mined", &own, 1, Some(200_010)));
        state.outputs.push(Owned { subaddress: 7, height: 200_010, key: "self".into(), ..received(&tx_hex("self-mined"), t + 500, XMR / 3) });
        state.sent.push(sent("self-pooled", &own, 1, None));
        state.sent.push(Sent { not_confirmed: true, ..sent("quiet", &sam_address, 0, None) });
        state.sent.push(Sent { released: true, ..sent("released", &sam_address, 0, None) });
        state.sent.push(Sent { pieces: Some(3), back: vec![1, 2, 3], ..sent("split", &own, 6, Some(200_011)) });
        state.sent.push(sent("waiting", "86Ux", 0, None));
        state.spends.push(ChainSpend { tx: tx_hex("light"), height: 200_012, time: t + 600, fee: 0 });
        state.spends.push(ChainSpend { tx: tx_hex("light"), height: 200_012, time: t + 600, fee: 0 });
        state.spends.push(ChainSpend { tx: tx_hex("self-mined"), height: 200_010, time: t + 500, fee: 40_000 });
        state.outputs.push(Owned { height: 199_000, key: "spent-light".into(), ..received(&tx_hex("old"), t, XMR) }.with_coin(Coin::Spent { ki: KeyImage::Known([2; 32]), tx: tx_hash("light"), height: 200_012 }));
        state.outputs.push(Owned { height: 199_001, key: "held".into(), ..received(&tx_hex("older"), t, XMR) }.with_coin(held_by_ours("quiet")));
        state.price_records.entry(tx_hex("both")).or_default().push(fiat::Record { currency: "EUR".into(), price: 1, provider: "Kraken".into(), at: t, daily: false });
        state.price_records.entry(tx_hex("both")).or_default().push(fiat::Record { currency: "USD".into(), price: 2, provider: "Kraken".into(), at: t, daily: true });
        state.tip = 200_020;
        state.last_block_time = t + 1_000;
        wallet.save_state(&state).unwrap();
    }
    {
        let mut pool = wallet.inner.pool.lock().expect("pool lock");
        pool.insert(tx_hex("self-pooled"), Pooled { seen: t + 700, parts: vec![(7, XMR / 3)] });
        pool.insert(tx_hex("both"), Pooled { seen: t + 700, parts: vec![(2_000, XMR)] });
        pool.insert(tx_hex("on-its-way"), Pooled { seen: t + 800, parts: vec![(2_000, XMR / 4), (2_002, XMR / 5)] });
    }
    same_as_before(&wallet, "as it is").await;
    // Nothing changed: the same rows, shared.
    assert!(Arc::ptr_eq(&wallet.activity().await, &wallet.activity().await));

    wallet.inner.state.lock().await.manual_sync = true;
    wallet.inner.state.lock().await.settled_confirmations = 30;
    same_as_before(&wallet, "Manual Sync").await;

    wallet.rewind(200_005, None).await.unwrap();
    same_as_before(&wallet, "after a rewind").await;

    wallet.inner.state.lock().await.sync_mode = SyncMode::Light;
    same_as_before(&wallet, "light mode").await;

    wallet.inner.state.lock().await.prices.currency = None;
    same_as_before(&wallet, "prices off").await;

    wallet.inner.pool.lock().expect("pool lock").clear();
    same_as_before(&wallet, "the pool empty").await;

    // Reopened: the receipts read back, the rows the same.
    drop(wallet);
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    same_as_before(&reopened, "reopened").await;
    assert!(reopened.activity().await.iter().any(|r| r.receipt.as_ref().is_some_and(|v| v.merchant == "Corner Café")));
}

/// A wallet whose payments were recorded before receipts were kept read
/// (database version 10) opens with the same activity.
#[tokio::test]
async fn a_version_10_database_opens_with_the_same_activity() {
    let path = dir("activity-v10");
    let wallet = long_history(&path, 200, 5, 5).await;
    wallet.inner.pool.lock().expect("pool lock").clear();
    let before = canonical(wallet.activity().await.to_vec());
    assert!(before.iter().any(|r| r.receipt.is_some()));
    drop(wallet);
    let old = rusqlite::Connection::open(path.with_extension("db")).unwrap();
    old.execute_batch(&format!("PRAGMA key = \"x'{}'\"; ALTER TABLE sent DROP COLUMN receipt_view; PRAGMA user_version = 10;", hex::encode([1u8; 32]))).unwrap();
    drop(old);
    let upgraded = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(canonical(upgraded.activity().await.to_vec()), before);
    assert!(upgraded.inner.state.lock().await.sent.iter().all(|s| s.receipt.is_none() || s.receipt_view.is_some()), "every receipt read");
    same_as_before(&upgraded, "upgraded").await;
}

/// Over a long history too.
#[tokio::test]
async fn a_long_history_is_the_same_as_before() {
    let wallet = long_history(&dir("activity-long-same"), 1_000, 40, 20).await;
    same_as_before(&wallet, "a long history").await;
}
